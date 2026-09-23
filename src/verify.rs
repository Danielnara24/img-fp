//! Deciding whether two images are the same picture.
//!
//! Nothing here is a similarity score with a threshold on it. A claim is made
//! only when a concrete geometric hypothesis exists — "this rectangle of A is
//! that rectangle of B, at this scale and rotation" — and the pixels agree
//! along it. That is what lets the tool say an image is *inside* a slide or a
//! collage, which a whole-image descriptor cannot express, and what keeps a
//! shuffled tiling from passing: its pixels agree only in pieces that no
//! single transform explains.
//!
//! Three tests, in increasing cost:
//!
//! 1. **Correspondences.** For each descriptor of one image, the nearest
//!    descriptor of the other among those sharing a vocabulary word.
//! 2. **Geometry.** A similarity transform is proposed by every single
//!    correspondence (a SIFT keypoint carries its own scale and orientation,
//!    so one match is enough), the best is kept by inlier count, then refined
//!    to an affine fit. Inliers are counted at distinct positions, because a
//!    keypoint with two dominant orientations is one piece of evidence.
//! 3. **Pixels.** The overlap is resampled from both thumbnails through the
//!    transform and compared blockwise. Blocks that are flat in both images
//!    abstain rather than agreeing for free.

use crate::decode::Gray;
use crate::sift::{Features, Keypoint, DESC_LEN};
use crate::timed;

/// 2x3 row-major affine: b = M * (a, 1).
pub type Affine = [f32; 6];

/// How the query image was transformed before its descriptors were matched.
///
/// `mirror` matters only while matching: a mirrored match is folded into the
/// transform itself, as an affine with negative determinant, so that
/// transforms stay composable. `invert` is a colour relation and cannot be,
/// so it rides along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Variant {
    pub mirror: bool,
    pub invert: bool,
}

/// The coordinate map of a horizontal mirror on an image `w` wide.
#[inline]
pub fn mirror_affine(w: f32) -> Affine {
    [-1.0, 0.0, w - 1.0, 0.0, 1.0, 0.0]
}

#[derive(Clone, Copy, Debug)]
pub struct Verdict {
    pub m: Affine,
    /// Kept for reporting and debugging: which permutation of the query
    /// produced this match.
    #[allow(dead_code)]
    pub variant: Variant,
    /// Inliers at distinct keypoint positions.
    pub n_in: u32,
    /// Whether the inliers enclose the centre of the overlap the transform
    /// claims. See `encloses_centre`.
    pub centred: bool,
    pub n_match: u32,
    /// Fraction of A's frame that lands inside B, and the reverse.
    pub ov_a: f32,
    pub ov_b: f32,
    pub scale: f32,
    pub rot_deg: f32,
    /// Mean correlation over the blocks of the overlap that carry detail.
    pub blk: f32,
    pub blk_n: u32,
    pub ncc: f32,
}

impl Default for Verdict {
    fn default() -> Self {
        Verdict {
            m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            variant: Variant::default(),
            n_in: 0,
            centred: false,
            n_match: 0,
            ov_a: 0.0,
            ov_b: 0.0,
            scale: 0.0,
            rot_deg: 0.0,
            blk: 0.0,
            blk_n: 0,
            ncc: 0.0,
        }
    }
}

/// One acceptance test. The tool applies three of these at different
/// strengths rather than one threshold, because the cost of a mistake is not
/// the same everywhere: see `Policy`.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    pub min_aligned_points: u32,
    pub min_frame_overlap: f32,
    pub min_pixel_correlation: f32,
    pub max_scale: f32,
    /// Whether the match's own correspondences must enclose the centre of the
    /// overlap it claims.
    ///
    /// Not a strength setting — a statement about what the evidence is being
    /// asked to support. Only an anchor can put two files into one cluster,
    /// and it does that by claiming a region; correspondences bunched outside
    /// that region extrapolate into it rather than attest to it, however many
    /// of them there are. A corroborated pair claims nothing new — the cluster
    /// already exists — so it is judged on the evidence as found. A match that
    /// fails here is therefore demoted rather than discarded: it cannot create
    /// a cluster, but it can still join one.
    pub centred_evidence: bool,
}

/// The three tests, and why they differ.
///
/// **anchor** is what a pair must clear to be believed on its own. It is the
/// only rule that can put two files into one cluster, so it is the only one
/// whose mistakes can merge two unrelated families — and a wrong merge does
/// not cost one pair, it costs every pair the two families imply. On this
/// corpus a single bad anchor between two beach photographs produced 354
/// false pairs downstream.
///
/// **propagated** judges a transform composed through other matches. No
/// features vouch for it, so it is decided on pixels alone and has to get a
/// clear answer from them.
///
/// **corroborated** applies to a direct match between two files an anchor has
/// already placed in one cluster. Such a match cannot merge anything, so the
/// question is no longer "are these the same picture" but "does this
/// particular pair hold up". It is judged on the evidence as it was found,
/// without the anchor's demand that the evidence enclose what it claims.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub anchor: Rules,
    pub propagated: Rules,
    pub corroborated: Rules,
}

const GRID: usize = 48;
const BLOCK: usize = 8;


/// How much easier it is to be believed *inside* a cluster than to create one.
///
/// A corroborated match cannot merge two families: the anchor that put its two
/// files together already did, or did not. So its mistakes cost one pair where
/// an anchor's cost every pair the two families imply. It gets the same rule
/// with a margin taken off — and this is that margin, stated once, where the
/// old policy spread the same idea over four unrelated-looking offsets.
///
/// These two are the only numbers left in the acceptance rule that were read
/// off a corpus rather than derived, and unlike the rest they earned it: the
/// held-out corpus prefers them too. Without the margin, corroborated pairs
/// face the anchor bar and validation recall falls from 93.1% to 90.7%.
const CLUSTER_SLACK_POINTS: u32 = 2;
const CLUSTER_SLACK_CORRELATION: f32 = 0.1;

impl Default for Rules {
    fn default() -> Self {
        Rules {
            min_aligned_points: 8,
            min_frame_overlap: 0.85,
            min_pixel_correlation: 0.6,
            // A sanity bound, not a tuned one: past a sixteen-fold size ratio
            // the smaller image is a few hundred pixels against a wall.
            max_scale: 16.0,
            centred_evidence: false,
        }
    }
}

