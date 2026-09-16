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
    pub depth: usize,
    pub sample: usize,
    pub iters: usize,
    /// Extra descent paths kept when a child is nearly as close as the best.
    pub max_paths: usize,
    pub path_ratio: f32,
    pub seed: u64,
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

/// Hierarchical k-means tree. Nodes are stored breadth-first per level;
/// `centres[l]` holds every node of level `l`, `DESC_LEN` floats each.
pub struct Vocabulary {
    pub branching: usize,
    pub depth: usize,
    levels: Vec<Vec<f32>>,
    /// Number of nodes at each level (`branching^(l+1)` minus pruned ones,
    /// stored dense with empty nodes marked by `live`).
    live: Vec<Vec<bool>>,
    max_paths: usize,
    path_ratio: f32,
}

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
        let mut live: Vec<Vec<bool>> = Vec::with_capacity(p.depth);
        // Which sample belongs to which node of the previous level.
        let mut assign: Vec<u32> = vec![0; take];
        let mut parents = 1usize;

        for _level in 0..p.depth {
            let nodes = parents * p.branching;
            let mut centres = vec![0f32; nodes * DESC_LEN];
            let mut alive = vec![false; nodes];
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
                centres[base * DESC_LEN..(base + p.branching) * DESC_LEN].copy_from_slice(&c);
                alive[base..base + p.branching].copy_from_slice(&l);
                for (i, child) in a {
                    assign[i as usize] = (base + child as usize) as u32;
                }
            }
            levels.push(centres);
            live.push(alive);
            parents = nodes;
        }
        Vocabulary {
            branching: p.branching,
            depth: p.depth,
            levels,
            live,
            max_paths: p.max_paths,
            path_ratio: p.path_ratio,
        }
    }

    /// Quantise one descriptor to up to `max_paths` words, best first.
    pub fn quantise(&self, desc: &[u8], out: &mut Vec<u32>) {
        out.clear();
        let mut q = [0f32; DESC_LEN];
        for i in 0..DESC_LEN {
            q[i] = desc[i] as f32;
        }
        // Frontier of (node index at this level, distance).
        let mut cur: Vec<(u32, f32)> = vec![(0, 0.0)];
        let mut next: Vec<(u32, f32)> = Vec::with_capacity(self.max_paths * self.branching);
        for l in 0..self.depth {
            next.clear();
            let centres = &self.levels[l];
            let alive = &self.live[l];
            for &(parent, _) in cur.iter() {
                let base = parent as usize * self.branching;
                for c in 0..self.branching {
                    let node = base + c;
                    if !alive[node] {
                        continue;
                    }
                    next.push((node as u32, d2(&q, &centres[node * DESC_LEN..(node + 1) * DESC_LEN])));
                }
            }
            if next.is_empty() {
                break;
            }
            next.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            let best = next[0].1;
            let cut = best * self.path_ratio * self.path_ratio + 1.0;
            next.truncate(self.max_paths);
            while next.len() > 1 && next.last().unwrap().1 > cut {
                next.pop();
            }
            std::mem::swap(&mut cur, &mut next);
        }
        for &(node, _) in cur.iter() {
            out.push(node);
        }
    }
}

/// k-means on a subset, with k-means++ seeding. Empty clusters are marked
/// dead rather than re-seeded: a vocabulary with fewer live nodes is correct,
/// one with a centre nobody uses is noise.
fn kmeans(data: &[f32], members: &[u32], k: usize, iters: usize, seed: u64) -> (Vec<f32>, Vec<bool>, Vec<(u32, u32)>) {
    let mut centres = vec![0f32; k * DESC_LEN];
    let mut live = vec![false; k];
    let mut assign: Vec<(u32, u32)> = Vec::with_capacity(members.len());
    if members.is_empty() {
        return (centres, live, assign);
    }
    if members.len() <= k {
        for (c, &m) in members.iter().enumerate() {
            centres[c * DESC_LEN..(c + 1) * DESC_LEN]
                .copy_from_slice(&data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN]);
            live[c] = true;
            assign.push((m, c as u32));
        }
        return (centres, live, assign);
    }
    let mut rng = Rng(seed | 1);
    // k-means++
    let mut chosen: Vec<u32> = Vec::with_capacity(k);
    chosen.push(members[rng.below(members.len())]);
    let mut best_d: Vec<f32> = members
        .iter()
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
        let cd = &data[c as usize * DESC_LEN..(c as usize + 1) * DESC_LEN];
        for (i, &m) in members.iter().enumerate() {
            let d = d2(&data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN], cd);
            if d < best_d[i] {
                best_d[i] = d;
            }
        }
    }
    let kk = chosen.len();
    for (c, &m) in chosen.iter().enumerate() {
        centres[c * DESC_LEN..(c + 1) * DESC_LEN]
            .copy_from_slice(&data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN]);
    }
    let mut owner = vec![0u32; members.len()];
    for it in 0..iters {
        let mut moved = false;
        for (i, &m) in members.iter().enumerate() {
            let dv = &data[m as usize * DESC_LEN..(m as usize + 1) * DESC_LEN];
            let mut best = (f32::MAX, 0u32);
            for c in 0..kk {
                let d = d2(dv, &centres[c * DESC_LEN..(c + 1) * DESC_LEN]);
                if d < best.0 {
                    best = (d, c as u32);
                }
            }
            if owner[i] != best.1 || it == 0 {
                moved = true;
            }
            owner[i] = best.1;
        }
        if !moved {
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

pub struct InvertedFile {
    /// For each word: the images containing it, and how many descriptors.
    pub postings: Vec<Vec<(u32, u32)>>,
    /// log(N / df), per word.
    pub idf: Vec<f32>,
}

impl InvertedFile {
    pub fn build(lists: &[WordList], n_words: usize, max_posting: usize) -> InvertedFile {
        let n = lists.len();
        let mut postings: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n_words];
        for (img, wl) in lists.iter().enumerate() {
            for (w, c) in wl.runs() {
                postings[w as usize].push((img as u32, c));
            }
        }
        let mut idf = vec![0f32; n_words];
        for (w, p) in postings.iter_mut().enumerate() {
            let df = p.len();
            if df == 0 {
                continue;
            }
            // A word in a large fraction of the corpus carries no information
            // and costs the most to traverse; dropping it is both faster and
            // more accurate.
            if df > max_posting {
                p.clear();
                p.shrink_to_fit();
                continue;
            }
            idf[w] = (n as f32 / df as f32).ln();
        }
        InvertedFile { postings, idf }
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
            let post = &self.postings[w];
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
