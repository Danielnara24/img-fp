//! Retrieval: which files a file may be, asked of the corpus's own index.
//!
//! A file asks under one or more variants of itself (`sift::Variant`): the
//! direct pass asks every file as it is, the second look asks a few again
//! mirrored and inverted. Each question is looked up in the inverted file
//! and ranked, and a file's answers are merged into its best `-k` candidates
//! (`Index::candidates`), whatever it asked; what is done with them is the
//! pipeline's. The index also owns what the questions need held: the word
//! lists, the postings between the passes that read them, and the
//! vocabulary for as long as a question may need new words
//! (`Index::let_go`, `Index::turn`).

use crate::analysis::Item;
use crate::index::{InvertedFile, Vocabulary, WordList};
use crate::sift::{Features, Variant};
use crate::{simd, timed};
use rayon::prelude::*;
use std::borrow::Cow;

simd::dispatched! {
    /// Quantise every descriptor of an image into the sorted word list the
    /// inverted file speaks.
    pub(crate) fn quantise(vocab: &Vocabulary, f: &Features) -> WordList => quantise_any;
}

/// `quantise`, for whichever copy `dispatched!` chose.
#[cfg_attr(dispatch, inline(always))]
fn quantise_any(vocab: &Vocabulary, f: &Features) -> WordList {
    let mut buf = Vec::with_capacity(4);
    let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(f.len() * 2);
    for i in 0..f.len() {
        vocab.quantise(f.d(i), &mut buf);
        for &w in buf.iter() {
            pairs.push((w, i as u32));
        }
    }
    pairs.sort_unstable();
    // A keypoint index fits an entry's `KP_BITS`: `retain_best` holds an
    // image to `FEATURES`. `from_sorted` checks it, and the word's bound.
    WordList::from_sorted(&pairs)
}

/// The vocabulary, every matched file's words and their postings: what a
/// file asks of the corpus.
pub(crate) struct Index<'a> {
    items: &'a [Item],
    matched: &'a [bool],
    /// Held while a question may still need words of its own: the second
    /// look's turned features are quantised with it (`turn`).
    vocab: Option<Vocabulary<'a>>,
    n_live: usize,
    lists: Vec<WordList>,
    /// Each list's idf mass, for reading answers both ways round
    /// (`both_ways`): kept against a library only, where not every file asks.
    mass: Option<Vec<f32>>,
    /// Let go between the passes that read them (`let_go`, `turn`).
    postings: Option<InvertedFile>,
    /// The second look's askers' turned word lists, made ahead of asking
    /// when they weigh less than the vocabulary they would keep alive
    /// (`turn`): by asker, in the order the askers were given.
    turned: Option<Vec<[WordList; 3]>>,
}

/// One worker's scratch for `Index::candidates`.
pub(crate) struct Asking {
    acc: Vec<f32>,
    scored: Vec<(u32, f32)>,
    merged: Vec<(u32, f32, u8)>,
}

