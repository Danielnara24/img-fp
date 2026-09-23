//! Finding the few pairs worth verifying, out of the millions that exist.
//!
//! Verification is exact but costs milliseconds; a corpus of n images has
//! n(n-1)/2 pairs. So candidates come from an inverted file over quantised
//! local descriptors — the same structure a text search engine uses, with
//! visual words in place of terms.
//!
//! Two properties matter more than raw retrieval quality:
//!
//! * **It is local.** An image pasted into a slide shares its descriptors with
//!   the original even though the two files look nothing alike globally. No
//!   whole-image hash or embedding can say that, which is why every tool in
//!   `BASELINE.md` scores zero on containment.
//! * **The vocabulary is built from the corpus being searched**, so it adapts
//!   to whatever is in it rather than to what some training set contained.
//!
//! The vocabulary is a hierarchical k-means tree (Nister & Stewenius): a
//! descriptor is quantised by descending it, which costs `branching * depth`
//! comparisons instead of one per word. Descent keeps several paths when the
//! choice is close, because a descriptor sitting near a boundary must not be
//! invisible to its own twin on the other side.

use crate::sift::DESC_LEN;
use rayon::prelude::*;

pub struct VocabParams {
    /// Children per node. Also derived: `for_corpus` starts from the default,
    /// which is the widest the descent's accumulators allow (`MAX_BRANCH`),
    /// and narrows it until the tree is no larger than the corpus needs. Depth
    /// alone can only size a tree to within a factor of itself, and that
    /// factor is sixteen.
    pub branching: usize,
    /// Levels of the tree. Not a constant: `for_corpus` derives it, so that a
    /// folder of eighty photographs and a drive of eighty thousand both get a
    /// vocabulary of the right grain.
    pub depth: usize,
    pub sample: usize,
    pub iters: usize,
    /// Extra descent paths kept when a child is nearly as close as the best.
    pub max_paths: usize,
    pub path_ratio: f32,
    pub seed: u64,
}

/// Descriptors per leaf word the tree aims for.
///
/// Fixing *occupancy* rather than the word count is what makes the vocabulary
/// independent of how many files are being scanned. Every image contributes
/// roughly the same number of features, so a constant number of descriptors
/// per word also fixes the average document frequency of a word — which is the
/// quantity idf weighting and the posting-list cap are both written against.
///
/// The value is not fitted: it is the occupancy the measured build already
/// runs at. The benchmark corpus describes 2,404,926 descriptors into
/// 1,048,576 words, which is 2.29 of them per word, and every configuration
/// that reaches 99.5% precision sits between 2.0 and 2.9.
///
/// It used to be 32, and the tree could not honour it. With `branching` pinned
/// at 16 the only reachable sizes were `16^depth`, so the *achieved* occupancy
/// swung by a factor of sixteen — from 2 just above a step to 32 just below
/// one — and the rule's own target was the far end of that swing. Measured,
/// the far end merges families: at 21 to 30 descriptors per word the tool
/// matches two different beaches through the furniture they share. A corpus
/// reached it by being ordinary — 3,965 files landed at 26 and merged three
/// families — which is why `for_corpus` now narrows the branching instead of
/// rounding the depth up and living with whatever occupancy that lands on.
const DESC_PER_WORD: usize = 3;

impl VocabParams {
    /// The smallest tree that still holds `n_desc/DESC_PER_WORD` leaves.
    ///
    /// Two steps, and the second is the one that matters. The depth is the
    /// shallowest that reaches the target at the widest branching the descent
    /// allows — as before. Then the branching is narrowed to the smallest that
    /// still reaches it at that depth, which is what lets the tree land near
    /// the target instead of wherever the next power of sixteen happens to be.
    ///
    /// Rounding up rather than to the nearest is deliberate and unchanged: too
    /// many words splits a true match across two of them, which multi-path
    /// descent already exists to survive, while too few makes every image
    /// share words with every other and the score stops meaning anything. The
    /// difference is that overshooting now costs at most a factor of two
    /// rather than a factor of sixteen, so "round up" no longer means "accept
    /// any occupancy between 2 and 32".
    ///
    /// The depth is still capped at six levels, so above roughly fifty million
    /// descriptors — something like a hundred thousand images — the tree stops
    /// growing and occupancy climbs again. Nothing here has been run at that
    /// size; if it ever is, this is the first thing to measure.
    pub fn for_corpus(n_desc: usize) -> VocabParams {
        let p = VocabParams::default();
        let target = (n_desc / DESC_PER_WORD).max(2);
        let mut depth = 1usize;
        while depth < 6 && p.branching.pow(depth as u32) < target {
            depth += 1;
        }
        let mut branching = 2usize;
        while branching < p.branching && branching.pow(depth as u32) < target {
            branching += 1;
        }
        VocabParams { depth, branching, ..p }
    }
}

impl Default for VocabParams {
    fn default() -> Self {
        VocabParams {
            branching: 16,
            depth: 4,
            sample: 160_000,
            iters: 8,
            max_paths: 3,
            path_ratio: 1.3,
            seed: 0x5eed_1234_abcd_ef01,
        }
    }
}

/// Hierarchical k-means tree. Nodes are numbered breadth-first per level, so a
/// node's children are `branching` consecutive numbers and a word is just the
/// node number of a leaf.
///
/// Only *live* nodes carry a centre. That distinction is not cosmetic at the
/// sizes this reaches: the tree is sized so that its leaves outnumber the
/// descriptors sampled to train it, and a level of `16^5` nodes would hold
/// 134 MB of centres of which at most a sixth can ever be reached. `slot` maps
/// a node number to its centre, or to `DEAD` for the nodes k-means never
/// populated.
pub struct Vocabulary {
    pub branching: usize,
    pub depth: usize,
    /// `levels[l]` holds the centres of the live nodes of level `l`, in node
    /// order, `DESC_LEN` **bytes** each and one node's dimensions contiguous.
    ///
    /// Bytes, because the descent is waiting for memory rather than for
    /// arithmetic — see `dist2` and `quantise_threads`. A centre is the mean of
    /// descriptor bytes and is stored to the nearest one of them.
    levels: Vec<Vec<u8>>,
    /// Per level, indexed by *parent* node number: where that parent's live
    /// children start in `levels[l]`, and how many there are.
    ///
    /// The descent used to read a dense node-to-centre table and scan a
    /// parent's sixteen slots twice — once to find the first live child, once
    /// to count them — before it could look at a single centre. Both answers
    /// are properties of the tree, settled when it was built; asking them
    /// again for every descriptor in the corpus is the same work several
    /// million times over.
    head: Vec<Vec<Kids>>,
    /// Per level, indexed by centre slot: the node number that centre belongs
    /// to. This is what the dense table was really being consulted for.
    node_of: Vec<Vec<u32>>,
    max_paths: usize,
    path_ratio: f32,
}

/// Where a parent's live children live, and how many.
#[derive(Clone, Copy, Default)]
struct Kids {
    first: u32,
    n: u32,
}

const DEAD: u32 = u32::MAX;

/// Upper bound on the descent frontier, `max_paths * branching`. Asserted at
/// build time so the descent can keep its frontiers in arrays.
const FRONTIER: usize = 64;

/// How many dimensions the k-means assignment loop is handed at a time, and
/// the vector width it is handed them for. See `child_dists`.
const SPAN: usize = 32;
const LANES: usize = 8;

/// Upper bound on `branching`, likewise, so the k-means assignment step's
/// accumulators are an array the compiler can keep in registers.
const MAX_BRANCH: usize = 16;