impl Policy {
    /// Two rules, because there are two questions — not three, and not nine
    /// numbers pretending to be three.
    ///
    /// The old policy had an anchor tier, a propagated tier and a corroborated
    /// tier, and they disagreed about six things: an inlier bonus of 2 for
    /// anchors, block minimums of 8, 6 and 4, an agreement bar ten points
    /// lower for corroboration, a scale surcharge of 6 per octave for two
    /// tiers and 4 for the third, and an overlap floor of 0.9 for propagation
    /// alone. Every one of those numbers was read off one corpus. The last
    /// two — a floor on comparable blocks, and the scale surcharge — went
    /// later still, once the pixel check stopped aliasing: both existed to
    /// distrust a comparison across a large scale gap, and that distrust was
    /// earned by a sampling bug rather than by the geometry.
    ///
    /// What actually differs is simpler, and it is not the strength of the
    /// evidence — it is **what a wrong answer costs**:
    ///
    ///   * An **anchor** is the only kind of claim that can put two files into
    ///     one cluster. Its mistakes do not cost one pair, they cost every
    ///     pair the two clusters imply; during development two bad anchors
    ///     cost 354 and 3,002 false pairs apiece. So an anchor faces the whole
    ///     rule: enough correspondences, enough overlap, and agreeing pixels.
    ///
    ///   * A **propagated** pair is a transform composed along a chain, and no
    ///     features vouch for it at all. The inlier count is not evidence
    ///     about it, so the pixels have to answer on their own: over the whole
    ///     overlap, correlated, and without a large scale gap to hide a blur
    ///     in.
    ///
    ///   * A **corroborated** pair is a direct match between two files an
    ///     anchor has *already* placed in one cluster. It cannot merge
    ///     anything, so its mistakes cost one pair rather than thousands, and
    ///     it gets the same rule with a margin off: `CLUSTER_SLACK_*` off the
    ///     two bars, and no demand that its correspondences enclose what they
    ///     claim, since that demand exists only to stop evidence from one
    ///     corner of a frame merging two families on the strength of the rest.
    ///
    /// The three tiers survived an attempt to make them two. Folding
    /// corroboration in with propagation looks right — both are claims inside
    /// a cluster — and is wrong, because a corroborated pair *does* have
    /// features vouching for it and a propagated one does not. Holding it to
    /// the pixels-only bar cost 1.8 points of recall on the tuning corpus and
    /// 4.2 on the held-out one. Three tiers is a fact about the evidence, not
    /// a number read off a corpus; the nine numbers were the problem, and they
    /// are gone.
    pub fn new(min_aligned_points: u32, min_frame_overlap: f32, min_pixel_correlation: f32) -> Policy {
        let anchor = Rules {
            min_aligned_points,
            min_frame_overlap,
            min_pixel_correlation,
            centred_evidence: true,
            ..Default::default()
        };
        Policy {
            anchor,
            propagated: Rules {
                min_aligned_points: 0,
                centred_evidence: false,
                ..anchor
            },
            corroborated: Rules {
                min_aligned_points: min_aligned_points.saturating_sub(CLUSTER_SLACK_POINTS),
                min_pixel_correlation: (min_pixel_correlation - CLUSTER_SLACK_CORRELATION).max(0.0),
                centred_evidence: false,
                ..anchor
            },
        }
    }
}

impl Verdict {
    /// Every bar this verdict has to clear, under one tier's rules.
    ///
    /// Two of these are the CLI's `--min-frame-overlap` and
    /// `--min-pixel-correlation`, and the nouns are the whole point: `ov_a`
    /// and `ov_b` are **frames**, pure geometry with no pixel read, so that
    /// floor asks what the transform *claims*; `blk` is the mean of |r| over
    /// the **pixels** of that overlap carrying detail, so that floor asks
    /// whether the claim is *true*. They used to read as a loose and a tight
    /// version of one bar, back when they were `--min-overlap` and
    /// `--min-agreement`, and this comment was three times as long trying to
    /// talk people out of it. They are the only defence against two failure
    /// modes that do not overlap at all, and each is blind to the other's:
    ///
    ///   * Two different photographs on the same page furniture — the corpus's
    ///     two Excel screenshots — map onto each other perfectly. Measured
    ///     over their 29 candidate verdicts: median overlap **1.000**, 93% of
    ///     them past the 0.85 floor. Geometry cannot say no to those. Their
    ///     median `blk` is **0.000**, and only 3% reach 0.50 — but 14% reach
    ///     0.40, which is exactly why the agreement bar merges that family one
    ///     step below where it ships.
    ///
    ///   * A `column_roll` against a crop of its own original agrees on pixels
    ///     almost perfectly, because it *is* the same pixels in the wrong
    ///     place. Over 9,641 such same-seed negatives: median `blk` **0.971**,
    ///     99% past 0.50. Agreement cannot say no to those either. Their
    ///     median overlap is **0.562** and only 8% reach 0.85, because the
    ///     fitted transform explains the rigid fraction and carries the rest
    ///     of the frame outside B.
    ///
    /// So the overlap floor is what insists a transform account for a whole
    /// frame rather than a fragment of one, and the agreement bar is the only
    /// thing standing between the tool and a wrong family merge. Loosening the
    /// first costs traps and merges nothing; loosening the second merges
    /// families and costs almost no traps. The sweeps are in `CLAUDE.md`.
    pub fn accepted(&self, r: &Rules) -> bool {
        self.scale.is_finite()
            && self.scale >= 1.0 / r.max_scale
            && self.scale <= r.max_scale
            && self.n_in >= r.min_aligned_points
            && (!r.centred_evidence || self.centred)
            && self.ov_a.max(self.ov_b) >= r.min_frame_overlap
            && self.blk >= r.min_pixel_correlation
    }
}

// ------------------------------------------------------------ correspondence