impl<'a> Index<'a> {
    /// The index of `lists`, quantised with `vocab` from the matched
    /// `items`. `both_ways` when not every file asks (against a library).
    pub(crate) fn new(items: &'a [Item], matched: &'a [bool], vocab: Vocabulary<'a>, lists: Vec<WordList>, both_ways: bool) -> Index<'a> {
        let n_live = vocab.n_live_words();
        let postings = timed!(13, InvertedFile::build(&lists, n_live));
        let mass = both_ways.then(|| lists.iter().map(|wl| postings.mass(wl)).collect());
        Index { items, matched, vocab: Some(vocab), n_live, lists, mass, postings: Some(postings), turned: None }
    }

    pub(crate) fn lists(&self) -> &[WordList] {
        &self.lists
    }

    pub(crate) fn asking(&self) -> Asking {
        Asking { acc: vec![0f32; self.items.len()], scored: Vec::new(), merged: Vec::new() }
    }

    /// File `i`'s best `k` candidates over every list it asks with (`asked`,
    /// its words under each variant it asks as), best first: each candidate
    /// once, under the list it scored highest for (its index in `asked`).
    pub(crate) fn candidates<'s>(&self, i: usize, asked: &[&WordList], k: usize, a: &'s mut Asking) -> &'s [(u32, f32, u8)] {
        let postings = self.postings.as_ref().expect("the postings are held while files ask");
        a.merged.clear();
        for (v, wl) in asked.iter().enumerate() {
            timed!(14, postings.query(wl, i as u32, &mut a.acc, &mut a.scored));
            timed!(41, {
                // Every image the query touched, including one that scores
                // nothing: that happens only when all it shares are words in
                // every image, which only a small folder or one that is all
                // one picture has (see `query_touched`), and on a folder of
                // two it is every word a duplicate shares. Dropping it there
                // found no pair at all.
                a.scored.retain(|&(j, _)| self.matched[j as usize]);
                if let Some(mass) = &self.mass {
                    both_ways(&mut a.scored, postings.mass(wl), mass);
                }
                if crate::pipeline::watch::wants(i) && v == 0 && asked.len() == 1 {
                    let mut all: Vec<f32> = a.scored.iter().map(|x| x.1).collect();
                    all.sort_by(|x, y| y.partial_cmp(x).unwrap());
                    let at = |r: usize| all.get(r - 1).copied().unwrap_or(0.0);
                    crate::pipeline::watch::say(i, i, &format!("scores\t{}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}\t{:.4}", all.len(), at(1), at(10), at(50), at(150), at(300), at(1000), at((all.len() / 2).max(1))));
                }
                if crate::pipeline::watch::wants(i) {
                    for j in crate::pipeline::watch::partners(i) {
                        let at = a.scored.iter().find(|&&(c, _)| c as usize == j).map(|&(_, sc)| sc);
                        let what = match at {
                            Some(sc) => format!("rank\t{v}\t{}\tof {}\tscore {sc:.4}\ttop {:.4}", 1 + a.scored.iter().filter(|&&(_, o)| o > sc).count(), a.scored.len(), a.scored.iter().map(|x| x.1).fold(0f32, f32::max)),
                            None => format!("rank\t{v}\tuntouched\tof {}", a.scored.len()),
                        };
                        crate::pipeline::watch::say(i, j, &what);
                    }
                }
                rank_best(&mut a.scored, k);
            });
            a.merged.extend(a.scored.iter().map(|&(j, s)| (j, s, v as u8)));
        }
        // One list is its own merge.
        if asked.len() > 1 {
            timed!(41, merge_variants(&mut a.merged, k));
        }
        &a.merged
    }

    /// The postings let go until the next pass asks: built again from the
    /// same lists, they are the same postings (the build is deterministic).
    ///
    /// Verification reads the word lists and never the postings, and the
    /// second look, which does, is asked of a few per cent of the files
    /// after verification is done: on a corpus of 27,000 pictures that is
    /// 94 MB off the run's peak, which is the end of verification, for a
    /// rebuild of under two seconds on one thread.
    pub(crate) fn let_go(&mut self) {
        self.postings = None;
    }

    /// Ready for `askers` to ask turned (`Variant::TURNED`): their turned
    /// words made ahead where that lets the vocabulary go, and the postings
    /// built again if anyone asks.
    ///
    /// The vocabulary is wanted only to quantise the askers' turned features,
    /// and the postings only to query them. Where the askers are few (a
    /// benchmark corpus, whose files nearly all found their family) their
    /// word lists are quantised first and the vocabulary let go before the
    /// postings are built again, so the two are never held together: on
    /// 27,000 pictures that is 1,336 files' lists, some 20 MB, against a
    /// 181 MB vocabulary. Where they are most of the corpus (a found one,
    /// where nearly every file is re-asked) their lists would outweigh the
    /// vocabulary several times over, and each file's are made and spent in
    /// turn (`turned`). The words are the same words either way.
    pub(crate) fn turn(&mut self, askers: &[usize]) {
        let vocab = self.vocab.as_ref().expect("the vocabulary is held until the second look");
        let ahead_bytes = askers.iter().map(|&i| self.lists[i].len()).sum::<usize>() * 3 * WordList::BYTES_PER_ENTRY;
        if ahead_bytes < vocab.heap_bytes() {
            let items = self.items;
            self.turned = Some(
                askers
                    .par_iter()
                    .map(|&i| {
                        std::array::from_fn(|v| {
                            let seen = timed!(40, items[i].feats.seen_as(Variant::TURNED[v]));
                            timed!(39, quantise(vocab, &seen))
                        })
                    })
                    .collect(),
            );
            self.vocab = None;
        }
        if !askers.is_empty() {
            self.postings = Some(timed!(13, InvertedFile::build(&self.lists, self.n_live)));
        }
    }

    /// The words of the `at`th asker given to `turn`, whose turned features
    /// are `seen` (`Variant::TURNED`, in its order).
    pub(crate) fn turned(&self, at: usize, seen: &[Features; 3]) -> Cow<'_, [WordList; 3]> {
        match &self.turned {
            Some(t) => Cow::Borrowed(&t[at]),
            None => {
                let vocab = self.vocab.as_ref().expect("the vocabulary is held where the words were not made ahead");
                Cow::Owned(std::array::from_fn(|v| timed!(39, quantise(vocab, &seen[v]))))
            }
        }
    }
}

