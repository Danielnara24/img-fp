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
//! 1. **Correspondences.** Descriptors that match across the two images,
//!    filtered by Lowe's ratio test.
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
    pub n_match: u32,
    /// Fraction of A's frame that lands inside B, and the reverse.
    pub ov_a: f32,
    pub ov_b: f32,
    pub scale: f32,
    pub rot_deg: f32,
    /// Fraction of comparable blocks whose content agrees.
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
    pub min_inliers: u32,
    pub min_overlap: f32,
    pub min_block_agreement: f32,
    pub min_blocks: u32,
    pub min_ncc: f32,
    pub max_scale: f32,
    /// Extra inliers demanded per octave of scale gap beyond 4x.
    pub inliers_per_octave: u32,
    /// Largest scale gap, in octaves, a claim may rest on.
    pub max_gap_octaves: f32,
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
/// **corroborated** is the weakest, and only ever applies to a pair whose two
/// files are *already* in one cluster. Such a match cannot merge anything:
/// the question is no longer "are these the same picture" but "does this
/// particular pair, inside a cluster we already believe, hold up". A
/// threshold that would be reckless as an anchor is reasonable there.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub anchor: Rules,
    pub propagated: Rules,
    pub corroborated: Rules,
}

impl Default for Rules {
    fn default() -> Self {
        Rules {
            min_inliers: 8,
            min_overlap: 0.85,
            min_block_agreement: 0.6,
            min_blocks: 8,
            min_ncc: 0.0,
            max_scale: 16.0,
            inliers_per_octave: 6,
            max_gap_octaves: 64.0,
        }
    }
}

const PROP_NCC: f32 = 0.7;
const PROP_GAP: f32 = 3.0;

impl Policy {
    pub fn new(min_inliers: u32, min_overlap: f32, min_agreement: f32) -> Policy {
        Policy {
            anchor: Rules {
                min_inliers: min_inliers + 2,
                min_overlap,
                min_block_agreement: min_agreement,
                min_blocks: 8,
                ..Default::default()
            },
            propagated: Rules {
                min_inliers: 0,
                min_overlap: min_overlap.max(0.9),
                min_block_agreement: min_agreement,
                min_blocks: 6,
                min_ncc: PROP_NCC,
                max_gap_octaves: PROP_GAP,
                ..Default::default()
            },
            corroborated: Rules {
                min_inliers,
                min_overlap,
                min_block_agreement: (min_agreement - 0.1).max(0.0),
                min_blocks: 4,
                inliers_per_octave: 4,
                ..Default::default()
            },
        }
    }
}

impl Verdict {
    /// Inliers demanded of this pair. A match across a large scale gap sees
    /// the smaller image against a blurred fraction of the larger one, where
    /// coincidences are cheap, so the bar rises with the gap.
    fn required_inliers(&self, r: &Rules) -> u32 {
        let gap = self.gap_octaves();
        if gap <= 2.0 {
            r.min_inliers
        } else {
            r.min_inliers + ((gap - 2.0) * r.inliers_per_octave as f32).round() as u32
        }
    }

    /// Size difference between the two views, in octaves.
    pub fn gap_octaves(&self) -> f32 {
        self.scale.max(1.0 / self.scale.max(1e-9)).max(1.0).log2()
    }

    pub fn accepted(&self, r: &Rules) -> bool {
        self.scale.is_finite()
            && self.scale >= 1.0 / r.max_scale
            && self.scale <= r.max_scale
            && self.gap_octaves() <= r.max_gap_octaves
            && self.n_in >= self.required_inliers(r)
            && self.ov_a.max(self.ov_b) >= r.min_overlap
            && self.blk_n >= r.min_blocks
            && self.blk >= r.min_block_agreement
            && self.ncc.abs() >= r.min_ncc
    }
}

// ------------------------------------------------------------ correspondence

#[inline]
fn dist2(a: &[u8], b: &[u8]) -> u32 {
    let mut s = 0u32;
    for i in 0..DESC_LEN {
        let d = a[i] as i32 - b[i] as i32;
        s += (d * d) as u32;
    }
    s
}