/// Squared distance between two descriptors.
///
/// Summed in sixteen independent lanes rather than one running total. The sum
/// is over integers, so it is exact whatever order it is taken in, and the
/// single total was a chain of 128 dependent adds — the loop could not use more
/// than one of the machine's adders at a time. This is the innermost loop of
/// the matcher: every candidate pair runs it once per shared word.
#[inline]
fn dist2(a: &[u8], b: &[u8]) -> u32 {
    const LANES: usize = 16;
    let mut acc = [0u32; LANES];
    for (ca, cb) in a[..DESC_LEN].chunks_exact(LANES).zip(b[..DESC_LEN].chunks_exact(LANES)) {
        for l in 0..LANES {
            let d = ca[l] as i32 - cb[l] as i32;
            acc[l] += (d * d) as u32;
        }
    }
    let mut s = 0u32;
    for l in 0..LANES {
        s += acc[l];
    }
    s
}

/// Nearest neighbour in B for each keypoint of A, over a restricted candidate
/// set.
///
/// `cands` holds, for each keypoint of A, the keypoints of B worth comparing
/// against (from the shared-word index).
///
/// There is no ratio test here. There was: Lowe's, at 0.9, comparing the best
/// match against the runner-up from the same candidate set and dropping the
/// ambiguous ones. It was doing nothing. Swept from 0.7 to 1.0 — which is the
/// whole usable range, 1.0 being no test at all — it moved F1 by 0.001 and the
/// count of false pairs by single digits, on both halves of the corpus
/// independently. The reason is that this tool does not decide anything on a
/// descriptor distance: an ambiguous match is one vote for a transform that
/// then has to explain hundreds of other correspondences and survive a pixel
/// comparison, and a wrong vote loses there. The code already said as much,
/// in the comment excusing single-candidate keypoints from the test it no
/// longer has.
pub fn correspond(
    a: &Features,
    b: &Features,
    cands: &[(u32, u32)],
    out: &mut Vec<(u32, u32)>,
) {
    out.clear();
    if cands.is_empty() {
        return;
    }
    let mut i = 0;
    while i < cands.len() {
        let qi = cands[i].0;
        let mut best = (u32::MAX, 0u32);
        let da = a.d(qi as usize);
        while i < cands.len() && cands[i].0 == qi {
            let tj = cands[i].1;
            let d = dist2(da, b.d(tj as usize));
            if d < best.0 {
                best = (d, tj);
            }
            i += 1;
        }
        if best.0 != u32::MAX {
            out.push((qi, best.1));
        }
    }
}

// ------------------------------------------------------------ geometry

#[inline]
fn apply(m: &Affine, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}

/// The similarity transform implied by one keypoint correspondence.
fn from_single(ka: &Keypoint, kb: &Keypoint) -> Affine {
    let s = kb.sigma / ka.sigma.max(1e-6);
    let th = (kb.angle - ka.angle).to_radians();
    let (sn, cs) = th.sin_cos();
    let (c, sn) = (cs * s, sn * s);
    [c, -sn, kb.x - (c * ka.x - sn * ka.y), sn, c, kb.y - (sn * ka.x + c * ka.y)]
}

/// Least-squares affine through the given correspondences.
fn fit_affine(a: &Features, b: &Features, pairs: &[(u32, u32)], mask: &[bool]) -> Option<Affine> {
    // Normal equations for [x y 1] -> x' and -> y'.
    let (mut sxx, mut sxy, mut sx, mut syy, mut sy, mut n) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    let (mut tx1, mut tx2, mut tx3) = (0f64, 0f64, 0f64);
    let (mut ty1, mut ty2, mut ty3) = (0f64, 0f64, 0f64);
    for (k, &(i, j)) in pairs.iter().enumerate() {
        if !mask[k] {
            continue;
        }
        let (x, y) = (a.kps[i as usize].x as f64, a.kps[i as usize].y as f64);
        let (u, v) = (b.kps[j as usize].x as f64, b.kps[j as usize].y as f64);
        sxx += x * x;
        sxy += x * y;
        sx += x;
        syy += y * y;
        sy += y;
        n += 1.0;
        tx1 += x * u;
        tx2 += y * u;
        tx3 += u;
        ty1 += x * v;
        ty2 += y * v;
        ty3 += v;
    }
    if n < 3.0 {
        return None;
    }
    let g = [[sxx, sxy, sx], [sxy, syy, sy], [sx, sy, n]];
    let r1 = solve3(g, [tx1, tx2, tx3])?;
    let r2 = solve3(g, [ty1, ty2, ty3])?;
    Some([r1[0] as f32, r1[1] as f32, r1[2] as f32, r2[0] as f32, r2[1] as f32, r2[2] as f32])
}

fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let mut out = [0f64; 3];
    for i in 0..3 {
        let mut m = a;
        for r in 0..3 {
            m[r][i] = b[r];
        }
        let d = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        out[i] = d * inv;
    }
    Some(out)
}

pub fn invert_affine(m: &Affine) -> Option<Affine> {
    let det = m[0] * m[4] - m[1] * m[3];
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let (a, b, c, d) = (m[4] * inv, -m[1] * inv, -m[3] * inv, m[0] * inv);
    Some([a, b, -(a * m[2] + b * m[5]), c, d, -(c * m[2] + d * m[5])])
}

/// M2 after M1: the transform taking A to C given A->B and B->C.
pub fn compose(m1: &Affine, m2: &Affine) -> Affine {
    [
        m2[0] * m1[0] + m2[1] * m1[3],
        m2[0] * m1[1] + m2[1] * m1[4],
        m2[0] * m1[2] + m2[1] * m1[5] + m2[2],
        m2[3] * m1[0] + m2[4] * m1[3],
        m2[3] * m1[1] + m2[4] * m1[4],
        m2[3] * m1[2] + m2[4] * m1[5] + m2[5],
    ]
}

