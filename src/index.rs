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
/// The value is also close to unfittable, which is the point. The tree has
/// `16^depth` leaves, so the depth only moves when the target crosses a factor
/// of sixteen: on the benchmark corpus every value from 21 to 327 descriptors
/// per word gives the same four-level tree, and on the validation corpus every
/// value from 9 to 132 does. What the rule does
/// change is the small end, where the old fixed 65,536 words failed outright —
/// a folder of eight images put every descriptor in a word of its own, and the
/// tool found one pair out of twenty-eight.
const DESC_PER_WORD: usize = 32;

impl VocabParams {
    /// Shallowest tree with at least `n_desc/DESC_PER_WORD` leaves.
    ///
    /// Rounded up rather than to the nearest, because the two directions are
    /// not symmetric: too many words splits a true match across two of them,
    /// which multi-path descent already exists to survive, while too few makes
    /// every image share words with every other and the score stops meaning
    /// anything.
    pub fn for_corpus(n_desc: usize) -> VocabParams {
        let p = VocabParams::default();
        let target = (n_desc / DESC_PER_WORD).max(p.branching);
        let mut depth = 1usize;
        while depth < 6 && p.branching.pow(depth as u32) < target {
            depth += 1;
        }
        VocabParams { depth, ..p }
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
/// 536 MB of centres of which at most a sixth can ever be reached. `slot` maps
/// a node number to its centre, or to `DEAD` for the nodes k-means never
/// populated.
pub struct Vocabulary {
    pub branching: usize,
    pub depth: usize,
    /// `levels[l]` holds the centres of the live nodes of level `l`,
    /// `DESC_LEN` floats each, in node order.
    levels: Vec<Vec<f32>>,
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

/// Upper bound on `branching`, likewise, so the descent's accumulators are an
/// array the compiler can keep in registers.
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

impl Vocabulary {
    /// Words are the leaves: `branching^depth` of them.
    pub fn n_words(&self) -> usize {
        self.branching.pow(self.depth as u32)
    }

    pub fn build(descriptors: &[u8], p: &VocabParams) -> Vocabulary {
        assert!(p.max_paths * p.branching <= FRONTIER, "vocabulary frontier too small");
        assert!(p.branching <= MAX_BRANCH, "branching wider than the descent's accumulators");
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

        let mut levels: Vec<Vec<f32>> = Vec::with_capacity(p.depth);
        let mut heads: Vec<Vec<Kids>> = Vec::with_capacity(p.depth);
        let mut node_ofs: Vec<Vec<u32>> = Vec::with_capacity(p.depth);
        // Which sample belongs to which node of the previous level.
        let mut assign: Vec<u32> = vec![0; take];
        let mut parents = 1usize;

        for _level in 0..p.depth {
            let nodes = parents * p.branching;
            let mut centres: Vec<f32> = Vec::new();
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
                // One parent's live children go down together, transposed:
                // dimension-major, so that the descent walks all of them in
                // step. See `quantise`.
                let live: Vec<usize> = (0..p.branching).filter(|&c| l[c]).collect();
                let first = (centres.len() / DESC_LEN) as u32;
                for (k, &child) in live.iter().enumerate() {
                    slot[base + child] = first + k as u32;
                }
                // `c` holds the live centres only, in child order.
                centres.reserve(live.len() * DESC_LEN);
                for i in 0..DESC_LEN {
                    for k in 0..live.len() {
                        centres.push(c[k * DESC_LEN + i]);
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
        let mut q = [0f32; DESC_LEN];
        for i in 0..DESC_LEN {
            q[i] = desc[i] as f32;
        }
        // Frontier of (node index at this level, distance).
        let mut cur = [(0u32, 0f32); FRONTIER];
        let mut next = [(0u32, 0f32); FRONTIER];
        let mut n_cur = 1usize;
        for l in 0..self.depth {
            let mut n_next = 0usize;
            let centres = &self.levels[l];
            let head = &self.head[l];
            let node_of = &self.node_of[l];
            for &(parent, _) in cur[..n_cur].iter() {
                let kids = head[parent as usize];
                let n_live = kids.n as usize;
                if n_live == 0 {
                    continue;
                }
                let first = kids.first as usize;
                // All of this parent's children at once. Each child's sum is
                // still taken over the dimensions in order, so it is the same
                // float to the bit as summing the children one at a time — but
                // sixteen independent sums keep the machine's adders busy
                // where one chain of 128 dependent adds could not. This is the
                // innermost loop of quantisation: it runs `depth * branching`
                // times for every descriptor in the corpus.
                let blk = &centres[first * DESC_LEN..(first + n_live) * DESC_LEN];
                let mut acc = [0f32; MAX_BRANCH];
                if n_live == MAX_BRANCH {
                    for (i, &qi) in q.iter().enumerate() {
                        let row = &blk[i * MAX_BRANCH..(i + 1) * MAX_BRANCH];
                        for k in 0..MAX_BRANCH {
                            let d = qi - row[k];
                            acc[k] += d * d;
                        }
                    }
                } else {
                    for (i, &qi) in q.iter().enumerate() {
                        let row = &blk[i * n_live..(i + 1) * n_live];
                        for (k, &c) in row.iter().enumerate() {
                            let d = qi - c;
                            acc[k] += d * d;
                        }
                    }
                }
                for k in 0..n_live.min(FRONTIER - n_next) {
                    next[n_next] = (node_of[first + k], acc[k]);
                    n_next += 1;
                }
            }
            if n_next == 0 {
                break;
            }
            // Only the closest `max_paths` children are descended, and only
            // their distances are looked at afterwards. Ordering the whole
            // frontier — up to forty-eight entries, once per level for every
            // descriptor in the corpus — decided the order of forty-five nodes
            // about to be discarded, so the three that survive are picked out
            // by a scan instead.
            //
            // The scan gives the sorted order of the smallest three whenever
            // those three, and the boundary between kept and dropped, are
            // unambiguous. When two distances are exactly equal across that
            // boundary the answer is not determined by the distances at all,
            // and which node the tree descends then depends on the sorting
            // algorithm; rather than change that by accident, such a frontier
            // is handed to the same sort as before. It is a rarity — a tie has
            // to be exact, in floats summed over 128 dimensions — and the
            // check that spots one is a pass of comparisons against a sort.
            let keep = self.max_paths.min(n_next);
            let mut held = 0usize;
            for i in 0..n_next {
                let e = next[i];
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
                nx.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                cur[..held].copy_from_slice(&next[..held]);
            }
            let cut = cur[0].1 * self.path_ratio * self.path_ratio + 1.0;
            n_cur = held;
            while n_cur > 1 && cur[n_cur - 1].1 > cut {
                n_cur -= 1;
            }
        }
        for &(node, _) in cur[..n_cur].iter() {
            out.push(node);
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
                if kk == MAX_BRANCH {
                    for (i, &qi) in dv.iter().enumerate() {
                        let row = &tc[i * MAX_BRANCH..(i + 1) * MAX_BRANCH];
                        for c in 0..MAX_BRANCH {
                            let d = qi - row[c];
                            acc[c] += d * d;
                        }
                    }
                } else {
                    for (i, &qi) in dv.iter().enumerate() {
                        let row = &tc[i * kk..(i + 1) * kk];
                        for (c, &cv) in row.iter().enumerate() {
                            let d = qi - cv;
                            acc[c] += d * d;
                        }
                    }
                }
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
    pub fn runs(&self) -> Runs<'_> {
        Runs { wl: self, i: 0 }
    }

}

pub struct Runs<'a> {
    wl: &'a WordList,
    i: usize,
}

impl Iterator for Runs<'_> {
    type Item = (u32, u32);
    fn next(&mut self) -> Option<(u32, u32)> {
        if self.i >= self.wl.word.len() {
            return None;
        }
        let w = self.wl.word[self.i];
        let mut c = 0u32;
        while self.i < self.wl.word.len() && self.wl.word[self.i] == w {
            c += 1;
            self.i += 1;
        }
        Some((w, c))
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
        for (w, c) in wl.runs() {
            let w = w as usize;
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

/// Candidate descriptor pairs for two images: keypoints that share a word.
pub fn shared(a: &WordList, b: &WordList, out: &mut Vec<(u32, u32)>, cap: usize) {
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
                // A word matching many keypoints on both sides is repeated
                // texture, not a landmark; it would dominate the list without
                // helping the fit.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