/// A query's scores read both ways round: for each candidate, the larger of
/// how much of the query it holds and how much of it the query holds.
///
/// `query` scores containment of the query, which finds a crop's original
/// and not an original's crop. A scan of one folder asks both questions by
/// querying from both files, but against a library only the scanned files
/// ask — querying from every library file was the cost the mode exists to
/// avoid — so a photograph whose library copy is a crop would rank that crop
/// by how little of the photograph it holds. The intersection `query`
/// accumulates is the same from either side, so the other reading is the
/// score scaled by the two lists' idf masses (`InvertedFile::mass`), and
/// costs one multiply a candidate.
fn both_ways(scored: &mut [(u32, f32)], query_mass: f32, mass: &[f32]) {
    for (j, s) in scored.iter_mut() {
        let m = mass[*j as usize];
        if m > 0.0 {
            *s = s.max(*s * query_mass / m);
        }
    }
}

/// The `k` best-scoring candidates a query returned, best first, ties to the
/// lowest index.
///
/// The ordering is a total order — the scores may tie but the image indices
/// cannot — so the `k` that come out and the order they come out in are
/// settled by the comparator alone, and selecting before sorting gives the
/// same answer as sorting the lot. It is not the same cost: a query on a
/// corpus of any size touches most of it, so this was sorting some nine
/// thousand candidates to keep a hundred and fifty of them, once per query
/// and three times more for every image the second look re-asks. Measured on
/// a nine-thousand-image corpus it was 2.2 ms a query, which is more than the
/// retrieval it ranks.
///
/// **The selection runs on integers.** A positive finite float's bits order
/// the same way the float does, so `(!score_bits << 32) | index` is one `u64`
/// whose ascending order is exactly the comparator's — score descending, then
/// the lower index — and comparing two of them is one instruction where the
/// float comparator was a `partial_cmp`, an `unwrap` and a tie-break, each a
/// branch the selection mispredicts about half the time. Nearly every score a
/// query hands this is positive — a touched image shares a word, and only a
/// word in every image weighs nothing; anything else takes the comparator, as
/// before.
fn rank_best(scored: &mut Vec<(u32, f32)>, k: usize) {
    let cmp = |a: &(u32, f32), b: &(u32, f32)| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0));
    if k == 0 {
        scored.clear();
        return;
    }
    if scored.len() <= k || !scored.iter().all(|e| e.1 > 0.0 && e.1.is_finite()) {
        if scored.len() > k {
            scored.select_nth_unstable_by(k - 1, cmp);
            scored.truncate(k);
        }
        scored.sort_unstable_by(cmp);
        return;
    }
    RANK_KEYS.with(|keys| {
        let keys = &mut *keys.borrow_mut();
        keys.clear();
        keys.extend(scored.iter().map(|&(j, s)| (!s.to_bits() as u64) << 32 | j as u64));
        keys.select_nth_unstable(k - 1);
        keys.truncate(k);
        keys.sort_unstable();
        scored.clear();
        scored.extend(keys.iter().map(|&key| (key as u32, f32::from_bits(!((key >> 32) as u32)))));
    });
}