/// Best transform explaining the correspondences, by inlier count.
fn best_transform(a: &Features, b: &Features, pairs: &[(u32, u32)], bw: f32, bh: f32, scratch: &mut Scratch) -> Option<(Affine, Vec<bool>)> {
    if pairs.len() < 3 {
        return None;
    }
    let tol = (0.015 * (bw * bw + bh * bh).sqrt()).max(3.0);
    let tol2 = tol * tol;
    let n = pairs.len();
    // The inlier count is taken once per hypothesis and there is one hypothesis
    // per correspondence, so these coordinates are read `n` times each. Reading
    // them from four flat arrays rather than through two index indirections
    // into a struct of five fields is the difference between a loop the
    // compiler can vectorise and one it cannot; the arithmetic is unchanged.
    let sc = &mut *scratch;
    sc.ax.clear();
    sc.ay.clear();
    sc.bx.clear();
    sc.by.clear();
    for &(p, q) in pairs.iter() {
        let ka = &a.kps[p as usize];
        let kb = &b.kps[q as usize];
        sc.ax.push(ka.x);
        sc.ay.push(ka.y);
        sc.bx.push(kb.x);
        sc.by.push(kb.y);
    }
    let (ax, ay, bx, by) = (&sc.ax[..n], &sc.ay[..n], &sc.bx[..n], &sc.by[..n]);
    sc.mask.clear();
    sc.mask.resize(n, false);
    sc.hit.clear();
    sc.hit.resize(n, false);
    let mut best: Option<(usize, Affine)> = None;
    // Every correspondence is a hypothesis. On this scale that is cheaper and
    // more reliable than random sampling: no iteration count to tune, and a
    // single good match is enough to find the answer.
    let cap = n.min(600);
    for &(i, j) in pairs.iter().take(cap) {
        let m = from_single(&a.kps[i as usize], &b.kps[j as usize]);
        let s = (m[0] * m[4] - m[1] * m[3]).abs().sqrt();
        if !(s.is_finite() && s > 1e-3 && s < 1e3) {
            continue;
        }
        // What this hypothesis would have to reach to be kept at all. Almost
        // none of them get near it — a wrong transform explains two or three
        // correspondences out of hundreds — and a hypothesis that cannot win
        // is not worth finishing. See `count_inliers`.
        let need = best.map_or(3, |(c, _)| c + 1);
        let count = count_inliers(&m, ax, ay, bx, by, tol2, need);
        if count >= need {
            // A hypothesis that beats the best so far is rare, and it is the
            // only kind whose mask anyone reads.
            let marked = mark_inliers(&m, ax, ay, bx, by, tol2, &mut sc.hit);
            debug_assert_eq!(marked, count);
            best = Some((marked, m));
            sc.mask.copy_from_slice(&sc.hit);
        }
    }
    let (_, mut m) = best?;
    // Refine: least-squares affine on the inliers, recount, repeat while the
    // set does not shrink.
    for _ in 0..3 {
        let held = sc.mask.iter().filter(|v| **v).count();
        let Some(m2) = fit_affine(a, b, pairs, &sc.mask) else { break };
        let count = count_inliers(&m2, ax, ay, bx, by, tol2, held);
        if count < held {
            break;
        }
        mark_inliers(&m2, ax, ay, bx, by, tol2, &mut sc.hit);
        m = m2;
        sc.mask.copy_from_slice(&sc.hit);
    }
    Some((m, sc.mask.clone()))
}

/// How many correspondences a transform explains, at a fixed tolerance.
///
/// `need` is the count below which the caller discards the answer. Once the
/// correspondences still to be tested cannot carry the running total that far,
/// the rest of them cannot change what the caller does, and the sweep stops.
/// The returned count is then short of the truth — and short of `need`, which
/// is all the caller reads it for.
///
/// This is where the geometry stage spends itself: one hypothesis per
/// correspondence, each scored against every correspondence. Almost all of
/// them are wrong and explain a handful of points, so almost all of them are
/// settled in the first chunk.
///
/// **It does not record which ones.** It used to, and the byte it wrote per
/// correspondence was the only scattered store in an otherwise wide loop —
/// eight positions map, subtract and compare in a vector, and then one lane at
/// a time went out to a `bool`. The caller needs the mask for the hypothesis it
/// keeps and for no other, and a hypothesis is kept only when it beats every
/// one before it, so `mark_inliers` takes a second pass over the handful that
/// win. Counting alone is what the other thousands get.
#[inline]
fn count_inliers(m: &Affine, ax: &[f32], ay: &[f32], bx: &[f32], by: &[f32], tol2: f32, need: usize) -> usize {
    const CHUNK: usize = 64;
    let n = ax.len();
    let mut count = 0usize;
    let mut k = 0usize;
    while k < n {
        let end = (k + CHUNK).min(n);
        for t in k..end {
            let px = m[0] * ax[t] + m[1] * ay[t] + m[2];
            let py = m[3] * ax[t] + m[4] * ay[t] + m[5];
            let (dx, dy) = (px - bx[t], py - by[t]);
            count += (dx * dx + dy * dy < tol2) as usize;
        }
        k = end;
        if count + (n - k) < need {
            return count;
        }
    }
    count
}

/// The same test, recording which correspondences passed it. Run only for a
/// hypothesis the caller is keeping, so it never stops early: the mask has to
/// cover every correspondence, and the count it returns is the whole count.
#[inline]
fn mark_inliers(m: &Affine, ax: &[f32], ay: &[f32], bx: &[f32], by: &[f32], tol2: f32, hit: &mut [bool]) -> usize {
    let mut count = 0usize;
    for t in 0..ax.len() {
        let px = m[0] * ax[t] + m[1] * ay[t] + m[2];
        let py = m[3] * ax[t] + m[4] * ay[t] + m[5];
        let (dx, dy) = (px - bx[t], py - by[t]);
        let ok = dx * dx + dy * dy < tol2;
        hit[t] = ok;
        count += ok as usize;
    }
    count
}

/// Inliers counted once per distinct source position.
fn distinct_inliers(a: &Features, pairs: &[(u32, u32)], mask: &[bool]) -> u32 {
    let mut pts: Vec<(i32, i32)> = pairs
        .iter()
        .zip(mask)
        .filter(|&(_, &m)| m)
        .map(|(&(i, _), _)| {
            let k = &a.kps[i as usize];
            ((k.x * 2.0) as i32, (k.y * 2.0) as i32)
        })
        .collect();
    pts.sort_unstable();
    pts.dedup();
    pts.len() as u32
}