#[inline]
fn d2(a: &[f32], b: &[f32]) -> f32 {
    let mut s = 0.0;
    for i in 0..DESC_LEN {
        let d = a[i] - b[i];
        s += d * d;
    }
    s
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Ask for the start of one parent's block of centres.
///
/// Two lines is one centre, and four is enough to start the hardware stream
/// prefetcher on the rest — a parent holds at most sixteen of them and they
/// are contiguous. Out of range cannot happen (the block is `n * DESC_LEN`
/// bytes from `first * DESC_LEN`), but the bound is checked anyway because a
/// prefetch of a wild address is a fault on some machines and this one is
/// reached for every descriptor in the corpus.
#[inline]
fn prefetch_centres(centres: &[u8], kids: Kids) {
    if kids.n == 0 {
        return;
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        let start = kids.first as usize * DESC_LEN;
        let end = (start + kids.n as usize * DESC_LEN).min(centres.len());
        let mut at = start;
        let mut lines = 0;
        while at < end && lines < 4 {
            _mm_prefetch(centres.as_ptr().add(at) as *const i8, _MM_HINT_T0);
            at += 64;
            lines += 1;
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = centres;
}

impl Vocabulary {
    /// Words are the leaves: `branching^depth` of them.
    ///
    /// This is the *node numbering*, not the count of words that exist. Almost
    /// none of them do: the tree is trained on a sample of 160,000
    /// descriptors, so at most that many nodes of the deepest level can be
    /// live, out of the two or three million this returns. See
    /// `n_live_words`, which is what the inverted file is built over.
    pub fn n_words(&self) -> usize {
        self.branching.pow(self.depth as u32)
    }

    /// How many words `quantise` can actually return.
    ///
    /// A word is the *centre slot* of a live leaf, not its node number, and on
    /// a real corpus the two differ by an order of magnitude: the found corpus
    /// numbers its leaves up to 1,771,561 and 156,519 of them are live, the
    /// benchmark corpus 537,824 and 139,691. A tree is trained on a sample of
    /// 160,000 descriptors, so no more than that many leaves can ever be live,
    /// however large the numbering grows. Everything downstream is indexed by
    /// word — the document frequencies, the idf, the posting offsets — so
    /// numbering them densely takes those three arrays from twenty-one
    /// megabytes read at random to two, which is the difference between a
    /// cache miss per word of every query and none.
    ///
    /// The two numberings sort the same way, which is what makes the swap
    /// invisible: centres are laid down parent by parent and, within a parent,
    /// child by child, so a leaf's slot rises with its node number. Every
    /// consumer of a word list — the merge in `shared`, the runs the inverted
    /// file is built from — reads only that order.
    pub fn n_live_words(&self) -> usize {
        self.node_of[self.depth - 1].len()
    }

    pub fn build(descriptors: &[u8], p: &VocabParams) -> Vocabulary {
        assert!(p.max_paths * p.branching <= FRONTIER, "vocabulary frontier too small");
        assert!(p.branching <= MAX_BRANCH, "branching wider than k-means' accumulators");
        let n = descriptors.len() / DESC_LEN;
        let mut rng = Rng(p.seed);
        // Sample without replacement, deterministically.
        let take = p.sample.min(n);
        let mut idx: Vec<u32> = (0..n as u32).collect();
        for i in 0..take {
            let j = i + rng.below(n - i);
            idx.swap(i, j);
        }
        let sample: Vec<f32> = idx[..take]
            .iter()
            .flat_map(|&i| {
                descriptors[i as usize * DESC_LEN..(i as usize + 1) * DESC_LEN]
                    .iter()
                    .map(|&v| v as f32)
            })
            .collect();

        let mut levels: Vec<Vec<u8>> = Vec::with_capacity(p.depth);
        let mut heads: Vec<Vec<Kids>> = Vec::with_capacity(p.depth);
        let mut node_ofs: Vec<Vec<u32>> = Vec::with_capacity(p.depth);
        // Which sample belongs to which node of the previous level.
        let mut assign: Vec<u32> = vec![0; take];
        let mut parents = 1usize;

        for _level in 0..p.depth {
            let nodes = parents * p.branching;
            let mut centres: Vec<u8> = Vec::new();
            let mut slot = vec![DEAD; nodes];
            // Group sample indices by parent.
            let mut groups: Vec<Vec<u32>> = vec![Vec::new(); parents];
            for (i, &a) in assign.iter().enumerate() {
                groups[a as usize].push(i as u32);
            }
            let results: Vec<(usize, Vec<f32>, Vec<bool>, Vec<(u32, u32)>)> = groups
                .par_iter()
                .enumerate()
                .map(|(g, members)| {
                    let (c, l, a) = kmeans(&sample, members, p.branching, p.iters, p.seed ^ (g as u64 + 1));
                    (g, c, l, a)
                })
                .collect();
            for (g, c, l, a) in results {
                let base = g * p.branching;
                // One parent's live children go down together, each one's
                // dimensions contiguous, so that the descent reads a child's
                // whole centre as two cache lines. See `quantise`.
                let live: Vec<usize> = (0..p.branching).filter(|&c| l[c]).collect();
                let first = (centres.len() / DESC_LEN) as u32;
                for (k, &child) in live.iter().enumerate() {
                    slot[base + child] = first + k as u32;
                }
                // `c` holds the live centres only, in child order. A centre is
                // a mean of descriptor bytes; it is kept as the nearest byte,
                // which is what makes the descent's reads a quarter of what
                // they were. The deepest level loses nothing at all by it —
                // a node with one member hands that member's own bytes down.
                centres.reserve(live.len() * DESC_LEN);
                for k in 0..live.len() {
                    for i in 0..DESC_LEN {
                        centres.push(quantise_centre(c[k * DESC_LEN + i]));
                    }
                }
                for (i, child) in a {
                    assign[i as usize] = (base + child as usize) as u32;
                }
            }
            centres.shrink_to_fit();
            // The descent's view of this level: one entry per parent saying
            // where its live children start and how many there are, and one
            // entry per centre saying which node it is. Same tree, asked once.
            let mut head = vec![Kids::default(); parents];
            let mut node_of = vec![0u32; centres.len() / DESC_LEN];
            for parent in 0..parents {
                let base = parent * p.branching;
                let mut first = u32::MAX;
                let mut n = 0u32;
                for c in 0..p.branching {
                    let sl = slot[base + c];
                    if sl == DEAD {
                        continue;
                    }
                    if first == u32::MAX {
                        first = sl;
                    }
                    node_of[sl as usize] = (base + c) as u32;
                    n += 1;
                }
                head[parent] = Kids { first: if first == u32::MAX { 0 } else { first }, n };
            }
            levels.push(centres);
            heads.push(head);
            node_ofs.push(node_of);
            parents = nodes;
        }
        Vocabulary {
            branching: p.branching,
            depth: p.depth,
            levels,
            head: heads,
            node_of: node_ofs,
            max_paths: p.max_paths,
            path_ratio: p.path_ratio,
        }
    }

    /// Quantise one descriptor to up to `max_paths` words, best first.
    ///
    /// The frontier never exceeds `max_paths * branching` entries and the
    /// descent runs once per descriptor in the corpus, so both frontiers live
    /// in fixed-size arrays: two heap allocations per descriptor is two per
    /// descriptor too many.
    pub fn quantise(&self, desc: &[u8], out: &mut Vec<u32>) {
        out.clear();
        let q: &[u8; DESC_LEN] = desc[..DESC_LEN].try_into().unwrap();
        // Frontier of (centre slot at this level, distance). The distance is an
        // integer: both sides are bytes, so the sum of 128 squares is exact and
        // at most 8.3 M, and comparing two of them needs no float ordering.
        //
        // The slot, rather than the node number it stands for, because the
        // translation is a random read into an array as long as the level and
        // it was being made for every child scored — a hundred-odd of them per
        // descriptor, against the three that survive. `node_of` is now asked
        // only about the survivors, at the top of the level below, and the
        // word a descriptor quantises to is the slot itself. See
        // `n_live_words` for why that numbering is the one everything after
        // this wants anyway.
        let mut cur = [(0u32, 0u32); FRONTIER];
        let mut next = [(0u32, 0u32); FRONTIER];
        let mut n_cur = 1usize;
        for l in 0..self.depth {
            let mut n_next = 0usize;
            let centres = &self.levels[l];
            let head = &self.head[l];
            // The frontier is rebuilt as the children are scored, so the
            // parents come out of it first. They are at most `max_paths`.
            // This is where a slot becomes the node number `head` is indexed
            // by; the root, which has no level above it, is node zero.
            let mut par = [0u32; FRONTIER];
            if l == 0 {
                par[0] = 0;
            } else {
                let above = &self.node_of[l - 1];
                for i in 0..n_cur {
                    par[i] = above[cur[i].0 as usize];
                }
            }
            // Where each parent's children are, asked for all of them before
            // any of them is measured.
            //
            // A level is three dependent cache misses deep — the slot's node
            // number, that node's entry in `head`, and the block of centres it
            // points at — and the parents' three chains are independent of one
            // another. Walked one parent at a time they do not overlap: a
            // parent's distances are some hundreds of instructions, which is
            // more than the machine can look past, so the second parent's
            // `head` entry is not asked for until the first parent is done
            // with. Reading all the `head` entries first, and handing the
            // prefetcher the head of each block while doing it, puts the three
            // chains alongside each other. The arithmetic is untouched: this
            // only changes when the same bytes are asked for.
            let mut kids = [Kids::default(); FRONTIER];
            for pi in 0..n_cur {
                kids[pi] = head[par[pi] as usize];
                prefetch_centres(centres, kids[pi]);
            }
            // `max_paths` rather than the old `min(max_paths, n_next)`: the
            // two differ only when the level offers fewer children than that,
            // and then the scan never fills up, so the bound is never read.
            let keep = self.max_paths;
            let mut held = 0usize;
            for pi in 0..n_cur {
                let kids = kids[pi];
                let n_live = kids.n as usize;
                if n_live == 0 {
                    continue;
                }
                let first = kids.first as usize;
                // This is the innermost loop of quantisation: it runs
                // `depth * max_paths` times for every descriptor in the
                // corpus, and on a corpus of any size it is most of the
                // retrieval stage. Each child's centre is its own contiguous
                // run of bytes, so a parent's children are read as one stream.
                let blk = &centres[first * DESC_LEN..(first + n_live) * DESC_LEN];
                // Only the closest `max_paths` children are descended, and
                // only their distances are looked at afterwards. Ordering the
                // whole frontier — up to forty-eight entries, once per level
                // for every descriptor in the corpus — decided the order of
                // forty-five nodes about to be discarded, so the survivors are
                // picked out by this scan instead. It reads each distance
                // where the distance is made.
                for (k, c) in blk.chunks_exact(DESC_LEN).enumerate() {
                    if n_next == FRONTIER {
                        break;
                    }
                    let e = ((first + k) as u32, dist2(q, c));
                    next[n_next] = e;
                    n_next += 1;
                    if held == keep && !(e.1 < cur[held - 1].1) {
                        continue;
                    }
                    let mut j = held.min(keep - 1);
                    while j > 0 && e.1 < cur[j - 1].1 {
                        cur[j] = cur[j - 1];
                        j -= 1;
                    }
                    cur[j] = e;
                    held += (held < keep) as usize;
                }
            }
            if n_next == 0 {
                break;
            }
            // The scan gives the sorted order of the smallest three whenever
            // those three, and the boundary between kept and dropped, are
            // unambiguous. When two distances are equal across that boundary
            // the answer is not determined by the distances at all, and which
            // node the tree descends then depends on the sorting algorithm;
            // rather than leave that to chance, such a frontier is handed to
            // the same sort as before. Integer centres make an exact tie less
            // of a rarity than float ones did, which is the one place this
            // matters.
            let mut ambiguous = false;
            for j in 1..held {
                ambiguous |= cur[j].1 == cur[j - 1].1;
            }
            if held < n_next {
                let bound = cur[held - 1].1;
                let mut n_eq = 0usize;
                for i in 0..n_next {
                    n_eq += (next[i].1 == bound) as usize;
                }
                ambiguous |= n_eq > 1;
            }
            if ambiguous {
                let nx = &mut next[..n_next];
                nx.sort_unstable_by_key(|e| e.1);
                cur[..held].copy_from_slice(&next[..held]);
            }
            let cut = cur[0].1 as f32 * self.path_ratio * self.path_ratio + 1.0;
            n_cur = held;
            while n_cur > 1 && (cur[n_cur - 1].1 as f32) > cut {
                n_cur -= 1;
            }
        }
        for &(slot, _) in cur[..n_cur].iter() {
            out.push(slot);
        }
    }
}

/// A centre, as the descent stores it: the nearest byte to the mean.
///
/// The rounding is worth a word, since it is the one lossy step in the
/// vocabulary. A centre is the mean of a cluster of descriptors drawn from a
/// *sample* of the corpus — 160,000 of several million — so its own sampling
/// error is on the order of a whole unit, a hundred times the half-unit this
/// rounding adds. And it adds nothing at all where it would matter most: the
/// deepest level of the tree is mostly nodes of one member, whose centre is
/// that member's own bytes and is exact.
#[inline]
fn quantise_centre(v: f32) -> u8 {
    // `as u8` saturates, and a mean of bytes cannot leave the range anyway.
    v.round() as u8
}

/// Squared distance between a descriptor and a centre, both bytes.
///
/// **Bytes rather than floats is the whole point.** The tree a real corpus
/// builds is a hundred megabytes and the descent reads a random part of it for
/// every descriptor, so what the descent costs is not its arithmetic but its
/// cache lines. Measured by `quantise_threads` against a 93 MB tree: with
/// float centres one core took 7.5 microseconds a descriptor and eight cores
/// took 2.9 each — a per-thread penalty of 3.2, where the same descent against
/// a 2 MB tree paid 2.1. With byte centres the penalty is 1.9 at *every* tree
/// size, which is to say the memory is no longer in the way, and the eight-core
/// figure is 1.2 microseconds.
///
/// A centre in bytes is two cache lines where four floats' worth was eight. It
/// was tried once before and rejected, on the grounds that widening the bytes
/// cost more than the traffic saved — which is true on a tree that fits in
/// cache, and was measured on the corpus where the descent is 9% of the run.
/// It is the arithmetic below that makes the difference: sixteen-bit lanes and
/// a pairwise multiply-add, so the widening costs two shuffles per thirty-two
/// dimensions rather than a conversion per dimension.
///
/// The sum is over integers and cannot overflow — 128 dimensions at most 255
/// apart is 8.3 M — so it is exact whatever order it is taken in, and the
/// portable form below gives the same number as the vector one.
#[cfg(target_feature = "avx2")]
#[inline]
pub fn dist2(q: &[u8; DESC_LEN], c: &[u8]) -> u32 {
    debug_assert!(c.len() >= DESC_LEN && DESC_LEN % 32 == 0);
    unsafe {
        use std::arch::x86_64::*;
        let zero = _mm256_setzero_si256();
        let mut acc = zero;
        for o in (0..DESC_LEN).step_by(32) {
            let a = _mm256_loadu_si256(q.as_ptr().add(o) as *const __m256i);
            let b = _mm256_loadu_si256(c.as_ptr().add(o) as *const __m256i);
            // |a - b| per byte, with no sign to carry: one of the two
            // saturating subtractions is zero.
            let d = _mm256_or_si256(_mm256_subs_epu8(a, b), _mm256_subs_epu8(b, a));
            // Sixteen-bit lanes, then square and sum adjacent pairs.
            let lo = _mm256_unpacklo_epi8(d, zero);
            let hi = _mm256_unpackhi_epi8(d, zero);
            acc = _mm256_add_epi32(acc, _mm256_madd_epi16(lo, lo));
            acc = _mm256_add_epi32(acc, _mm256_madd_epi16(hi, hi));
        }
        let half = _mm_add_epi32(_mm256_castsi256_si128(acc), _mm256_extracti128_si256(acc, 1));
        let pair = _mm_add_epi32(half, _mm_shuffle_epi32(half, 0b00_00_11_10));
        let one = _mm_add_epi32(pair, _mm_shuffle_epi32(pair, 0b00_00_00_01));
        _mm_cvtsi128_si32(one) as u32
    }
}

/// The same distance, in sixteen independent lanes so that a target without
/// `avx2` still does not walk one chain of 128 dependent adds. Integers, so it
/// is the same number the vector form gives.
#[cfg(not(target_feature = "avx2"))]
#[inline]
pub fn dist2(q: &[u8; DESC_LEN], c: &[u8]) -> u32 {
    const LANES: usize = 16;
    let mut acc = [0u32; LANES];
    for (a, b) in q.chunks_exact(LANES).zip(c[..DESC_LEN].chunks_exact(LANES)) {
        for l in 0..LANES {
            let d = a[l] as i32 - b[l] as i32;
            acc[l] += (d * d) as u32;
        }
    }
    let mut s = 0u32;
    for l in 0..LANES {
        s += acc[l];
    }
    s
}

fn child_dists(k: usize, q: &[f32], blk: &[f32], acc: &mut [f32; MAX_BRANCH]) {
    macro_rules! widths {
        ($($w:literal)*) => {
            match k {
                $($w => {
                    let mut a = [0f32; $w];
                    if $w % LANES == 0 {
                        dists::<$w>(q, blk, &mut a);
                    } else {
                        for c in 0..DESC_LEN / SPAN {
                            let d0 = c * SPAN;
                            dists::<$w>(&q[d0..d0 + SPAN], &blk[d0 * $w..(d0 + SPAN) * $w], &mut a);
                        }
                    }
                    acc[..$w].copy_from_slice(&a);
                })*
                // `MAX_BRANCH` is the widest a node can be — asserted where
                // the tree is built — so nothing reaches this. It is here so
                // that the match is total without a panic in a hot loop.
                _ => dists_dyn(k, q, blk, acc),
            }
        };
    }
    widths!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);
}

#[inline(always)]
fn dists<const K: usize>(q: &[f32], blk: &[f32], acc: &mut [f32; K]) {
    for (&qi, row) in q.iter().zip(blk.chunks_exact(K)) {
        for (a, &c) in acc.iter_mut().zip(row.iter()) {
            let d = qi - c;
            *a += d * d;
        }
    }
}

fn dists_dyn(k: usize, q: &[f32], blk: &[f32], acc: &mut [f32; MAX_BRANCH]) {
    for (&qi, row) in q.iter().zip(blk.chunks_exact(k)) {
        for (a, &c) in acc[..k].iter_mut().zip(row.iter()) {
            let d = qi - c;
            *a += d * d;
        }
    }
}

/// k-means on a subset, with k-means++ seeding. Empty clusters are marked
/// dead rather than re-seeded: a vocabulary with fewer live nodes is correct,
/// one with a centre nobody uses is noise.
fn kmeans(data: &[f32], members: &[u32], k: usize, iters: usize, seed: u64) -> (Vec<f32>, Vec<bool>, Vec<(u32, u32)>) {
    assert!(k <= MAX_BRANCH);
    let mut live = vec![false; k];
    let mut assign: Vec<(u32, u32)> = Vec::with_capacity(members.len());
    if members.is_empty() {
        return (Vec::new(), live, assign);
    }
    if members.len() <= k {
        let mut centres = Vec::with_capacity(members.len() * DESC_LEN);
        for (c, &m) in members.iter().enumerate() {
            centres.extend_from_slice(&data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN]);
            live[c] = true;
            assign.push((m, c as u32));
        }
        return (centres, live, assign);
    }
    let mut centres = vec![0f32; k * DESC_LEN];
    let mut rng = Rng(seed | 1);
    // k-means++
    let mut chosen: Vec<u32> = Vec::with_capacity(k);
    chosen.push(members[rng.below(members.len())]);
    let mut best_d: Vec<f32> = members
        .par_iter()
        .map(|&m| {
            d2(
                &data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN],
                &data[chosen[0] as usize * DESC_LEN..(chosen[0] as usize + 1) * DESC_LEN],
            )
        })
        .collect();
    while chosen.len() < k {
        let total: f64 = best_d.iter().map(|&v| v as f64).sum();
        if total <= 0.0 {
            break;
        }
        let mut t = (rng.next() as f64 / u64::MAX as f64) * total;
        let mut pick = members.len() - 1;
        for (i, &v) in best_d.iter().enumerate() {
            t -= v as f64;
            if t <= 0.0 {
                pick = i;
                break;
            }
        }
        let c = members[pick];
        chosen.push(c);
        // Every member's distance to the new centre is independent of every
        // other's. The first level of the tree has one k-means over the whole
        // sample, so this is the one place in the build with no parallelism of
        // its own to fall back on.
        let cd = &data[c as usize * DESC_LEN..(c as usize + 1) * DESC_LEN];
        best_d
            .par_iter_mut()
            .zip(members.par_iter())
            .for_each(|(b, &m)| {
                let d = d2(&data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN], cd);
                if d < *b {
                    *b = d;
                }
            });
    }
    let kk = chosen.len();
    for (c, &m) in chosen.iter().enumerate() {
        centres[c * DESC_LEN..(c + 1) * DESC_LEN]
            .copy_from_slice(&data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN]);
    }
    let mut owner = vec![0u32; members.len()];
    let mut tc = vec![0f32; kk * DESC_LEN];
    for it in 0..iters {
        // Centres transposed, dimension-major, so one pass over a descriptor
        // measures it against all `kk` of them at once. Each centre's sum is
        // still taken dimension by dimension in order, so every distance is
        // the same float it was when they were measured one at a time — but
        // `kk` sums run at once instead of one chain of 128 dependent adds.
        for c in 0..kk {
            for i in 0..DESC_LEN {
                tc[i * kk + c] = centres[c * DESC_LEN + i];
            }
        }
        let changed: usize = owner
            .par_iter_mut()
            .zip(members.par_iter())
            .map(|(own, &m)| {
                let dv = &data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN];
                let mut acc = [0f32; MAX_BRANCH];
                child_dists(kk, dv, &tc, &mut acc);
                let mut best = (f32::MAX, 0u32);
                for c in 0..kk {
                    if acc[c] < best.0 {
                        best = (acc[c], c as u32);
                    }
                }
                let same = *own == best.1;
                *own = best.1;
                !same as usize
            })
            .sum();
        if changed == 0 && it > 0 {
            break;
        }
        let mut sums = vec![0f64; kk * DESC_LEN];
        let mut counts = vec![0u32; kk];
        for (i, &m) in members.iter().enumerate() {
            let c = owner[i] as usize;
            counts[c] += 1;
            let dv = &data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN];
            let s = &mut sums[c * DESC_LEN..(c + 1) * DESC_LEN];
            for j in 0..DESC_LEN {
                s[j] += dv[j] as f64;
            }
        }
        for c in 0..kk {
            if counts[c] == 0 {
                continue;
            }
            let inv = 1.0 / counts[c] as f64;
            for j in 0..DESC_LEN {
                centres[c * DESC_LEN + j] = (sums[c * DESC_LEN + j] * inv) as f32;
            }
        }
    }
    let mut used = vec![0u32; kk];
    for &c in owner.iter() {
        used[c as usize] += 1;
    }
    for c in 0..kk {
        live[c] = used[c] > 0;
    }
    for (i, &m) in members.iter().enumerate() {
        assign.push((m, owner[i]));
    }
    // Only the live centres travel back. The deepest level of the tree has one
    // call per node of the level above — sixty-five thousand of them, averaging
    // two or three samples each — and returning a full k-wide block from every
    // one of those was half a gigabyte of mostly-empty centres held at once.
    let n_live = live.iter().filter(|&&l| l).count();
    if n_live < k {
        let mut compact = Vec::with_capacity(n_live * DESC_LEN);
        for c in 0..kk {
            if live[c] {
                compact.extend_from_slice(&centres[c * DESC_LEN..(c + 1) * DESC_LEN]);
            }
        }
        centres = compact;
    }
    (centres, live, assign)
}