thread_local! {
    /// `rank_best`'s keys, kept per worker rather than allocated per query.
    static RANK_KEYS: std::cell::RefCell<Vec<u64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Several variants' candidate lists as one: each candidate once, under the
/// variant it scored highest for, and the best `k` of those in the order
/// `rank_best` uses — score, then the lower index.
///
/// Each list is already its variant's best `k`, and that is enough to rank
/// the merge exactly. A candidate outside its best variant's top `k` has `k`
/// others ahead of it there, and each of those scores at least as well in the
/// merge as it did in that list, so it could not have made the merged `k`
/// either.
fn merge_variants(merged: &mut Vec<(u32, f32, u8)>, k: usize) {
    merged.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)).then(a.2.cmp(&b.2)));
    merged.dedup_by_key(|e| e.0);
    merged.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    merged.truncate(k);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(mut x: u64) -> impl FnMut() -> u64 {
        move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        }
    }

    /// `rank_best` selects on integer keys when it can; it has to choose the
    /// same candidates in the same order as the float comparator, ties and
    /// all, and fall back to the comparator for anything a key cannot carry.
    #[test]
    fn rank_best_keys_order_as_the_comparator_does() {
        let cmp = |a: &(u32, f32), b: &(u32, f32)| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0));
        let mut next = rng(0x2545_f491_4f6c_dd1d);
        for n in [0usize, 1, 5, 149, 150, 151, 400, 3000] {
            for levels in [3u64, 50, 1 << 20] {
                let mut scored: Vec<(u32, f32)> = (0..n as u32).map(|j| (j, 1e-3 + (next() % levels) as f32 * 0.37)).collect();
                // Shuffled, since a query now hands them over in index order.
                for i in (1..scored.len()).rev() {
                    scored.swap(i, (next() % (i as u64 + 1)) as usize);
                }
                for k in [0usize, 1, 7, 150] {
                    let mut want = scored.clone();
                    want.sort_by(cmp);
                    want.truncate(k);
                    let mut got = scored.clone();
                    rank_best(&mut got, k);
                    assert_eq!(got.iter().map(|e| (e.0, e.1.to_bits())).collect::<Vec<_>>(), want.iter().map(|e| (e.0, e.1.to_bits())).collect::<Vec<_>>(), "n {n} levels {levels} k {k}");
                }
            }
        }
        // A score no key can carry goes to the comparator.
        let mut got = vec![(3u32, 0.0f32), (1, 2.0), (2, 2.0)];
        rank_best(&mut got, 2);
        assert_eq!(got, vec![(1, 2.0), (2, 2.0)]);
    }

    /// One variant's ranked list is already its own merge, which is why
    /// `candidates` does not merge it: the merge would hand back the same
    /// list, ties, zero scores and all.
    #[test]
    fn one_ranked_list_is_its_own_merge() {
        let mut next = rng(0x9e37_79b9_7f4a_7c15);
        for n in [0usize, 1, 2, 150, 151, 2000] {
            for levels in [1u64, 3, 1 << 20] {
                for k in [1usize, 7, 150] {
                    let mut scored: Vec<(u32, f32)> = (0..n as u32).map(|j| (j, (next() % levels) as f32 * 0.25)).collect();
                    rank_best(&mut scored, k);
                    let mut merged: Vec<(u32, f32, u8)> = scored.iter().map(|&(j, s)| (j, s, 0)).collect();
                    let want = merged.clone();
                    merge_variants(&mut merged, k);
                    assert_eq!(merged.iter().map(|e| (e.0, e.1.to_bits())).collect::<Vec<_>>(), want.iter().map(|e| (e.0, e.1.to_bits())).collect::<Vec<_>>(), "n {n} levels {levels} k {k}");
                }
            }
        }
    }
}