/// Whether the correspondences actually reach around the region they claim.
///
/// A fitted transform is evidence about the region its inliers came from.
/// Inside their bounding box it interpolates; outside it extrapolates. Two
/// photographs laid out on the same page furniture match along the furniture —
/// a rule, a margin, a caption — and the transform then claims the whole page,
/// and with it the photograph, on the strength of evidence that never touched
/// it. Seventeen such anchors accounted for every cross-family error this tool
/// made, each claiming a region from inliers spanning a median 14% of it.
///
/// The test is that the inliers bracket the middle of that region — the
/// weakest way to say "this transform is interpolating where it matters". It
/// asks about position, not degree, so it needs no magnitude and adds no
/// constant.
///
/// Two details keep it from rejecting honest matches. The middle is taken over
/// the *keypoints* inside the claimed region rather than over its area,
/// because a photograph of a building under a clear sky has nothing to match
/// in its top half and evidence from the bottom half is not thereby
/// one-sided. And it is asked of both frames and passes on either, because a
/// photograph inside a slide legitimately has all its evidence in one corner
/// of the slide: that is what containment looks like.
fn encloses_centre(a: &Features, b: &Features, m: &Affine, pairs: &[(u32, u32)], mask: &[bool]) -> bool {
    let (aw, ah) = (a.w as f32, a.h as f32);
    let (bw, bh) = (b.w as f32, b.h as f32);

    // Middle of the evidence that was available inside the claimed region.
    let mut ca = (0f32, 0f32, 0f32);
    for k in &a.kps {
        let (u, v) = apply(m, k.x, k.y);
        if u >= 0.0 && u < bw && v >= 0.0 && v < bh {
            ca = (ca.0 + k.x, ca.1 + k.y, ca.2 + 1.0);
        }
    }
    let mut cb = (0f32, 0f32, 0f32);
    if let Some(mi) = invert_affine(m) {
        for k in &b.kps {
            let (u, v) = apply(&mi, k.x, k.y);
            if u >= 0.0 && u < aw && v >= 0.0 && v < ah {
                cb = (cb.0 + k.x, cb.1 + k.y, cb.2 + 1.0);
            }
        }
    }

    let mut ax = (f32::MAX, f32::MIN);
    let mut ay = (f32::MAX, f32::MIN);
    let mut bx = (f32::MAX, f32::MIN);
    let mut by = (f32::MAX, f32::MIN);
    for (k, &(i, j)) in pairs.iter().enumerate() {
        if !mask[k] {
            continue;
        }
        let (p, q) = (&a.kps[i as usize], &b.kps[j as usize]);
        ax = (ax.0.min(p.x), ax.1.max(p.x));
        ay = (ay.0.min(p.y), ay.1.max(p.y));
        bx = (bx.0.min(q.x), bx.1.max(q.x));
        by = (by.0.min(q.y), by.1.max(q.y));
    }
    let holds = |sp: (f32, f32), c: f32| sp.0 <= c && c <= sp.1;
    let side = |c: (f32, f32, f32), x: (f32, f32), y: (f32, f32)| {
        c.2 > 0.0 && holds(x, c.0 / c.2) && holds(y, c.1 / c.2)
    };
    side(ca, ax, ay) || side(cb, bx, by)
}

/// Fraction of each frame that maps inside the other.
fn overlap(m: &Affine, aw: f32, ah: f32, bw: f32, bh: f32) -> (f32, f32) {
    const N: usize = 16;
    let mut inside = 0;
    for iy in 0..N {
        for ix in 0..N {
            let x = (ix as f32 + 0.5) / N as f32 * aw;
            let y = (iy as f32 + 0.5) / N as f32 * ah;
            let (u, v) = apply(m, x, y);
            if u >= 0.0 && u < bw && v >= 0.0 && v < bh {
                inside += 1;
            }
        }
    }
    let ov_a = inside as f32 / (N * N) as f32;
    let Some(mi) = invert_affine(m) else { return (ov_a, 0.0) };
    let mut inside = 0;
    for iy in 0..N {
        for ix in 0..N {
            let x = (ix as f32 + 0.5) / N as f32 * bw;
            let y = (iy as f32 + 0.5) / N as f32 * bh;
            let (u, v) = apply(&mi, x, y);
            if u >= 0.0 && u < aw && v >= 0.0 && v < ah {
                inside += 1;
            }
        }
    }
    (ov_a, inside as f32 / (N * N) as f32)
}

// ------------------------------------------------------------ pixels

/// Thumbnail kept for pixel verification: small, blurred once at build time.
///
/// Carries a mip pyramid, because the pixel check compares two views of the
/// same region that are almost never at the same resolution. Reading both with
/// plain bilinear taps samples whichever side is finer far below its Nyquist
/// rate, and an aliased view does not correlate with a properly filtered one —
/// so the check reported disagreement for a difference the *sampling* had
/// introduced. The pyramid is derived from `px` and is not serialised; the
/// cache rebuilds it on load.
#[derive(Clone, Debug, Default)]
pub struct Thumb {
    pub w: u16,
    pub h: u16,
    /// Scale from working-image coordinates to thumbnail coordinates.
    pub scale: f32,
    pub px: Vec<u8>,
    /// Half-resolution levels above `px`: level i has been halved i+1 times.
    mips: Vec<(u16, u16, Vec<u8>)>,
}

impl Thumb {
    pub fn build(g: &Gray, long: usize) -> Thumb {
        // `fit_to` takes ownership, and the working image is wanted whole
        // elsewhere; resampling from a borrow avoids copying it first.
        let long_side = g.w.max(g.h);
        let owned;
        let t: &Gray = if long == 0 || long_side <= long {
            g
        } else {
            let s = long as f32 / long_side as f32;
            let tw = ((g.w as f32 * s).round() as usize).max(1);
            let th = ((g.h as f32 * s).round() as usize).max(1);
            owned = crate::decode::resize_area(g, tw, th);
            &owned
        };
        let scale = t.w as f32 / g.w as f32;
        Thumb::new(
            t.w as u16,
            t.h as u16,
            scale,
            t.px.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect(),
        )
    }