// ------------------------------------------------------------ inverted file

/// Words of one image, sorted, with the keypoint each came from.
#[derive(Clone, Debug, Default)]
pub struct WordList {
    /// Parallel arrays sorted by `word`.
    pub word: Vec<u32>,
    pub kp: Vec<u32>,
}

impl WordList {
    /// Iterate (word, count) over the sorted list.
    pub fn runs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.runs_at().map(|(_, w, c)| (w, c))
    }

    /// The same, with the index each run starts at — which is what the query
    /// needs in order to ask for a later word's postings ahead of time.
    pub fn runs_at(&self) -> Runs<'_> {
        Runs { wl: self, i: 0 }
    }
}

pub struct Runs<'a> {
    wl: &'a WordList,
    i: usize,
}

impl Iterator for Runs<'_> {
    type Item = (usize, u32, u32);
    fn next(&mut self) -> Option<(usize, u32, u32)> {
        if self.i >= self.wl.word.len() {
            return None;
        }
        let at = self.i;
        let w = self.wl.word[at];
        let mut c = 0u32;
        while self.i < self.wl.word.len() && self.wl.word[self.i] == w {
            c += 1;
            self.i += 1;
        }
        Some((at, w, c))
    }
}

/// Postings in one flat run per word (`data[off[w]..off[w + 1]]`), rather than
/// a `Vec` per word. A vocabulary has as many words as the corpus has
/// descriptors over `DESC_PER_WORD`, most of them with a posting or two, so a
/// separate allocation each costs more in headers and allocator slack than in
/// postings — and the query walks one contiguous run instead of chasing a
/// pointer per word.
pub struct InvertedFile {
    off: Vec<u32>,
    data: Vec<(u32, u32)>,
    /// log(N / df), per word.
    idf: Vec<f32>,
}

