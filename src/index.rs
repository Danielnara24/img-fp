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

/// How many dimensions the distance loop is handed at a time, and the vector
/// width it is handed them for. See `child_dists`.
const SPAN: usize = 32;
const LANES: usize = 8;

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
            // The frontier is rebuilt as the children are scored, so the
            // parents come out of it first. They are at most `max_paths`.
            let mut par = [0u32; FRONTIER];
            for i in 0..n_cur {
                par[i] = cur[i].0;
            }
            // `max_paths` rather than the old `min(max_paths, n_next)`: the
            // two differ only when the level offers fewer children than that,
            // and then the scan never fills up, so the bound is never read.
            let keep = self.max_paths;
            let mut held = 0usize;
            for pi in 0..n_cur {
                let kids = head[par[pi] as usize];
                let n_live = kids.n as usize;
                if n_live == 0 {
                    continue;
                }
                let first = kids.first as usize;
                // All of this parent's children at once — see `child_dists`.
                // This is the innermost loop of quantisation: it runs
                // `depth * max_paths` times for every descriptor in the
                // corpus, and on a corpus of any size it is most of the
                // retrieval stage.
                let blk = &centres[first * DESC_LEN..(first + n_live) * DESC_LEN];
                let mut acc = [0f32; MAX_BRANCH];
                child_dists(n_live, &q, blk, &mut acc);
                // Only the closest `max_paths` children are descended, and
                // only their distances are looked at afterwards. Ordering the
                // whole frontier — up to forty-eight entries, once per level
                // for every descriptor in the corpus — decided the order of
                // forty-five nodes about to be discarded, so the survivors are
                // picked out by this scan instead. It used to run over the
                // finished frontier; run per child, it reads each distance
                // where the distance already is.
                for k in 0..n_live.min(FRONTIER - n_next) {
                    let e = (node_of[first + k], acc[k]);
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
            // unambiguous. When two distances are exactly equal across that
            // boundary the answer is not determined by the distances at all,
            // and which node the tree descends then depends on the sorting
            // algorithm; rather than change that by accident, such a frontier
            // is handed to the same sort as before. It is a rarity — a tie has
            // to be exact, in floats summed over 128 dimensions — and the
            // check that spots one is a pass of comparisons against a sort.
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

/// Squared distance from `q` to each of `k` centres held dimension-major in
/// `blk`, into the first `k` lanes of `acc`. Each centre's sum is taken over
/// the dimensions in order, so every distance is the same float it would be if
/// the centres were measured one at a time — but `k` sums run at once instead
/// of one chain of 128 dependent adds.
///
/// **The width reaches the loop as a constant**, which is the whole point of
/// the dispatch below. It is a property of the tree, settled when it was
/// built, and it is almost never `MAX_BRANCH`: `for_corpus` narrows the
/// branching to the smallest tree that still holds the target occupancy, so
/// the only corpora that land on sixteen are the ones whose descriptor count
/// sits just above a power of it. Everything else ran down a loop whose trip
/// count the compiler could not see, which neither unrolls nor vectorises.
/// Measured on the descent alone, at depth 4 over 60,000 descriptors, the
/// runtime-width loop cost 9,461 ns/descriptor at width 15 against 5,431 at
/// width 16 — three quarters more, for less arithmetic. Held constant, every
/// width is on the same curve.
///
/// **And the dimensions go in four spans of thirty-two, except where the
/// width is a whole number of vector lanes.** The sum is the same sum — the
/// terms are still added in dimension order, so a distance finished in four
/// pieces is the same float to the bit — but handing the loop a short,
/// fixed-length run of dimensions is worth 20 to 37 per cent of the descent at
/// every width from 9 to 15, which is every width `for_corpus` picks except
/// two. At 8 and 16 the `k` floats are exactly one or two vector registers,
/// the whole-array loop was already compiling to the right thing, and the
/// split costs 6 to 8 per cent; those two keep it. `cargo test --release --
/// --ignored --nocapture quantise_branching` is the measurement, and it wants
/// running against both forms, because which one wins is a fact about the
/// compiler and not about the arithmetic.
#[inline(always)]
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

/// Index of the first element of `s` that is not below `key`.
///
/// `s[0] < key` is the caller's precondition, so the answer is at least one
/// and the walk always makes progress. Exponential search first, then binary
/// over the bracket it lands in: the step doubles, so a key one place along
/// costs one comparison and a key a thousand places along costs twenty rather
/// than a thousand.
#[inline]
fn gallop(s: &[u32], key: u32) -> usize {
    debug_assert!(!s.is_empty() && s[0] < key);
    let mut lo = 0usize;
    let mut step = 1usize;
    while lo + step < s.len() && s[lo + step] < key {
        lo += step;
        step *= 2;
    }
    let hi = (lo + step).min(s.len());
    lo + s[lo..hi].partition_point(|&v| v < key)
}

/// Candidate descriptor pairs for two images: keypoints that share a word.
///
/// The two lists are walked as a galloping intersection rather than a plain
/// merge, because the intersection is *sparse*: a few hundred images' word
/// lists hold about 1,300 entries each over a vocabulary of a million-odd
/// words, and two of them share some tens. A plain merge takes one step per
/// entry of both lists, and each step is a three-way comparison on data that
/// gives the predictor nothing — measured, it was 43 microseconds a pair
/// against 6 for the sort at the end of the same function, and it was called
/// five million times in one run of a nine-thousand-image corpus. Skipping to
/// the next candidate instead of walking to it is the same intersection in the
/// same order, so the pairs that come out are unchanged.
pub fn shared(a: &WordList, b: &WordList, out: &mut Vec<(u32, u32)>, cap: usize) {
    out.clear();
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.word.len() && j < b.word.len() {
        match a.word[i].cmp(&b.word[j]) {
            std::cmp::Ordering::Less => i += gallop(&a.word[i..], b.word[j]),
            std::cmp::Ordering::Greater => j += gallop(&b.word[j..], a.word[i]),
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
                live as f64 * DESC_LEN as f64 * 4.0 / 1e6
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