    /// Build a thumbnail and its pyramid. Used by `build` and by the cache,
    /// which stores only level zero.
    pub fn new(w: u16, h: u16, scale: f32, px: Vec<u8>) -> Thumb {
        let mut mips: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        let (mut cw, mut ch) = (w as usize, h as usize);
        while cw >= 4 && ch >= 4 {
            let (nw, nh) = (cw / 2, ch / 2);
            let cur: &[u8] = match mips.last() {
                None => &px,
                Some((_, _, p)) => p,
            };
            let mut next = Vec::with_capacity(nw * nh);
            for y in 0..nh {
                next.extend((0..nw).map(|x| {
                    let i = 2 * y * cw + 2 * x;
                    let s = cur[i] as u32
                        + cur[i + 1] as u32
                        + cur[i + cw] as u32
                        + cur[i + cw + 1] as u32;
                    ((s + 2) / 4) as u8
                }));
            }
            mips.push((nw as u16, nh as u16, next));
            cw = nw;
            ch = nh;
        }
        Thumb { w, h, scale, px, mips }
    }

    /// Where a coordinate lands in a level: the pixel below it and the
    /// fraction past it, with the clamp the tap used to carry.
    ///
    /// Written as comparisons rather than `clamp` so that the result is a
    /// number even when the input is not — `clamp` propagates a NaN, and the
    /// integer conversion below is only sound on a value known to be in range.
    /// For every finite input it is the same clamp and the same split.
    #[inline]
    fn split(v: f32, n: usize) -> (usize, f32) {
        let hi = n as f32 - 1.001;
        let v = if v > 0.0 {
            if v < hi { v } else { hi }
        } else {
            0.0
        };
        // `v` is now in [0, n - 1.001] and finite, so the truncation cannot
        // saturate. Saying so drops the range check and the conditional move
        // that a plain `as usize` carries — this runs four times for every one
        // of the two thousand grid samples of every pair considered.
        let i = unsafe { v.to_int_unchecked::<usize>() };
        (i, v - i as f32)
    }

    /// Bilinear tap from a row pair already located: the four reads and the
    /// three interpolations, and nothing else.
    #[inline]
    fn lerp(px: &[u8], w: usize, i: usize, fx: f32, fy: f32) -> f32 {
        // The caller's `split` put the row and column inside the level, so the
        // four taps are inside `px`, which is w*h long.
        debug_assert!(i + w + 1 < px.len());
        let (p00, p01, p10, p11) = unsafe {
            (
                *px.get_unchecked(i) as f32,
                *px.get_unchecked(i + 1) as f32,
                *px.get_unchecked(i + w) as f32,
                *px.get_unchecked(i + w + 1) as f32,
            )
        };
        let a = p00 * (1.0 - fx) + p01 * fx;
        let b = p10 * (1.0 - fx) + p11 * fx;
        a * (1.0 - fy) + b * fy
    }

    /// Bilinear tap into one level of the pyramid.
    #[inline]
    fn tap(px: &[u8], w: usize, h: usize, x: f32, y: f32) -> f32 {
        if w < 2 || h < 2 {
            return px.first().copied().unwrap_or(0) as f32;
        }
        let (x0, fx) = Thumb::split(x, w);
        let (y0, fy) = Thumb::split(y, h);
        Thumb::lerp(px, w, y0 * w + x0, fx, fy)
    }

    /// Pick the pyramid levels to read for a given sample footprint.
    ///
    /// `footprint` is how far apart consecutive samples of the comparison grid
    /// land in *this* thumbnail's pixels, so it is the width of the box each
    /// sample should represent. It is fixed for a whole comparison, so the
    /// choice is made once here rather than per sample.
    fn lod(&self, footprint: f32) -> Lod<'_> {
        let whole = Lod {
            lo: (&self.px[..], self.w as usize, self.h as usize, 1.0),
            hi: None,
            t: 0.0,
        };
        if !(footprint > 1.0) || self.mips.is_empty() {
            return whole;
        }
        let l = footprint.log2();
        let li = (l as usize).min(self.mips.len());
        let lo: (&[u8], usize, usize, f32) = if li == 0 {
            (&self.px[..], self.w as usize, self.h as usize, 1.0)
        } else {
            let (w, h, ref p) = self.mips[li - 1];
            (&p[..], w as usize, h as usize, 1.0 / (1 << li) as f32)
        };
        if li >= self.mips.len() {
            return Lod { lo, hi: None, t: 0.0 };
        }
        // Blending into the next level keeps a footprint that drifts across a
        // power of two from stepping the measurement.
        let (w, h, ref p) = self.mips[li];
        Lod {
            lo,
            hi: Some((&p[..], w as usize, h as usize, 1.0 / (1 << (li + 1)) as f32)),
            t: l - li as f32,
        }
    }
}

/// Two pyramid levels and the weight between them, chosen once per comparison.
struct Lod<'a> {
    lo: (&'a [u8], usize, usize, f32),
    hi: Option<(&'a [u8], usize, usize, f32)>,
    t: f32,
}

impl Lod<'_> {
    #[inline]
    fn at(&self, x: f32, y: f32) -> f32 {
        let (p, w, h, f) = self.lo;
        let a = Thumb::tap(p, w, h, x * f, y * f);
        match self.hi {
            None => a,
            Some((p, w, h, f)) => {
                let b = Thumb::tap(p, w, h, x * f, y * f);
                a + (b - a) * self.t
            }
        }
    }
}

/// Where the comparison grid lands in one thumbnail, one axis at a time.
///
/// The grid is walked in A's own frame, so a sample's column in A's thumbnail
/// depends only on `ix` and its row only on `iy`. Locating a sample — clamping
/// it into the level, truncating to a pixel, taking the fraction past it — is
/// therefore ninety-six pieces of arithmetic per level, not two thousand three
/// hundred; what is left per sample is the four reads and the three
/// interpolations that actually look at the picture. The B side has a rotation
/// in it and gets no such reduction.
struct AxisTaps {
    i: [u32; GRID],
    f: [f32; GRID],
}

impl AxisTaps {
    /// Sample `k` sits at `((start + span * k / (GRID - 1)) * scale) * f`.
    ///
    /// Spelled in exactly that order, and with the thumbnail's scale and the
    /// level's kept apart, because multiplication of floats is not
    /// associative: folding the two into one factor moves the last bit of some
    /// sample positions, and a sample that lands a bit either side of a pixel
    /// boundary is read from a different pair of pixels. That is a different
    /// answer, not a rounder one.
    fn of(start: f32, span: f32, scale: f32, f: f32, n: usize) -> AxisTaps {
        let mut t = AxisTaps { i: [0; GRID], f: [0.0; GRID] };
        for k in 0..GRID {
            let p = (start + span * k as f32 / (GRID - 1) as f32) * scale * f;
            let (i, fr) = Thumb::split(p, n);
            t.i[k] = i as u32;
            t.f[k] = fr;
        }
        t
    }
}