/// How far down the word list the query asks for its postings, in entries.
///
/// Entries rather than distinct words, because a run is walked without
/// counting it first; a word occurs about twice in a list, so this is three or
/// four words of lead. It wants to be short. A word's postings are a few
/// hundred contiguous bytes and every one of them is read, so the whole run is
/// asked for at once, and a core can only have so many misses outstanding —
/// asking twenty words early would either exceed that or have the lines
/// evicted before the loop arrived. Prefetching a line twice costs nothing, so
/// the imprecision of counting entries does not matter.
const POST_AHEAD: usize = 6;

/// Lines of one posting run to ask for. Eight postings to a line, so this is
/// the first five hundred and twelve of them, which is longer than all but a
/// handful of runs.
const POST_LINES: usize = 8;

/// Ask for the postings of `words[at]`.
///
/// This is the query's whole memory problem. The postings of a word are a
/// short contiguous run somewhere in a structure of tens of megabytes — too
/// short for the hardware prefetcher to lock on to before it ends — and the
/// address of the run is not even known until `off[w]` has arrived, so every
/// word of every query paid two dependent trips to memory. Some thousand words
/// a query, tens of thousands of queries.
#[inline]
fn prefetch_postings(inv: &InvertedFile, words: &[u32], at: usize) {
    let Some(&w) = words.get(at) else { return };
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        let w = w as usize;
        let start = *inv.off.get_unchecked(w) as usize;
        let end = *inv.off.get_unchecked(w + 1) as usize;
        if start >= end {
            return;
        }
        let p = inv.data.as_ptr().add(start) as *const i8;
        let bytes = (end - start) * std::mem::size_of::<(u32, u32)>();
        let mut at = 0usize;
        let mut lines = 0usize;
        while at < bytes && lines < POST_LINES {
            _mm_prefetch(p.add(at), _MM_HINT_T0);
            at += 64;
            lines += 1;
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = (inv, w);
}

impl InvertedFile {
    pub fn build(lists: &[WordList], n_words: usize, max_posting: usize) -> InvertedFile {
        let n = lists.len();
        let mut df = vec![0u32; n_words];
        for wl in lists.iter() {
            for (w, _) in wl.runs() {
                df[w as usize] += 1;
            }
        }
        let mut idf = vec![0f32; n_words];
        let mut off: Vec<u32> = Vec::with_capacity(n_words + 1);
        let mut total = 0u32;
        for w in 0..n_words {
            off.push(total);
            let d = df[w] as usize;
            // A word in a large fraction of the corpus carries no information
            // and costs the most to traverse; dropping it is both faster and
            // more accurate.
            if d == 0 || d > max_posting {
                continue;
            }
            idf[w] = (n as f32 / d as f32).ln();
            total += df[w];
        }
        off.push(total);
        let mut data = vec![(0u32, 0u32); total as usize];
        // The document counts become write cursors, so the images of a word
        // land in the order they are visited: increasing image index, as
        // before.
        let mut cursor = df;
        cursor.copy_from_slice(&off[..n_words]);
        for (img, wl) in lists.iter().enumerate() {
            for (w, c) in wl.runs() {
                let w = w as usize;
                if off[w + 1] == off[w] {
                    continue;
                }
                let at = cursor[w] as usize;
                data[at] = (img as u32, c);
                cursor[w] = at as u32 + 1;
            }
        }
        InvertedFile { off, data, idf }
    }

    /// The idf-weighted fraction of the query's words that also occur in each
    /// other image.
    ///
    /// This is a *containment* measure, not a similarity: it is normalised by
    /// the query alone, so a 200x200 crop scores near 1 against the 4000x3000
    /// photograph it came from. Cosine similarity, which every bag-of-words
    /// retrieval system reaches for first, would score that pair near zero
    /// because the two images have wildly different word counts — and
    /// containment is exactly the case this tool exists to find.
    ///
    /// The query list need not be one of the indexed ones; the mirrored and
    /// inverted passes query with lists that were never indexed.
    pub fn query(
        &self,
        wl: &WordList,
        exclude: u32,
        acc: &mut [f32],
        touched: &mut Vec<u32>,
        out: &mut Vec<(u32, f32)>,
    ) {
        out.clear();
        touched.clear();
        let mut qmass = 0f32;
        let words = &wl.word[..];
        for (at, w, c) in wl.runs_at() {
            let w = w as usize;
            // The postings of a word further down the list, asked for now.
            //
            // A query walks about a thousand words and each one's postings are
            // a few hundred contiguous bytes somewhere in a structure of tens
            // of megabytes — so the run is streamed happily once it starts,
            // and the miss that starts it is the whole cost. Worse, it is a
            // *dependent* miss: `off[w]` has to arrive before the address of
            // the postings is even known. Asking a few words early breaks the
            // chain, and asks for nothing the loop was not about to read.
            prefetch_postings(self, words, at + POST_AHEAD);
            let post = &self.data[self.off[w] as usize..self.off[w + 1] as usize];
            if post.is_empty() {
                continue;
            }
            let idf2 = self.idf[w] * self.idf[w];
            qmass += c as f32 * idf2;
            for &(other, cnt) in post.iter() {
                if other == exclude {
                    continue;
                }
                let a = &mut acc[other as usize];
                if *a == 0.0 {
                    touched.push(other);
                }
                // Histogram intersection: a word occurring three times in the
                // query and once in the other image is one shared landmark,
                // not three.
                *a += cnt.min(c) as f32 * idf2;
            }
        }
        let inv = 1.0 / qmass.max(1e-6);
        for &o in touched.iter() {
            let s = acc[o as usize] * inv;
            acc[o as usize] = 0.0;
            out.push((o, s));
        }
    }
}

/// Whether any word of `a` is also a word of `b`, for two runs of `BLOCK`
/// sorted words.
///
/// All sixty-four comparisons at once: one vector holds `b`'s block, each of
/// `a`'s eight words is broadcast against it, and the masks are collected. The
/// intersection is *sparse* — two images share some tens of words out of
/// thirteen hundred each — so this answers "no" almost every time, and the
/// answer costs about one instruction per word compared against four unusable
/// branches.
#[cfg(target_feature = "avx2")]
#[inline]
fn blocks_meet(a: &[u32], b: &[u32]) -> bool {
    debug_assert!(a.len() >= BLOCK && b.len() >= BLOCK);
    unsafe {
        use std::arch::x86_64::*;
        let bv = _mm256_loadu_si256(b.as_ptr() as *const __m256i);
        let mut acc = _mm256_setzero_si256();
        for k in 0..BLOCK {
            let av = _mm256_set1_epi32(*a.get_unchecked(k) as i32);
            acc = _mm256_or_si256(acc, _mm256_cmpeq_epi32(av, bv));
        }
        _mm256_movemask_epi8(acc) != 0
    }
}

/// On a target without `avx2` there is no filter to run — `BLOCK` is zero and
/// the block phase of `shared` is dead code — and this exists so that the phase
/// still type-checks. Answering "yes, look at these" would also be *correct*
/// there, just pointless: sixty-four scalar comparisons cost more than the eight
/// merge steps they could skip.
#[cfg(not(target_feature = "avx2"))]
#[inline]
fn blocks_meet(_a: &[u32], _b: &[u32]) -> bool {
    true
}

/// Words compared at a time by the block filter, and the width of the vector
/// that does it. Zero disables the filter, which is what a target without
/// `avx2` gets.
#[cfg(target_feature = "avx2")]
const BLOCK: usize = 8;
#[cfg(not(target_feature = "avx2"))]
const BLOCK: usize = 0;

/// Candidate descriptor pairs for two images: keypoints that share a word.
///
/// The intersection is *sparse*: two images' word lists hold about thirteen
/// hundred entries each over a vocabulary of a million-odd words, and share
/// some tens. It is also the matcher's most-called function — five million
/// times in one run of a nine-thousand-image corpus, and four times that on
/// one where most files have no duplicate and the mirrored pass re-asks.
///
/// So the words are merged in two phases. A **block filter** asks whether the
/// next eight words of each side have anything in common at all, and when they
/// do not — which is nearly always — it advances the side whose block ends
/// first by all eight. A **scalar merge** takes over for the block pair that
/// does meet, and it is where the pairs are emitted. The words come out in the
/// same order a plain merge would give them, so `cap` still cuts the same
/// place.
///
/// The two forms this replaces are worth recording, because both were
/// measured. A plain three-way merge steps once per entry of both lists, and
/// every step is a comparison the predictor cannot learn: 43 microseconds a
/// pair. Galloping — skipping to the next candidate rather than walking to it
/// — reduced the *steps* to one per entry of one list and left the branches
/// exactly as unpredictable, which is why it was worth only 4%. Neither was
/// bound by its comparisons; both were bound by being wrong about them.
/// Measured on `shared_timings`, at 1,300 entries over 1.7 M words: 13.9
/// microseconds galloping against 2.5 with the filter.
pub fn shared(a: &WordList, b: &WordList, out: &mut Vec<(u32, u32)>, cap: usize) {
    out.clear();
    // The largest keypoint index emitted, as a running `or`. It decides
    // whether the pairs can be sorted by counting rather than by comparing —
    // see `sort_pairs` — and it costs one integer operation per pair.
    let mut hi = 0u32;
    let (aw, bw) = (&a.word[..], &b.word[..]);
    let (na, nb) = (aw.len(), bw.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < na && j < nb {
        // Block phase. Nothing is emitted here: it only finds the first block
        // pair that could hold a shared word.
        let (mut ia, mut jb) = (i + BLOCK, j + BLOCK);
        while BLOCK > 0 && ia <= na && jb <= nb && !blocks_meet(&aw[i..ia], &bw[j..jb]) {
            // The block that ends first cannot match anything the other side
            // has left, so it goes whole. Equal ends retire both.
            let (am, bm) = (aw[ia - 1], bw[jb - 1]);
            if am <= bm {
                i = ia;
                ia += BLOCK;
            }
            if bm <= am {
                j = jb;
                jb += BLOCK;
            }
        }
        if i >= na || j >= nb {
            break;
        }
        // Scalar phase, over the block pair the filter stopped at — or over
        // the tails, where there is no whole block left to filter. It runs
        // until one of the two windows is used up, which is as far as the
        // filter's own reasoning reaches.
        let (aend, bend) = if BLOCK == 0 { (na, nb) } else { (ia.min(na), jb.min(nb)) };
        while i < aend && j < bend {
            let (av, bv) = (aw[i], bw[j]);
            // Two conditional increments rather than a three-way branch: a
            // shared word is rare enough that the test below predicts, and the
            // ordering of two words that differ does not.
            i += (av < bv) as usize;
            j += (bv < av) as usize;
            if av != bv {
                continue;
            }
            let (i0, j0) = (i, j);
            while i < na && aw[i] == av {
                i += 1;
            }
            while j < nb && bw[j] == av {
                j += 1;
            }
            // A word matching many keypoints on both sides is repeated
            // texture, not a landmark; it would dominate the list without
            // helping the fit.
            if (i - i0) * (j - j0) > 64 {
                continue;
            }
            for x in i0..i {
                for y in j0..j {
                    let e = (a.kp[x], b.kp[y]);
                    hi |= e.0 | e.1;
                    out.push(e);
                }
            }
            if out.len() > cap {
                sort_pairs(out, hi);
                out.dedup();
                return;
            }
        }
    }
    sort_pairs(out, hi);
    out.dedup();
}

/// Bits taken per radix pass, and the buckets that implies. Two passes of ten
/// cover a keypoint index of a thousand, which is more than `max_features`
/// lets an image hold.
const RADIX_BITS: usize = 10;
const RADIX_BUCKETS: usize = 1 << RADIX_BITS;

/// Below this many pairs the comparison sort wins, because a radix pass has to
/// clear and prefix-sum a thousand buckets whatever the list holds. Measured
/// on lists of the shape this function really sees: 56 pairs cost 0.9
/// microseconds comparing and 2.2 counting, 400 cost 7.5 and 4.0, 1,200 cost
/// 23.6 and 10.2, 3,200 cost 66.5 and 23.8. The crossing is near 150.
const RADIX_MIN: usize = 192;

/// Scratch for the counting sort: the buffer it ping-pongs through and the
/// buckets it counts into, kept per thread rather than allocated five million
/// times.
struct SortScratch {
    tmp: Vec<(u32, u32)>,
    cnt: Vec<u32>,
}

thread_local! {
    static SORT_SCRATCH: std::cell::RefCell<SortScratch> =
        const { std::cell::RefCell::new(SortScratch { tmp: Vec::new(), cnt: Vec::new() }) };
}

/// Sort the emitted pairs.
///
/// **This is what `shared` costs**, and it took a bench to see it. The merge
/// that finds the shared words is a couple of microseconds and does not care
/// how much the two images have in common; the list it emits does. The
/// candidates a query returns are by construction the images sharing the most
/// words with it, so the calls that matter are the ones emitting hundreds or
/// thousands of pairs, and at that size the whole call is this sort:
/// `shared_overlap` reads 2.1 microseconds at no shared words and 33.9 at
/// eight hundred, and a comparison sort of the 3,190 pairs those eight hundred
/// emit is 66 microseconds of the 68 a two-thread machine would charge for it.
///
/// A keypoint index is smaller than `max_features`, so a pair is twenty bits
/// and two counting passes put it in order — the same order, since a
/// least-significant-digit radix sort is stable and sorts on the whole key.
/// `hi` is the `or` of every index emitted, so the fast path is taken only
/// when the key really fits; anything wider falls back to comparing, as does
/// anything short enough that clearing the buckets would cost more than the
/// comparisons.
fn sort_pairs(out: &mut Vec<(u32, u32)>, hi: u32) {
    if out.len() < RADIX_MIN || hi >= RADIX_BUCKETS as u32 {
        out.sort_unstable();
        return;
    }
    SORT_SCRATCH.with(|s| {
        let s = &mut *s.borrow_mut();
        let n = out.len();
        // Grown, never refilled: the scratch is written before it is read on
        // every pass, so zeroing it would be a pass over the pairs of its own.
        if s.tmp.len() < n {
            s.tmp.resize(n, (0, 0));
        }
        if s.cnt.len() != RADIX_BUCKETS {
            s.cnt.resize(RADIX_BUCKETS, 0);
        }
        let cnt = &mut s.cnt[..RADIX_BUCKETS];
        let tmp = &mut s.tmp[..n];
        // Pass one on the second index, pass two on the first: both are below
        // `RADIX_BUCKETS`, so each is its own digit and no shifting is needed.
        for pass in 0..2 {
            cnt.fill(0);
            let (src, dst): (&[(u32, u32)], &mut [(u32, u32)]) =
                if pass == 0 { (&out[..n], tmp) } else { (tmp, &mut out[..n]) };
            for e in src.iter() {
                let k = if pass == 0 { e.1 } else { e.0 } as usize;
                cnt[k] += 1;
            }
            let mut run = 0u32;
            for c in cnt.iter_mut() {
                let take = *c;
                *c = run;
                run += take;
            }
            for &e in src.iter() {
                let k = if pass == 0 { e.1 } else { e.0 } as usize;
                dst[cnt[k] as usize] = e;
                cnt[k] += 1;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plain three-way merge the block filter replaced, kept as the
    /// reference the filter is checked against: same pairs, same order, same
    /// `cap` cut.
    fn shared_reference(a: &WordList, b: &WordList, out: &mut Vec<(u32, u32)>, cap: usize) {
        out.clear();
        let (mut i, mut j) = (0usize, 0usize);
        while i < a.word.len() && j < b.word.len() {
            match a.word[i].cmp(&b.word[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    let w = a.word[i];
                    let i0 = i;
                    while i < a.word.len() && a.word[i] == w {
                        i += 1;
                    }
                    let j0 = j;
                    while j < b.word.len() && b.word[j] == w {
                        j += 1;
                    }
                    if (i - i0) * (j - j0) > 64 {
                        continue;
                    }
                    for x in i0..i {
                        for y in j0..j {
                            out.push((a.kp[x], b.kp[y]));
                        }
                    }
                    if out.len() > cap {
                        break;
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }

    /// Random lists at every shape that matters — sparse and dense
    /// intersections, runs on one side and both, lists shorter than a block,
    /// and a cap small enough to cut.
    #[test]
    fn the_block_filter_intersects_exactly_as_a_merge_does() {
        let mut rng = Lcg(0x243f_6a88_85a3_08d3);
        let mut made = |len: usize, span: u32, reps: u32| -> WordList {
            let mut pairs: Vec<(u32, u32)> = (0..len)
                .map(|k| {
                    let w = (rng.byte() as u32) << 8 | rng.byte() as u32;
                    (w % span.max(1) / reps.max(1) * reps.max(1), k as u32)
                })
                .collect();
            pairs.sort_unstable();
            WordList {
                word: pairs.iter().map(|p| p.0).collect(),
                kp: pairs.iter().map(|p| p.1).collect(),
            }
        };
        let mut got = Vec::new();
        let mut want = Vec::new();
        // The last pair reaches past a thousand keypoints, which is the width
        // `sort_pairs` counts in: above it the pairs go back to being compared,
        // and that path has to give the same answer as the counting one.
        for &(la, lb) in [(0usize, 7usize), (1, 1), (3, 40), (9, 9), (17, 8), (64, 64), (300, 290), (1000, 30), (1500, 1200)].iter() {
            for &(span, reps) in [(65535u32, 1u32), (400, 1), (64, 1), (65535, 7), (200, 3)].iter() {
                let a = made(la, span, reps);
                let b = made(lb, span, reps);
                for cap in [60_000usize, 32, 3] {
                    shared(&a, &b, &mut got, cap);
                    shared_reference(&a, &b, &mut want, cap);
                    assert_eq!(got, want, "la {la} lb {lb} span {span} reps {reps} cap {cap}");
                }
            }
        }
    }

    struct Lcg(u64);
    impl Lcg {
        fn byte(&mut self) -> u8 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u8
        }
    }

    /// The descent measures a node's children all at once, out of a block
    /// stored dimension-major, which is an indexing trick over an obvious
    /// loop. The check is not that the indices look right — it is that the
    /// tree still does its job. Descriptors are built in sixteen groups that
    /// sit ~1,100 apart in L2, each group holding sixteen tight modes ~160
    /// apart; a vocabulary that measures distances correctly never gives two
    /// groups the same word, and does subdivide inside a group. Confuse a
    /// centre's dimensions with its siblings' and every distance is noise.
    #[test]
    fn a_two_level_vocabulary_separates_what_is_separable() {
        const GROUPS: usize = 16;
        const SUBS: usize = 16;
        const EACH: usize = 10;
        let mut rng = Lcg(0x9e37_79b9_7f4a_7c15);
        let mut desc: Vec<u8> = Vec::with_capacity(GROUPS * SUBS * EACH * DESC_LEN);
        for _ in 0..GROUPS {
            let centre: Vec<i32> = (0..DESC_LEN).map(|_| (rng.byte() / 2) as i32).collect();
            for sub in 0..SUBS {
                let mut mode = centre.clone();
                for d in 0..8 {
                    mode[(sub * 8 + d) % DESC_LEN] += 40;
                }
                for _ in 0..EACH {
                    for &m in mode.iter() {
                        desc.push((m + (rng.byte() % 3) as i32 - 1).clamp(0, 255) as u8);
                    }
                }
            }
        }
        let p = VocabParams { depth: 2, ..Default::default() };
        let v = Vocabulary::build(&desc, &p);

        let mut words: Vec<Vec<u32>> = Vec::new();
        let mut out = Vec::new();
        for (i, d) in desc.chunks_exact(DESC_LEN).enumerate() {
            v.quantise(d, &mut out);
            assert!(!out.is_empty(), "descriptor {i} quantised to nothing");
            assert!(out.iter().all(|&w| (w as usize) < v.n_words()));
            words.push(out.clone());
        }
        for (i, wi) in words.iter().enumerate() {
            for (j, wj) in words.iter().enumerate().skip(i + 1) {
                if i / (SUBS * EACH) != j / (SUBS * EACH) {
                    assert!(
                        wi.iter().all(|w| !wj.contains(w)),
                        "descriptors {i} and {j} are from different groups and share a word"
                    );
                }
            }
        }
        let mut firsts: Vec<u32> = words.iter().map(|w| w[0]).collect();
        firsts.sort_unstable();
        firsts.dedup();
        assert!(firsts.len() > GROUPS, "the second level separated nothing");
    }
}

// ---------------------------------------------------------------- kernel timings

/// `cargo test --release -- --ignored --nocapture quantise_timings`
#[cfg(test)]
mod bench {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn byte(&mut self) -> u8 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u8
        }
    }

    #[test]
    #[ignore]
    fn quantise_timings() {
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        let n = 60_000;
        let desc: Vec<u8> = (0..n * DESC_LEN).map(|_| rng.byte()).collect();
        let p = VocabParams { depth: 4, sample: 40_000, ..Default::default() };
        let v = Vocabulary::build(&desc, &p);
        let mut out = Vec::new();
        let mut best = f64::MAX;
        for _ in 0..7 {
            let t = std::time::Instant::now();
            for d in desc.chunks_exact(DESC_LEN) {
                v.quantise(d, &mut out);
                std::hint::black_box(&out);
            }
            best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
        }
        println!("quantise: {best:8.1} ns/descriptor  ({} words)", v.n_words());
    }

    /// `cargo test --release -- --ignored --nocapture shared_timings`
    ///
    /// The word-list intersection, on one core, at the shape a real corpus
    /// produces: `L` entries per list over `V` words, both sorted.
    #[test]
    #[ignore]
    fn shared_timings() {
        let mut rng = Lcg(0x9e37_79b9_7f4a_7c15);
        let mut w32 = || {
            let a = rng.byte() as u32;
            let b = rng.byte() as u32;
            let c = rng.byte() as u32;
            (a << 16) | (b << 8) | c
        };
        for (l, v) in [(1300usize, 1_771_561u32), (1070, 1_048_576), (1300, 262_144)] {
            // A pool of lists, so that each call reads a different one and the
            // measurement is not a single pair sitting in L1.
            let lists: Vec<WordList> = (0..64)
                .map(|_| {
                    let mut pairs: Vec<(u32, u32)> =
                        (0..l).map(|k| (w32() % v, (k / 3) as u32)).collect();
                    pairs.sort_unstable();
                    WordList {
                        word: pairs.iter().map(|p| p.0).collect(),
                        kp: pairs.iter().map(|p| p.1).collect(),
                    }
                })
                .collect();
            let mut out = Vec::new();
            let mut best = f64::MAX;
            for _ in 0..7 {
                let t = std::time::Instant::now();
                let mut n = 0usize;
                for a in lists.iter() {
                    for b in lists.iter() {
                        shared(a, b, &mut out, 60_000);
                        std::hint::black_box(&out);
                        n += 1;
                    }
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
            println!("shared: {best:9.0} ns/call  (len {l}, {v} words)");
        }
    }

    /// `cargo test --release -- --ignored --nocapture query_timings`
    ///
    /// Retrieval at the shape a found corpus gives it: nine thousand images of
    /// about 1,300 word entries each over 170,000 live words, so the average
    /// word is in some seventy images and a query walks a hundred thousand
    /// postings into an accumulator of one float per image. The accumulator is
    /// 37 KB at that size — bigger than a first-level cache and the reason
    /// `query` blocks its images (see `ACC_BLOCK`).
    #[test]
    #[ignore]
    fn query_timings() {
        for (n_imgs, words) in [(9285usize, 170_000u32), (5637, 160_000)] {
            let mut rng = Lcg(0x8e3f_1a9c_2b7d_4e51);
            let lists: Vec<WordList> = (0..n_imgs)
                .map(|_| {
                    // Zipf-ish: a word's popularity is uneven, which is what
                    // makes some postings long and the accumulator's reuse
                    // pattern irregular.
                    let mut pairs: Vec<(u32, u32)> = (0..1300)
                        .map(|k| {
                            let a = (rng.byte() as u32) << 16 | (rng.byte() as u32) << 8 | rng.byte() as u32;
                            let b = (rng.byte() as u32) << 8 | rng.byte() as u32;
                            let w = if b % 4 == 0 { a % (words / 64) } else { a % words };
                            (w, (k / 3) as u32)
                        })
                        .collect();
                    pairs.sort_unstable();
                    WordList {
                        word: pairs.iter().map(|p| p.0).collect(),
                        kp: pairs.iter().map(|p| p.1).collect(),
                    }
                })
                .collect();
            let inv = InvertedFile::build(&lists, words as usize, (n_imgs / 5).max(32));
            let mut acc = vec![0f32; n_imgs];
            let (mut touched, mut out) = (Vec::new(), Vec::new());
            let mut best = f64::MAX;
            let mut hits = 0usize;
            for _ in 0..5 {
                let t = std::time::Instant::now();
                hits = 0;
                for (i, wl) in lists.iter().enumerate().step_by(7) {
                    inv.query(wl, i as u32, &mut acc, &mut touched, &mut out);
                    hits += out.len();
                    std::hint::black_box(&out);
                }
                let n = lists.len().div_ceil(7);
                best = best.min(t.elapsed().as_secs_f64() * 1e6 / n as f64);
            }
            println!(
                "query: {best:8.1} us/query  ({n_imgs} images, {words} words, {} candidates scored)",
                hits / lists.len().div_ceil(7)
            );
        }
    }

    /// `cargo test --release -- --ignored --nocapture shared_overlap`
    ///
    /// The same intersection at the overlap a real pair has. Two images that
    /// share nothing still share some words — the reach histogram of a
    /// nine-thousand-image corpus puts the typical candidate at thirty to a
    /// hundred shared entries — and the cost of a call is not all in the merge:
    /// every shared word emits `c_a * c_b` pairs and the list is sorted and
    /// deduplicated before it is returned. This says how the two halves divide.
    #[test]
    #[ignore]
    fn shared_overlap() {
        let mut rng = Lcg(0x51ed_270b_6efc_2f4d);
        let l = 1300usize;
        let v = 1_771_561u32;
        for shared_words in [0usize, 10, 40, 100, 300, 800] {
            let lists: Vec<(WordList, WordList)> = (0..32)
                .map(|_| {
                    let mut wa: Vec<u32> = Vec::new();
                    let mut wb: Vec<u32> = Vec::new();
                    let mut w32 = |rng: &mut Lcg| {
                        let (a, b, c) = (rng.byte() as u32, rng.byte() as u32, rng.byte() as u32);
                        ((a << 16) | (b << 8) | c) % v
                    };
                    // Shared words first, then each side's own, then a run
                    // length of one to three for each, as a descriptor's
                    // multi-assignment gives.
                    let mut common = Vec::new();
                    for _ in 0..shared_words {
                        common.push(w32(&mut rng));
                    }
                    for w in common.iter() {
                        for _ in 0..1 + (rng.byte() % 3) {
                            wa.push(*w);
                        }
                        for _ in 0..1 + (rng.byte() % 3) {
                            wb.push(*w);
                        }
                    }
                    while wa.len() < l {
                        wa.push(w32(&mut rng));
                    }
                    while wb.len() < l {
                        wb.push(w32(&mut rng));
                    }
                    let mk = |mut w: Vec<u32>| {
                        w.sort_unstable();
                        let kp: Vec<u32> = (0..w.len() as u32).collect();
                        WordList { word: w, kp }
                    };
                    (mk(wa), mk(wb))
                })
                .collect();
            let mut out = Vec::new();
            let mut best = f64::MAX;
            let mut emitted = 0usize;
            for _ in 0..7 {
                let t = std::time::Instant::now();
                let mut n = 0usize;
                emitted = 0;
                for (a, b) in lists.iter() {
                    shared(a, b, &mut out, 60_000);
                    emitted += out.len();
                    std::hint::black_box(&out);
                    n += 1;
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
            println!(
                "shared_overlap: {best:9.0} ns/call  ({shared_words} shared words, {} pairs out)",
                emitted / 32
            );
        }
    }

    /// `cargo test --release -- --ignored --nocapture shared_pool`
    ///
    /// The same intersection against a pool too large to cache, which is the
    /// shape the matcher really asks for: a query list against nine thousand
    /// other images' lists, each some ten kilobytes and none of them recently
    /// read. `shared_timings` keeps its pool in L2 and so measures the
    /// arithmetic; this one measures the arithmetic plus whatever the memory
    /// costs, and the gap between the two is the answer to "would a faster
    /// loop help".
    #[test]
    #[ignore]
    fn shared_pool() {
        let mut rng = Lcg(0x9e37_79b9_7f4a_7c15);
        let mut w32 = || {
            let a = rng.byte() as u32;
            let b = rng.byte() as u32;
            let c = rng.byte() as u32;
            (a << 16) | (b << 8) | c
        };
        let l = 1300usize;
        let v = 1_771_561u32;
        for n_lists in [64usize, 1024, 8192] {
            let lists: Vec<WordList> = (0..n_lists)
                .map(|_| {
                    let mut pairs: Vec<(u32, u32)> =
                        (0..l).map(|k| (w32() % v, (k / 3) as u32)).collect();
                    pairs.sort_unstable();
                    WordList {
                        word: pairs.iter().map(|p| p.0).collect(),
                        kp: pairs.iter().map(|p| p.1).collect(),
                    }
                })
                .collect();
            let mb = n_lists as f64 * l as f64 * 8.0 / 1e6;
            let mut out = Vec::new();
            let mut best = f64::MAX;
            for _ in 0..5 {
                let t = std::time::Instant::now();
                let mut n = 0usize;
                // One query list against a long stride of others, which is
                // what a candidate list looks like: the query stays warm, the
                // other side never does.
                for q in 0..n_lists {
                    for step in 1..17 {
                        let o = (q * 7 + step * 613) % n_lists;
                        shared(&lists[q], &lists[o], &mut out, 60_000);
                        std::hint::black_box(&out);
                        n += 1;
                    }
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
            println!("shared_pool: {best:9.0} ns/call  ({n_lists} lists, {mb:.0} MB)");
        }
    }

    /// `cargo test --release -- --ignored --nocapture quantise_depth`
    ///
    /// Cost per child-distance against the size of the tree, at a fixed
    /// branching. A shallow tree fits in cache and a deep one does not, so
    /// this says whether the descent is waiting on the centres or on the
    /// arithmetic — which decides whether to shrink the centres or to
    /// vectorise harder.
    #[test]
    #[ignore]
    fn quantise_depth() {
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        let n = 60_000;
        let desc: Vec<u8> = (0..n * DESC_LEN).map(|_| rng.byte()).collect();
        for depth in 1..=5usize {
            let p = VocabParams { depth, branching: 16, sample: 40_000, ..Default::default() };
            let v = Vocabulary::build(&desc, &p);
            let live: usize = v.levels.iter().map(|l| l.len() / DESC_LEN).sum();
            let mut out = Vec::new();
            let mut best = f64::MAX;
            for _ in 0..7 {
                let t = std::time::Instant::now();
                for d in desc.chunks_exact(DESC_LEN) {
                    v.quantise(d, &mut out);
                    std::hint::black_box(&out);
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
            // Distances actually evaluated: every level but the first has up
            // to `max_paths` parents.
            let dists = 16 + (depth - 1) * 3 * 16;
            println!(
                "depth {depth}: {best:8.1} ns/descriptor  {:5.2} ns/child-distance  ({live} live centres, {:.1} MB)",
                best / dists as f64,
                live as f64 * DESC_LEN as f64 / 1e6
            );
        }
    }

    /// `cargo test --release -- --ignored --nocapture quantise_threads`
    ///
    /// The descent on one core and on all of them, against a tree that fits in
    /// cache and against one the size a real corpus builds. What the ratio
    /// says: if eight threads cost far more per descriptor than one does, the
    /// descent is waiting for memory and the way to speed it up is to make the
    /// centres smaller; if they cost about the same, it is waiting for the
    /// arithmetic and the way is to issue fewer instructions.
    #[test]
    #[ignore]
    fn quantise_threads() {
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        let n = 120_000;
        let desc: Vec<u8> = (0..n * DESC_LEN).map(|_| rng.byte()).collect();
        for (depth, sample) in [(3usize, 160_000usize), (4, 160_000), (5, 160_000)] {
            let p = VocabParams { depth, branching: 16, sample, ..Default::default() };
            let v = Vocabulary::build(&desc, &p);
            let live: usize = v.levels.iter().map(|l| l.len() / DESC_LEN).sum();
            let mut best1 = f64::MAX;
            let mut best8 = f64::MAX;
            for _ in 0..3 {
                let mut out = Vec::new();
                let t = std::time::Instant::now();
                for d in desc.chunks_exact(DESC_LEN) {
                    v.quantise(d, &mut out);
                    std::hint::black_box(&out);
                }
                best1 = best1.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
                let t = std::time::Instant::now();
                desc.par_chunks(DESC_LEN * 64).for_each(|blk| {
                    let mut out = Vec::new();
                    for d in blk.chunks_exact(DESC_LEN) {
                        v.quantise(d, &mut out);
                        std::hint::black_box(&out);
                    }
                });
                best8 = best8.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
            println!(
                "depth {depth}: 1 thread {best1:8.1} ns/desc, all threads {best8:8.1} ns/desc  (x{:.2} per thread on 8, {live} centres, {:.0} MB)",
                best8 * 8.0 / best1,
                live as f64 * DESC_LEN as f64 / 1e6
            );
        }
    }

    /// `cargo test --release -- --ignored --nocapture quantise_branching`
    ///
    /// The descent's cost against the branching `for_corpus` picked, which is
    /// almost never `MAX_BRANCH`.
    #[test]
    #[ignore]
    fn quantise_branching() {
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        let n = 60_000;
        let desc: Vec<u8> = (0..n * DESC_LEN).map(|_| rng.byte()).collect();
        for branching in [8usize, 9, 10, 11, 12, 13, 14, 15, 16] {
            let p = VocabParams { depth: 4, branching, sample: 40_000, ..Default::default() };
            let v = Vocabulary::build(&desc, &p);
            let mut out = Vec::new();
            let mut best = f64::MAX;
            for _ in 0..7 {
                let t = std::time::Instant::now();
                for d in desc.chunks_exact(DESC_LEN) {
                    v.quantise(d, &mut out);
                    std::hint::black_box(&out);
                }
                best = best.min(t.elapsed().as_secs_f64() * 1e9 / n as f64);
            }
            println!("branching {branching:2}: {best:8.1} ns/descriptor  ({} words)", v.n_words());
        }
    }
}