/// Lowe's ratio test over a restricted candidate set.
///
/// `cands` holds, for each keypoint of A, the keypoints of B worth comparing
/// against (from the shared-word index). The second-nearest neighbour is taken
/// from the same set, which is what makes this cheap: the ratio test needs a
/// competitor, not the true second nearest over all of B.
pub fn correspond(
    a: &Features,
    b: &Features,
    cands: &[(u32, u32)],
    ratio: f32,
    out: &mut Vec<(u32, u32)>,
) {
    out.clear();
    if cands.is_empty() {
        return;
    }
    let thr = ratio * ratio;
    let mut i = 0;
    while i < cands.len() {
        let qi = cands[i].0;
        let mut best = (u32::MAX, 0u32);
        let mut second = u32::MAX;
        let da = a.d(qi as usize);
        let start = i;
        while i < cands.len() && cands[i].0 == qi {
            let tj = cands[i].1;
            let d = dist2(da, b.d(tj as usize));
            if d < best.0 {
                second = best.0;
                best = (d, tj);
            } else if d < second {
                second = d;
            }
            i += 1;
        }
        let n = i - start;
        if best.0 == u32::MAX {
            continue;
        }
        // With a single candidate there is no competitor, so the ratio test
        // cannot run. Such a match is kept: the geometric stage is the real
        // filter, and discarding them costs recall on sparse images.
        if n == 1 || (best.0 as f32) < thr * second as f32 {
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
fn best_transform(a: &Features, b: &Features, pairs: &[(u32, u32)], bw: f32, bh: f32) -> Option<(Affine, Vec<bool>)> {
    if pairs.len() < 3 {
        return None;
    }
    let tol = (0.03 * (bw * bw + bh * bh).sqrt()).max(3.0);
    let tol2 = tol * tol;
    let mut mask = vec![false; pairs.len()];
    let mut best: Option<(usize, Affine)> = None;
    // Every correspondence is a hypothesis. On this scale that is cheaper and
    // more reliable than random sampling: no iteration count to tune, and a
    // single good match is enough to find the answer.
    let cap = pairs.len().min(600);
    let mut scratch = vec![false; pairs.len()];
    for &(i, j) in pairs.iter().take(cap) {
        let m = from_single(&a.kps[i as usize], &b.kps[j as usize]);
        let sc = (m[0] * m[4] - m[1] * m[3]).abs().sqrt();
        if !(sc.is_finite() && sc > 1e-3 && sc < 1e3) {
            continue;
        }
        let mut count = 0usize;
        for (k, &(p, q)) in pairs.iter().enumerate() {
            let ka = &a.kps[p as usize];
            let kb = &b.kps[q as usize];
            let (px, py) = apply(&m, ka.x, ka.y);
            let (dx, dy) = (px - kb.x, py - kb.y);
            let ok = dx * dx + dy * dy < tol2;
            scratch[k] = ok;
            count += ok as usize;
        }
        if count >= 3 && best.map_or(true, |(c, _)| count > c) {
            best = Some((count, m));
            mask.copy_from_slice(&scratch);
        }
    }
    let (_, mut m) = best?;
    // Refine: least-squares affine on the inliers, recount, repeat while the
    // set does not shrink.
    for _ in 0..3 {
        let Some(m2) = fit_affine(a, b, pairs, &mask) else { break };
        let mut next = vec![false; pairs.len()];
        let mut count = 0usize;
        for (k, &(p, q)) in pairs.iter().enumerate() {
            let ka = &a.kps[p as usize];
            let kb = &b.kps[q as usize];
            let (px, py) = apply(&m2, ka.x, ka.y);
            let (dx, dy) = (px - kb.x, py - kb.y);
            let ok = dx * dx + dy * dy < tol2;
            next[k] = ok;
            count += ok as usize;
        }
        if count < mask.iter().filter(|v| **v).count() {
            break;
        }
        m = m2;
        mask = next;
    }
    Some((m, mask))
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
#[derive(Clone, Debug, Default)]
pub struct Thumb {
    pub w: u16,
    pub h: u16,
    /// Scale from working-image coordinates to thumbnail coordinates.
    pub scale: f32,
    pub px: Vec<u8>,
}

impl Thumb {
    pub fn build(g: &Gray, long: usize) -> Thumb {
        let t = crate::decode::fit_to(g.clone(), long);
        let scale = t.w as f32 / g.w as f32;
        Thumb {
            w: t.w as u16,
            h: t.h as u16,
            scale,
            px: t.px.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect(),
        }
    }
    #[inline]
    fn sample(&self, x: f32, y: f32) -> f32 {
        // Bilinear, clamped. Callers check bounds first.
        let x = x.clamp(0.0, self.w as f32 - 1.001);
        let y = y.clamp(0.0, self.h as f32 - 1.001);
        let (x0, y0) = (x as usize, y as usize);
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let w = self.w as usize;
        let i = y0 * w + x0;
        let a = self.px[i] as f32 * (1.0 - fx) + self.px[i + 1] as f32 * fx;
        let b = self.px[i + w] as f32 * (1.0 - fx) + self.px[i + w + 1] as f32 * fx;
        a * (1.0 - fy) + b * fy
    }
}

const GRID: usize = 48;
const BLOCK: usize = 8;

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
    for iy in 0..GRID {
        let y = y0 + (y1 - y0) * iy as f32 / (GRID - 1) as f32;
        for ix in 0..GRID {
            let x = x0 + (x1 - x0) * ix as f32 / (GRID - 1) as f32;
            let (u, v) = apply(m, x, y);
            if u < 0.0 || u >= bw || v < 0.0 || v >= bh {
                continue;
            }
            let k = iy * GRID + ix;
            let s = ta.sample(x * ta.scale, y * ta.scale);
            // An inverted match is compared against the inverse of A rather
            // than by keeping a second copy of every thumbnail.
            va[k] = if invert { 255.0 - s } else { s };
            vb[k] = tb.sample(u * tb.scale, v * tb.scale);
            ok[k] = true;
        }
    }
    let mut agree = 0u32;
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
            if n < BLOCK * BLOCK * 4 / 5 {
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
            if (cov / (vara * varb).sqrt()).abs() > 0.5 {
                agree += 1;
            }
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
    (if total > 0 { agree as f32 / total as f32 } else { 0.0 }, total, ncc)
}

// ------------------------------------------------------------ entry points

pub struct Pair<'a> {
    pub fa: &'a Features,
    pub fb: &'a Features,
    pub ta: &'a Thumb,
    pub tb: &'a Thumb,
}

/// Full verification from a candidate correspondence list.
pub fn verify(p: &Pair, cands: &[(u32, u32)], var: Variant, ratio: f32, scratch: &mut Vec<(u32, u32)>) -> Verdict {
    let mut v = Verdict { variant: var, ..Default::default() };
    correspond(p.fa, p.fb, cands, ratio, scratch);
    v.n_match = scratch.len() as u32;
    let (bw, bh) = (p.fb.w as f32, p.fb.h as f32);
    let (aw, ah) = (p.fa.w as f32, p.fa.h as f32);
    let Some((m, mask)) = best_transform(p.fa, p.fb, scratch, bw, bh) else { return v };
    // `p.fa` is the query image already mirrored, so `m` maps mirrored-A
    // coordinates into B. Composing the mirror back in gives a transform from
    // A's own coordinates, which is what the rest of the tool stores, checks
    // and composes.
    let m = if var.mirror { compose(&mirror_affine(aw), &m) } else { m };
    v.m = m;
    v.n_in = distinct_inliers(p.fa, scratch, &mask);
    v.scale = (m[0] * m[4] - m[1] * m[3]).abs().sqrt();
    v.rot_deg = m[3].atan2(m[0]).to_degrees();
    let (oa, ob) = overlap(&m, aw, ah, bw, bh);
    v.ov_a = oa;
    v.ov_b = ob;
    if v.n_in >= 3 && v.ov_a.max(v.ov_b) > 0.2 {
        let (blk, n, ncc) = pixel_check(p.ta, p.tb, &m, aw, ah, bw, bh, var.invert);
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
pub fn verify_transform(p: &Pair, m: &Affine, var: Variant) -> Verdict {
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
    if v.ov_a.max(v.ov_b) > 0.2 {
        let (blk, n, ncc) = pixel_check(p.ta, p.tb, m, aw, ah, bw, bh, var.invert);
        v.blk = blk;
        v.blk_n = n;
        v.ncc = ncc;
    }
    v
}