/// One pyramid level's grid taps, or a note that the level is too small to
/// interpolate in — where `Thumb::tap` returns the single pixel it has.
enum LevelTaps {
    Degenerate(f32),
    Grid { x: AxisTaps, y: AxisTaps },
}

impl LevelTaps {
    fn of(lvl: (&[u8], usize, usize, f32), gx: (f32, f32), gy: (f32, f32), scale: f32) -> LevelTaps {
        let (px, w, h, f) = lvl;
        if w < 2 || h < 2 {
            return LevelTaps::Degenerate(px.first().copied().unwrap_or(0) as f32);
        }
        LevelTaps::Grid {
            x: AxisTaps::of(gx.0, gx.1, scale, f, w),
            y: AxisTaps::of(gy.0, gy.1, scale, f, h),
        }
    }

    #[inline]
    fn at(&self, px: &[u8], w: usize, ix: usize, iy: usize) -> f32 {
        match self {
            LevelTaps::Degenerate(v) => *v,
            LevelTaps::Grid { x, y } => {
                let i = y.i[iy] as usize * w + x.i[ix] as usize;
                Thumb::lerp(px, w, i, x.f[ix], y.f[iy])
            }
        }
    }
}

/// Both levels of one `Lod`, with the grid resolved against each.
struct GridTaps {
    lo: LevelTaps,
    hi: Option<LevelTaps>,
}

impl GridTaps {
    fn of(l: &Lod, gx: (f32, f32), gy: (f32, f32), scale: f32) -> GridTaps {
        GridTaps {
            lo: LevelTaps::of(l.lo, gx, gy, scale),
            hi: l.hi.map(|h| LevelTaps::of(h, gx, gy, scale)),
        }
    }

    #[inline]
    fn at(&self, l: &Lod, ix: usize, iy: usize) -> f32 {
        let a = self.lo.at(l.lo.0, l.lo.1, ix, iy);
        match (&self.hi, l.hi) {
            (Some(t), Some(h)) => {
                let b = t.at(h.0, h.1, ix, iy);
                a + (b - a) * l.t
            }
            _ => a,
        }
    }
}

/// Resample the overlap from both thumbnails and compare it blockwise.
///
/// Returns (agreement, comparable blocks, whole-overlap correlation).
///
/// Blocks flat in both images abstain: a white margin matching a white margin
/// is not evidence of anything. Correlation is signed-absolute per block so a
/// contrast inversion inside one region does not fail an otherwise exact
/// match, while the block *positions* still have to line up — which is what
/// the shuffled-tiling trap gets wrong.
fn pixel_check(
    ta: &Thumb,
    tb: &Thumb,
    m: &Affine,
    aw: f32,
    ah: f32,
    bw: f32,
    bh: f32,
    invert: bool,
) -> (f32, u32, f32) {
    // Bounding box, in A's frame, of the part of A that lands inside B.
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    const P: usize = 24;
    for iy in 0..P {
        for ix in 0..P {
            let x = (ix as f32 + 0.5) / P as f32 * aw;
            let y = (iy as f32 + 0.5) / P as f32 * ah;
            let (u, v) = apply(m, x, y);
            if u >= 0.0 && u < bw && v >= 0.0 && v < bh {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !(x1 > x0 && y1 > y0) {
        return (0.0, 0, 0.0);
    }
    let mut va = [0f32; GRID * GRID];
    let mut vb = [0f32; GRID * GRID];
    let mut ok = [false; GRID * GRID];

    // How far apart consecutive grid samples land in each thumbnail, and hence
    // how wide a box each sample stands for. The two are almost never equal:
    // a photograph matched into a slide is read densely on one side and across
    // a handful of pixels on the other.
    let step_a = ((x1 - x0) / (GRID - 1) as f32).max((y1 - y0) / (GRID - 1) as f32);
    let lin = (m[0] * m[4] - m[1] * m[3]).abs().sqrt();
    let raw_a = (step_a * ta.scale).max(1e-6);
    let raw_b = (step_a * lin * tb.scale).max(1e-6);
    // Filtering each side over its own footprint puts both at one sample per
    // grid cell. That is enough when both thumbnails hold at least one pixel
    // per cell — but a photograph filling a slide corner may occupy fewer
    // thumbnail pixels than the grid has cells, and then its samples are
    // interpolation rather than detail. Correlating the other side's real
    // detail against that measures the gap in resolution, not a difference in
    // content, so the sharper side is taken down to what the blunter one can
    // actually show.
    let lod_a = ta.lod(raw_a);
    let lod_b = tb.lod(raw_b);

    let grid_a = GridTaps::of(&lod_a, (x0, x1 - x0), (y0, y1 - y0), ta.scale);

    for iy in 0..GRID {
        let y = y0 + (y1 - y0) * iy as f32 / (GRID - 1) as f32;
        for ix in 0..GRID {
            let x = x0 + (x1 - x0) * ix as f32 / (GRID - 1) as f32;
            let (u, v) = apply(m, x, y);
            if u < 0.0 || u >= bw || v < 0.0 || v >= bh {
                continue;
            }
            let k = iy * GRID + ix;
            let s = grid_a.at(&lod_a, ix, iy);
            // An inverted match is compared against the inverse of A rather
            // than by keeping a second copy of every thumbnail.
            va[k] = if invert { 255.0 - s } else { s };
            vb[k] = lod_b.at(u * tb.scale, v * tb.scale);
            ok[k] = true;
        }
    }
    let mut agree = 0f32;
    let mut total = 0u32;
    for by in (0..GRID).step_by(BLOCK) {
        for bx in (0..GRID).step_by(BLOCK) {
            let mut n = 0usize;
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0f64, 0f64, 0f64, 0f64, 0f64);
            for y in by..by + BLOCK {
                for x in bx..bx + BLOCK {
                    let k = y * GRID + x;
                    if !ok[k] {
                        continue;
                    }
                    n += 1;
                    let (p, q) = (va[k] as f64, vb[k] as f64);
                    sa += p;
                    sb += q;
                    saa += p * p;
                    sbb += q * q;
                    sab += p * q;
                }
            }
            if n < BLOCK * BLOCK {
                continue;
            }
            let nf = n as f64;
            let vara = saa - sa * sa / nf;
            let varb = sbb - sb * sb / nf;
            let flat = 4.0 * 4.0 * nf;
            if vara < flat || varb < flat {
                // A block with no detail on either side cannot disagree, so
                // counting it as agreement is counting nothing as evidence.
                // It abstains: out of the numerator and out of the
                // denominator both. This is what stops a thumbnail matched
                // into a tenth of a large image — where the large side
                // resolves to a smear — from scoring a confident agreement on
                // blur alone.
                continue;
            }
            total += 1;
            let cov = sab - sa * sb / nf;
            // How well this block agrees, not whether it does. Scoring each
            // block against a cut and then counting the blocks that cleared it
            // takes two numbers to say one thing, and throws away the
            // difference between a block that just failed and one that matched
            // nothing at all. The mean keeps it, and needs no cut.
            agree += (cov / (vara * varb).sqrt()).abs() as f32;
        }
    }
    // Whole-overlap correlation, for reporting.
    let (mut sa, mut sb, mut saa, mut sbb, mut sab, mut n) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    for k in 0..GRID * GRID {
        if !ok[k] {
            continue;
        }
        let (p, q) = (va[k] as f64, vb[k] as f64);
        sa += p;
        sb += q;
        saa += p * p;
        sbb += q * q;
        sab += p * q;
        n += 1.0;
    }
    let ncc = if n > 8.0 {
        let vara = saa - sa * sa / n;
        let varb = sbb - sb * sb / n;
        if vara > 0.0 && varb > 0.0 {
            ((sab - sa * sb / n) / (vara * varb).sqrt()) as f32
        } else {
            0.0
        }
    } else {
        0.0
    };
    (if total > 0 { agree / total as f32 } else { 0.0 }, total, ncc)
}

// ------------------------------------------------------------ entry points

/// Reusable working buffers for one verification. Sized to the largest pair a
/// worker has seen, so the geometry stage allocates nothing per pair.
#[derive(Default)]
pub struct Scratch {
    ax: Vec<f32>,
    ay: Vec<f32>,
    bx: Vec<f32>,
    by: Vec<f32>,
    mask: Vec<bool>,
    hit: Vec<bool>,
}

pub struct Pair<'a> {
    pub fa: &'a Features,
    pub fb: &'a Features,
    pub ta: &'a Thumb,
    pub tb: &'a Thumb,
}

/// Full verification from a candidate correspondence list.
/// Full verification from a candidate correspondence list.
///
/// `gate` is the weakest (inliers, overlap) any consumer of this verdict will
/// accept. Below it the verdict is discarded whatever the pixels say, so the
/// pixels are not read: the check is the most expensive thing in the pipeline
/// and a third of the pairs reaching it have already lost on geometry.
pub fn verify(p: &Pair, cands: &[(u32, u32)], var: Variant, gate: (u32, f32), matches: &mut Vec<(u32, u32)>, scratch: &mut Scratch) -> Verdict {
    let mut v = Verdict { variant: var, ..Default::default() };
    timed!(16, correspond(p.fa, p.fb, cands, matches));
    v.n_match = matches.len() as u32;
    let (bw, bh) = (p.fb.w as f32, p.fb.h as f32);
    let (aw, ah) = (p.fa.w as f32, p.fa.h as f32);
    let Some((m, mask)) = timed!(17, best_transform(p.fa, p.fb, matches, bw, bh, scratch)) else { return v };
    // `p.fa` is the query image already mirrored, so `m` maps mirrored-A
    // coordinates into B. Composing the mirror back in gives a transform from
    // A's own coordinates, which is what the rest of the tool stores, checks
    // and composes.
    // `encloses_centre` compares inlier positions against the centre of the
    // overlap, so it needs the transform in the same frame the inliers are
    // recorded in — `p.fa`'s, which is the mirrored one.
    let m_query = m;
    let m = if var.mirror { compose(&mirror_affine(aw), &m) } else { m };
    v.m = m;
    v.n_in = distinct_inliers(p.fa, matches, &mask);
    v.centred = timed!(18, encloses_centre(p.fa, p.fb, &m_query, matches, &mask));
    v.scale = (m[0] * m[4] - m[1] * m[3]).abs().sqrt();
    v.rot_deg = m[3].atan2(m[0]).to_degrees();
    let (oa, ob) = timed!(19, overlap(&m, aw, ah, bw, bh));
    v.ov_a = oa;
    v.ov_b = ob;
    if v.n_in >= gate.0.max(3) && v.ov_a.max(v.ov_b) >= gate.1 {
        let (blk, n, ncc) = timed!(20, pixel_check(p.ta, p.tb, &m, aw, ah, bw, bh, var.invert));
        v.blk = blk;
        v.blk_n = n;
        v.ncc = ncc;
    }
    v
}

/// Check a transform that came from somewhere else — composed through a third
/// image — using pixels only. No descriptor matching, so it costs almost
/// nothing, and it is still a real test of this pair rather than an assumption
/// that matching is transitive.
pub fn verify_transform(p: &Pair, m: &Affine, var: Variant, min_ov: f32) -> Verdict {
    let mut v = Verdict { m: *m, variant: var, ..Default::default() };
    let (aw, ah) = (p.fa.w as f32, p.fa.h as f32);
    let (bw, bh) = (p.fb.w as f32, p.fb.h as f32);
    v.scale = (m[0] * m[4] - m[1] * m[3]).abs().sqrt();
    if !(v.scale.is_finite() && v.scale > 1e-3) {
        return v;
    }
    v.rot_deg = m[3].atan2(m[0]).to_degrees();
    let (oa, ob) = overlap(m, aw, ah, bw, bh);
    v.ov_a = oa;
    v.ov_b = ob;
    if v.ov_a.max(v.ov_b) >= min_ov {
        let (blk, n, ncc) = timed!(43, pixel_check(p.ta, p.tb, m, aw, ah, bw, bh, var.invert));
        v.blk = blk;
        v.blk_n = n;
        v.ncc = ncc;
    }
    v
}

