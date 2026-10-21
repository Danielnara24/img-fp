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

/// How the query image was turned before its descriptors were matched.
pub use crate::sift::Variant;

/// The coordinate map of a horizontal mirror on an image `w` wide.
#[inline]
pub fn mirror_affine(w: f32) -> Affine {
    [-1.0, 0.0, w - 1.0, 0.0, 1.0, 0.0]
}

#[derive(Clone, Copy, Debug)]
pub struct Verdict {
    pub m: Affine,
    /// Which permutation of the query produced this match. Its inversion is
    /// the edge's; see `lib::Edge`.
    pub variant: Variant,
    /// Inliers at distinct keypoint positions.
    pub n_in: u32,
    /// Whether the inliers enclose the centre of the overlap the transform
    /// claims. See `encloses_centre`.
    pub centred: bool,
    /// The transform takes A's frame onto B's whole, each corner onto its own:
    /// one picture is all of the other, resized. See `frame_onto_frame`.
    pub framed: bool,
    pub n_match: u32,
    /// Fraction of A's frame that lands inside B, and the reverse.
    pub ov_a: f32,
    pub ov_b: f32,
    pub scale: f32,
    pub rot_deg: f32,
    /// Mean correlation over the blocks of the overlap that carry detail.
    pub blk: f32,
    pub blk_n: u32,
    /// The lowest of those blocks' correlations: whether any part of the
    /// overlap disagrees, where `blk` says how much it agrees on the whole.
    /// See `Standing::Clean`.
    pub blk_min: f32,
    pub ncc: f32,
}

impl Default for Verdict {
    fn default() -> Self {
        Verdict {
            m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            variant: Variant::default(),
            n_in: 0,
            centred: false,
            framed: false,
            n_match: 0,
            ov_a: 0.0,
            ov_b: 0.0,
            scale: 0.0,
            rot_deg: 0.0,
            blk: 0.0,
            blk_n: 0,
            blk_min: 0.0,
            ncc: 0.0,
        }
    }
}

/// What a direct verdict may do, by what a wrong answer would cost. Ordered:
/// each standing clears every bar of the ones below it.
///
/// What differs between them is not the strength of the evidence, it is the
/// cost of a mistake. A claim that can put two files into one cluster costs,
/// when wrong, every pair the two clusters imply: on this corpus one bad
/// anchor between two beach photographs made 354 false pairs downstream, and
/// another 3,002. A claim inside a cluster that already exists costs one.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Standing {
    Rejected,
    /// A direct match between two files an anchor has *already* placed in one
    /// cluster. It cannot merge anything, so the question is no longer "are
    /// these the same picture" but "does this pair hold up", and it faces the
    /// anchor's rule with the correlation bar lowered
    /// (`CLUSTER_SLACK_CORRELATION`) and without the demand that its evidence
    /// enclose what it claims: that demand exists only to stop evidence from
    /// one corner of a frame putting two files together on the strength of
    /// the rest.
    Corroborating,
    /// Believed on its own: enough aligned points, enough of the frame, and
    /// agreeing pixels, with the points enclosing the centre of the overlap
    /// they claim (`Verdict::centred`; without it, 36 merges). It may add a
    /// file to a cluster, and joins two only as `claims::admit` allows.
    Anchor,
    /// An anchor whose every detailed block agrees (`CLEAN_WORST_BLOCK`): the
    /// only claim that joins two clusters on its own.
    Clean,
}

/// The lowest correlation every detailed block of a clean anchor reaches
/// (`Standing::Clean`), and of a whole-frame match short of aligned points
/// (`Policy::standing`).
///
/// **This number is fitted**, and it is the one number the near-miss rule
/// has. It is the plateau's lower middle, as it was when it was 0.85: swept on
/// IMGS-ALL, two photographs of one scene in Segovia merge at 0.85 (6,933
/// false pairs) and no family merges from 0.875 to 0.95 (F1 0.9823, 0.9821,
/// 0.9816, 0.9806). It moved up a step when the fit and the pyramid stopped
/// costing exact copies agreement (`best_transform`, `Pyramid`): near misses
/// rose by about 0.04 with them, and the bar was 0.04 above the old cliff. A
/// count of aligned points was tried beside it and is worse: a dark or plain
/// picture has few features and good pixels, and its family fell apart. See
/// CLAUDE.md.
pub const CLEAN_WORST_BLOCK: f32 = 0.9;

/// The largest scale between the working images a claim may span. A sanity
/// bound, not a tuned one. Both working images are near `--work-size` on
/// their long side whatever the files' sizes, so it does not limit how much
/// smaller one file may be than another: it limits how small a part of one
/// picture the other may be, and past sixteen-fold that part is some thirty
/// pixels of a 512-pixel analysis.
const MAX_SCALE: f32 = 16.0;

/// How much easier it is to be believed *inside* a cluster than to create one
/// (`Standing::Corroborating`): the correlation bar, this much lower.
///
/// The one number in the acceptance rule read off a corpus rather than
/// derived, and it earned it: the held-out corpus preferred the margin too
/// (without it validation recall fell from 93.1% to 90.7%), and on IMGS-ALL it
/// is half a point of recall and moves no merge cliff. What it admits is a
/// pair whose pixels agree less than its fit accounts for: a whole-picture
/// overlay (`tiled_watermark`, 39 of the 60 pairs it adds) and non-affine
/// warps (barrel, wave, keystone, perspective). A margin of two aligned
/// points stood beside it, carried 0.03 points of recall and no cliff, and
/// is gone: a corroborated match has the anchor's points or a whole frame.
const CLUSTER_SLACK_CORRELATION: f32 = 0.1;

/// The three bars a user sets, and what a verdict is worth against them: a
/// direct one's `Standing`, or whether a composed one stands (`composed`).
///
/// The old policy had an anchor tier, a propagated tier and a corroborated
/// tier, and they disagreed about six things: an inlier bonus of 2 for
/// anchors, block minimums of 8, 6 and 4, an agreement bar ten points lower
/// for corroboration, a scale surcharge of 6 per octave for two tiers and 4
/// for the third, and an overlap floor of 0.9 for propagation alone. Every
/// one of those numbers was read off one corpus. The last two went once the
/// pixel check stopped aliasing: both existed to distrust a comparison across
/// a large scale gap, and that distrust was earned by a sampling bug rather
/// than by the geometry.
///
/// Corroboration survived an attempt to fold it in with propagation. Both
/// are claims inside a cluster, and that is wrong, because a corroborated
/// pair *does* have features vouching for it and a propagated one does not.
/// Holding it to the pixels-only bar cost 1.8 points of recall on the tuning
/// corpus and 4.2 on the held-out one.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    min_aligned_points: u32,
    min_frame_overlap: f32,
    min_pixel_correlation: f32,
}

impl Policy {
    pub fn new(min_aligned_points: u32, min_frame_overlap: f32, min_pixel_correlation: f32) -> Policy {
        Policy { min_aligned_points, min_frame_overlap, min_pixel_correlation }
    }

    /// `--min-frame-overlap`, which every claim faces whatever vouches for it.
    pub fn min_frame_overlap(&self) -> f32 {
        self.min_frame_overlap
    }

    /// What a verdict must already have before its pixels are worth reading:
    /// the aligned points and the overlap every standing asks (a whole frame
    /// is read below the points; see `standing`).
    pub fn gate(&self) -> (u32, f32) {
        (self.min_aligned_points, self.min_frame_overlap)
    }

    /// The geometry every claim must have: a transform within `MAX_SCALE`
    /// that puts enough of one frame on the other.
    ///
    /// Two of the bars are the CLI's `--min-frame-overlap` and
    /// `--min-pixel-correlation`, and the nouns are the whole point: `ov_a`
    /// and `ov_b` are **frames**, pure geometry with no pixel read, so that
    /// floor asks what the transform *claims*; `blk` is the mean of |r| over
    /// the **pixels** of that overlap carrying detail, so that floor asks
    /// whether the claim is *true*. They are the only defence against two
    /// failure modes that do not overlap at all, and each is blind to the
    /// other's:
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
    fn frames(&self, v: &Verdict) -> bool {
        v.scale.is_finite()
            && v.scale >= 1.0 / MAX_SCALE
            && v.scale <= MAX_SCALE
            && v.ov_a.max(v.ov_b) >= self.min_frame_overlap
    }

    /// What a direct verdict may do; see `Standing`.
    ///
    /// **A frame landing on a frame vouches for the geometry instead of the
    /// points** (`Verdict::framed`): a verdict that takes one frame onto the
    /// other whole, resized, may have as few aligned points as a transform is
    /// fitted through when every detailed block of it agrees as a clean
    /// anchor's does. A count of aligned points is a defence against a
    /// transform that a few coincidental correspondences happen to agree on;
    /// four corners landing on four corners, within the tolerance an inlier
    /// is held to, is not something such a transform does. What it cannot
    /// vouch for is the content, and that is the pixel check's at the bar a
    /// cluster is built on. A picture with little detail — an icon, a logo, a
    /// plain diagram — has a handful of features and was matched to its own
    /// resized copy by none of them: a 512-pixel icon and its 128-pixel copy
    /// had 7 aligned points and correlated at 0.997 on every block. Never
    /// clean on that evidence: it may add a file to a cluster and never join
    /// two.
    pub fn standing(&self, v: &Verdict) -> Standing {
        if !self.frames(v) {
            return Standing::Rejected;
        }
        let points = v.n_in >= self.min_aligned_points;
        let whole = v.framed && v.n_in >= 3 && v.blk_min >= CLEAN_WORST_BLOCK;
        let agrees = v.blk >= self.min_pixel_correlation;
        if points && v.centred && agrees && v.blk_min >= CLEAN_WORST_BLOCK {
            Standing::Clean
        } else if !(points || whole) {
            Standing::Rejected
        } else if v.centred && agrees {
            Standing::Anchor
        } else if v.blk >= (self.min_pixel_correlation - CLUSTER_SLACK_CORRELATION).max(0.0) {
            Standing::Corroborating
        } else {
            Standing::Rejected
        }
    }

    /// Whether a transform composed through other matches stands
    /// (`claims::propagate`). No features vouch for it, so the inlier count is
    /// not evidence about it, and the pixels have to answer on their own.
    pub fn composed(&self, v: &Verdict) -> bool {
        self.frames(v) && v.blk >= self.min_pixel_correlation
    }
}

const GRID: usize = 48;
const BLOCK: usize = 8;

impl Verdict {
    /// What two files with one working image are stated with when no pass
    /// verified them (`same::Kind::SameGrey`): the identity, every one of
    /// the `n` keypoints aligned and every block agreeing.
    pub fn perfect(n: u32) -> Verdict {
        Verdict {
            n_in: n,
            n_match: n,
            centred: true,
            framed: true,
            ov_a: 1.0,
            ov_b: 1.0,
            scale: 1.0,
            blk: 1.0,
            blk_n: 1,
            blk_min: 1.0,
            ncc: 1.0,
            ..Default::default()
        }
    }

    /// The same verdict read from B to A: the transform inverted and every
    /// figure that has a direction turned round with it. `None` when the
    /// transform has no inverse.
    ///
    /// The counts, the correlations and `centred` — which passes on either
    /// frame — say the same thing both ways.
    pub fn reversed(&self) -> Option<Verdict> {
        let m = invert_affine(&self.m)?;
        Some(Verdict {
            m,
            ov_a: self.ov_b,
            ov_b: self.ov_a,
            scale: linear_scale(&m),
            rot_deg: m[3].atan2(m[0]).to_degrees(),
            ..self.clone()
        })
    }
}

// ------------------------------------------------------------ correspondence

// The squared distance between two descriptors is the vocabulary descent's,
// and for the descent's reason: both sides are bytes, sixteen-bit lanes and a
// pairwise multiply-add fold 128 of them in about twenty cycles, and the sum
// is over integers so it is exact whatever order it is taken in. The matcher
// had its own portable copy — sixteen `u32` lanes, which the compiler widens a
// dimension at a time — on the grounds that the loop around it waits for
// memory rather than for arithmetic. It does; it also runs four million times
// in a second look, and the arithmetic it was waiting with was four times the
// instructions for the same number.
use crate::index::dist2;

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
#[cfg_attr(dispatch, inline(always))]
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
        let da: &[u8; DESC_LEN] = a.d(qi as usize).try_into().unwrap();
        while i < cands.len() && cands[i].0 == qi {
            let tj = cands[i].1;
            // The next few candidates' descriptors, asked for now.
            //
            // This loop is not bound by `dist2` — a descriptor is 128 bytes and
            // sixteen integer lanes measure it in about twenty cycles — it is
            // bound by *finding* the descriptor. `b`'s block is 76 KB, the
            // candidates land in it at random, and on a corpus of nine thousand
            // images nothing in it is in cache: two lines per candidate, fetched
            // one dependent gather at a time. Measured against a pool of images
            // too large to cache, the same call costs two to three times what it
            // costs against a warm one. So the addresses are handed to the
            // prefetcher while the current distance is still being summed; it
            // changes nothing about what is computed.
            prefetch_descriptor(b, cands, i + PREFETCH);
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

/// How far ahead the candidate walk asks for its descriptors. Two cache lines
/// each and a DRAM round trip of a few hundred cycles against the twenty a
/// distance takes, so the distance is worth about eight candidates of lead.
const PREFETCH: usize = 8;

/// Ask for the descriptor and the keypoint of `cands[k]`'s B side. Out of range
/// is not an error — the walk is near its end and there is nothing to fetch.
#[cfg_attr(dispatch, inline(always))]
#[cfg_attr(not(dispatch), inline)]
fn prefetch_descriptor(b: &Features, cands: &[(u32, u32)], k: usize) {
    let Some(&(_, tj)) = cands.get(k) else { return };
    let tj = tj as usize;
    if tj >= b.kps.len() {
        return;
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use std::arch::x86_64::_mm_prefetch;
        let d = b.desc.as_ptr().add(tj * DESC_LEN);
        _mm_prefetch(d as *const i8, std::arch::x86_64::_MM_HINT_T0);
        _mm_prefetch(d.add(64) as *const i8, std::arch::x86_64::_MM_HINT_T0);
        // `best_transform` reads this keypoint next, out of the same cold
        // block, and one prefetch here is cheaper than a miss there.
        _mm_prefetch(b.kps.as_ptr().add(tj) as *const i8, std::arch::x86_64::_MM_HINT_T0);
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = (b, tj);
}

// ------------------------------------------------------------ geometry

#[cfg_attr(dispatch, inline(always))]
#[cfg_attr(not(dispatch), inline)]
fn apply(m: &Affine, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}

/// The similarity transform implied by one keypoint correspondence.
#[cfg_attr(dispatch, inline(always))]
fn from_single(ka: &Keypoint, kb: &Keypoint) -> Affine {
    let s = kb.sigma / ka.sigma.max(1e-6);
    let th = (kb.angle - ka.angle).to_radians();
    let (sn, cs) = th.sin_cos();
    let (c, sn) = (cs * s, sn * s);
    [c, -sn, kb.x - (c * ka.x - sn * ka.y), sn, c, kb.y - (sn * ka.x + c * ka.y)]
}

/// Least-squares affine through the given correspondences.
#[cfg_attr(dispatch, inline(always))]
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

#[cfg_attr(dispatch, inline(always))]
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

/// The determinant of a transform's linear part: the factor it scales area
/// by, negative when it turns the picture over.
#[inline(always)]
pub fn det(m: &Affine) -> f32 {
    m[0] * m[4] - m[1] * m[3]
}

/// The factor a transform scales length by, on average over directions.
#[inline(always)]
pub fn linear_scale(m: &Affine) -> f32 {
    det(m).abs().sqrt()
}

/// Whether a transform turns its picture over: a left-right or top-bottom
/// flip, alone or with any rotation.
pub fn mirrored(m: &Affine) -> bool {
    det(m) < 0.0
}

/// The transform back, or `None` when it has none.
pub fn invert_affine(m: &Affine) -> Option<Affine> {
    let det = det(m);
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

/// Best transform explaining the correspondences, and how many it explains.
///
/// Which ones is left in `scratch.mask`, where the caller reads it. It used to
/// come back as a `Vec<bool>` of its own, which is an allocation and a copy per
/// verdict — four million of them on a corpus where most files have no
/// duplicate — for a buffer the caller drops a few lines later.
#[cfg_attr(dispatch, inline(always))]
fn best_transform(a: &Features, b: &Features, pairs: &[(u32, u32)], bw: f32, bh: f32, scratch: &mut Scratch) -> Option<(Affine, usize)> {
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
    for (k, &(p, q)) in pairs.iter().enumerate() {
        // Both keypoint arrays are cold and read at random — see
        // `prefetch_descriptor`, which has already asked for `b`'s side of the
        // candidates this list came from.
        prefetch_keypoints(a, b, pairs, k + PREFETCH);
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
    // The coordinates a hypothesis is counted against, in the order it is
    // counted: the best hypothesis so far's outliers first, then its inliers.
    //
    // A count does not depend on the order, and a hypothesis is only ever
    // asked whether it beats the best — so the order decides nothing but how
    // soon a loser is known for one. And most hypotheses on a real duplicate
    // are themselves inliers of the best: they describe nearly the same
    // transform, explain nearly the same points, and used to be counted over
    // every correspondence to find out they did not explain one more. Counted
    // over the best's outliers first, such a hypothesis that explains none of
    // them can reach no more than the best's own count, which `count_inliers`
    // sees at the end of the outliers and stops there. Rebuilt each time the
    // best changes, which is a handful of times a pair.
    let mut by_best = false;
    // Every correspondence is a hypothesis. On this scale that is cheaper and
    // more reliable than random sampling: no iteration count to tune, and a
    // single good match is enough to find the answer.
    let cap = n.min(600);
    for &(i, j) in pairs.iter().take(cap) {
        let m = from_single(&a.kps[i as usize], &b.kps[j as usize]);
        let s = linear_scale(&m);
        if !(s.is_finite() && s > 1e-3 && s < 1e3) {
            continue;
        }
        // What this hypothesis would have to reach to be kept at all. Almost
        // none of them get near it — a wrong transform explains two or three
        // correspondences out of hundreds — and a hypothesis that cannot win
        // is not worth finishing. See `count_inliers`.
        let need = best.map_or(3, |(c, _)| c + 1);
        let count = if by_best {
            let [ox, oy, qx, qy] = &sc.order;
            count_inliers(&m, &ox[..n], &oy[..n], &qx[..n], &qy[..n], tol2, need)
        } else {
            count_inliers(&m, ax, ay, bx, by, tol2, need)
        };
        if count >= need {
            // A hypothesis that beats the best so far is rare, and it is the
            // only kind whose mask anyone reads.
            let marked = mark_inliers(&m, ax, ay, bx, by, tol2, &mut sc.hit);
            debug_assert_eq!(marked, count);
            best = Some((marked, m));
            sc.mask.copy_from_slice(&sc.hit);
            for (o, src) in sc.order.iter_mut().zip([ax, ay, bx, by]) {
                o.clear();
                o.extend((0..n).filter(|&t| !sc.hit[t]).map(|t| src[t]));
                o.extend((0..n).filter(|&t| sc.hit[t]).map(|t| src[t]));
            }
            by_best = true;
        }
    }
    let (n_in, mut m) = best?;
    let mut held_after = n_in;
    // Refine: least-squares affine on the inliers, recount, repeat while the
    // set does not shrink.
    //
    // **The fitted transform is kept even when the recount shrinks**; only
    // the inlier set and its count stay the hypothesis's. The hypothesis is a
    // similarity read off one keypoint pair's scale and angle, which a
    // keypoint knows to a percent or so: on exact copies it was 1-9 working
    // pixels out at the corners (median 4.9 on a plain half-size copy, 8.7 on
    // a crop), and the pixel check read that misregistration as disagreement
    // — 110 of 288 lossless copies fell under the clean bar. The fit through
    // hundreds of inliers is a few tenths of a pixel out. It was thrown away
    // whenever it moved one inlier past the tolerance, which at 1.5% of the
    // diagonal is nearly always: the count is a vote for the hypothesis, not a
    // measure of the fit.
    for _ in 0..3 {
        let held = sc.mask.iter().filter(|v| **v).count();
        let Some(m2) = fit_affine(a, b, pairs, &sc.mask) else { break };
        let count = count_inliers(&m2, ax, ay, bx, by, tol2, held);
        if count < held {
            m = m2;
            break;
        }
        mark_inliers(&m2, ax, ay, bx, by, tol2, &mut sc.hit);
        m = m2;
        sc.mask.copy_from_slice(&sc.hit);
        held_after = count;
    }
    Some((m, held_after))
}

/// Ask for the keypoints `pairs[k]` will read, on both sides.
#[cfg_attr(dispatch, inline(always))]
#[cfg_attr(not(dispatch), inline)]
fn prefetch_keypoints(a: &Features, b: &Features, pairs: &[(u32, u32)], k: usize) {
    let Some(&(p, q)) = pairs.get(k) else { return };
    #[cfg(target_arch = "x86_64")]
    unsafe {
        use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        if (p as usize) < a.kps.len() {
            _mm_prefetch(a.kps.as_ptr().add(p as usize) as *const i8, _MM_HINT_T0);
        }
        if (q as usize) < b.kps.len() {
            _mm_prefetch(b.kps.as_ptr().add(q as usize) as *const i8, _MM_HINT_T0);
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = (a, b, p, q);
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
#[cfg_attr(dispatch, inline(always))]
#[cfg_attr(not(dispatch), inline)]
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
#[cfg_attr(dispatch, inline(always))]
#[cfg_attr(not(dispatch), inline)]
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
#[cfg_attr(dispatch, inline(always))]
fn distinct_inliers(a: &Features, pairs: &[(u32, u32)], mask: &[bool], pts: &mut Vec<(i32, i32)>) -> u32 {
    pts.clear();
    pts.extend(
        pairs
            .iter()
            .zip(mask)
            .filter(|&(_, &m)| m)
            .map(|(&(i, _), _)| {
                let k = &a.kps[i as usize];
                ((k.x * 2.0) as i32, (k.y * 2.0) as i32)
            }),
    );
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
#[cfg_attr(dispatch, inline(always))]
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
#[cfg_attr(dispatch, inline(always))]
/// Whether `m` takes A's frame onto B's whole, resized and nothing else: each
/// of A's four corners lands on the same corner of B, within the inlier
/// tolerance (`best_transform`'s).
///
/// **Not turned or flipped.** A turn of a frame lands on a frame as well, and
/// so does what a rearrangement of a picture's parts makes: on IMGS-ALL a
/// `quadrant_rotate` and a `tile_shuffle` of one dark photograph, turned half
/// a turn onto each other, met it on three aligned points and nine detailed
/// blocks, and the rearranged copy was suggested for deletion. A resize is the
/// one way round the frame vouches for; a turned or mirrored copy is held to
/// the aligned points as before.
fn frame_onto_frame(m: &Affine, aw: f32, ah: f32, bw: f32, bh: f32) -> bool {
    let tol = (0.015 * (bw * bw + bh * bh).sqrt()).max(3.0);
    [((0.0, 0.0), (0.0, 0.0)), ((aw, 0.0), (bw, 0.0)), ((0.0, ah), (0.0, bh)), ((aw, ah), (bw, bh))].iter().all(|&((x, y), (cx, cy))| {
        let (u, v) = apply(m, x, y);
        // Squared, not `hypot`: glibc's `hypotf` would raise the floor.
        (u - cx) * (u - cx) + (v - cy) * (v - cy) <= tol * tol
    })
}

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
/// introduced. The pyramid is derived from `px` and is not serialised.
///
/// Nor is it kept. It is built for each comparison that reads it, into
/// scratch the worker keeps (`Pyramid`), and that costs a few microseconds,
/// paid once for a run of comparisons on one side: a level is the one below
/// filtered by [1 3 3 1]/8 each way and halved (`Pyramid`). Kept on the thumbnail it was a third again of every
/// thumbnail, held from the first comparison to the end of the run — 103 MB
/// on a corpus of 27,000 pictures, nearly all of whose thumbnails are read
/// above level zero, and alive at the end of verification, which is where
/// that run peaks. The levels are a pure function of `px`, so when they are
/// made cannot change what they hold.
#[derive(Clone, Debug, Default)]
pub struct Thumb {
    pub w: u16,
    pub h: u16,
    /// Scale from working-image coordinates to thumbnail coordinates.
    pub scale: f32,
    /// In the heap, or in the run's scratch file (`spill`).
    pub px: crate::spill::Shared<u8>,
    /// Which thumbnail this is, for a worker's `Pyramid` to know it already
    /// holds its levels: unique to each `Thumb::new`, and shared only by a
    /// clone, which holds the same pixels. 0, a default thumbnail's, is none.
    pub id: u64,
}

/// The half-resolution levels above a thumbnail's `px`, built into buffers a
/// worker keeps from one comparison to the next: level i has been halved
/// i+1 times.
///
/// **A level is the one below filtered by [1 3 3 1]/8 each way, then
/// halved**, not its 2x2 mean. Pixel `j` of a level is centred where the 2x2
/// block's centre was (`to_level` is unchanged), but a 2x2 mean is a box with
/// no margin, and what it keeps of detail near its own scale depends on where
/// its grid falls: two thumbnails of one picture whose grids fall differently
/// — a crop, which moves the grid by any fraction, or a reflection of a
/// thumbnail with an odd side, which moves it by half a level pixel — held
/// different levels, and the pixel check read the difference as disagreement.
/// Exact lossless copies, compared through the exact transform, fell under
/// the clean bar in 12 of 288 cases (a 180-degree turn of a 128x75 thumbnail
/// read 0.75 at its worst block), all of them a reflection or a crop; a
/// resize, whose grids fall alike, read 0.999. The wider filter suppresses
/// what the grid's position decides, and the 12 became 2. It is what
/// `suggest::blurred` found for the same reason, at the cost of a Gaussian.
#[derive(Default)]
pub(crate) struct Pyramid {
    buf: Vec<u8>,
    /// Width, height and where in `buf` each level starts.
    levels: Vec<(u16, u16, usize)>,
    /// The vertical pass of the row being made.
    rows: Vec<u16>,
    /// The thumbnail whose levels these are (`Thumb::id`), 0 for none.
    of: u64,
}

impl Pyramid {
    /// At least the first `want` levels of `t`, or all it has if fewer.
    ///
    /// Levels already made for `t` are kept: a worker verifies a file against
    /// its candidates one after another, so the same thumbnail is the first
    /// side of a long run of comparisons, and its levels were made again for
    /// each. And no deeper than asked, since a comparison reads two adjacent
    /// levels. Neither changes what a level holds, only how often it is made.
    #[cfg_attr(dispatch, inline(always))]
    fn fill(&mut self, t: &Thumb, want: usize) {
        if t.id == 0 || t.id != self.of {
            self.buf.clear();
            self.levels.clear();
            self.of = t.id;
            // Every level together is under a third of level zero.
            self.buf.reserve(t.w as usize * t.h as usize / 2);
        }
        let (mut cw, mut ch, mut from) = match self.levels.last() {
            Some(&(w, h, at)) => (w as usize, h as usize, Some(at)),
            None => (t.w as usize, t.h as usize, None),
        };
        while self.levels.len() < want && cw >= 4 && ch >= 4 {
            let (nw, nh) = (cw / 2, ch / 2);
            let at = self.buf.len();
            self.buf.resize(at + nw * nh, 0);
            let (done, next) = self.buf.split_at_mut(at);
            let cur: &[u8] = match from {
                None => &t.px,
                Some(f) => &done[f..f + cw * ch],
            };
            // Vertical first, at the source's width, then horizontal: the
            // vertical pass reads whole rows and the horizontal one then runs
            // over half as many.
            self.rows.clear();
            self.rows.resize(cw, 0);
            let row = |k: usize| &cur[k.min(ch - 1) * cw..(k.min(ch - 1) + 1) * cw];
            for y in 0..nh {
                filter_column(row((2 * y).saturating_sub(1)), row(2 * y), row(2 * y + 1), row(2 * y + 2), &mut self.rows);
                filter_row(&self.rows, &mut next[y * nw..(y + 1) * nw]);
            }
            self.levels.push((nw as u16, nh as u16, at));
            from = Some(at);
            cw = nw;
            ch = nh;
        }
    }

    #[cfg_attr(dispatch, inline(always))]
    fn len(&self) -> usize {
        self.levels.len()
    }

    #[cfg_attr(dispatch, inline(always))]
    fn level(&self, i: usize) -> (&[u8], usize, usize) {
        let (w, h, at) = self.levels[i];
        let (w, h) = (w as usize, h as usize);
        (&self.buf[at..at + w * h], w, h)
    }
}

/// The vertical half of a pyramid level: `r0 + 3 r1 + 3 r2 + r3` per
/// column, at most 8 x 255. Contiguous, and the compiler vectorises it.
#[cfg_attr(dispatch, inline(always))]
#[cfg_attr(not(dispatch), inline)]
fn filter_column(r0: &[u8], r1: &[u8], r2: &[u8], r3: &[u8], out: &mut [u16]) {
    let n = out.len();
    let (r0, r1, r2, r3) = (&r0[..n], &r1[..n], &r2[..n], &r3[..n]);
    for x in 0..n {
        out[x] = r0[x] as u16 + 3 * (r1[x] as u16 + r2[x] as u16) + r3[x] as u16;
    }
}

/// The horizontal half: each output `(v[2x-1] + 3v[2x] + 3v[2x+1] + v[2x+2]
/// + 32) / 64` of the vertical pass `v`, its ends repeated past its edges,
/// rounded to a byte. Every sum is at most 64 x 255, inside an `i16`.
#[cfg_attr(dispatch, inline(always))]
#[cfg_attr(not(dispatch), inline)]
fn filter_row(v: &[u16], out: &mut [u8]) {
    // Functions rather than closures, and inlined: a closure here was kept
    // out of line, and out of the x86-64-v3 copy of the pixel check.
    #[inline(always)]
    fn tap(v: &[u16], x: usize) -> u8 {
        let at = |i: isize| v[i.clamp(0, v.len() as isize - 1) as usize] as u32;
        let i = 2 * x as isize;
        (((at(i - 1) + 3 * (at(i) + at(i + 1)) + at(i + 2)) + 32) >> 6) as u8
    }
    /// Outputs `x..x + 8`; `v` holds `2x - 1` to `2x + 17`.
    #[cfg(target_arch = "x86_64")]
    #[inline(always)]
    unsafe fn block(v: &[u16], x: usize, out: &mut [u8]) {
        use std::arch::x86_64::*;
        unsafe {
            let w13 = _mm256_set1_epi32(1 | 3 << 16);
            let w31 = _mm256_set1_epi32(3 | 1 << 16);
            let l = _mm256_madd_epi16(_mm256_loadu_si256(v.as_ptr().add(2 * x - 1) as *const __m256i), w13);
            let r = _mm256_madd_epi16(_mm256_loadu_si256(v.as_ptr().add(2 * x + 1) as *const __m256i), w31);
            let s = _mm256_srli_epi32(_mm256_add_epi32(_mm256_add_epi32(l, r), _mm256_set1_epi32(32)), 6);
            // Eight 32-bit lanes to eight bytes: pack within each 128-bit
            // half, then take each half's low four bytes.
            let w = _mm256_packus_epi32(s, s);
            let b = _mm256_packus_epi16(w, w);
            let lo = _mm_cvtsi128_si32(_mm256_castsi256_si128(b)) as u32;
            let hi = _mm_cvtsi128_si32(_mm256_extracti128_si256(b, 1)) as u32;
            std::ptr::write_unaligned(out.as_mut_ptr().add(x) as *mut u64, lo as u64 | (hi as u64) << 32);
        }
    }
    let n = out.len();
    if n == 0 {
        return;
    }
    out[0] = tap(v, 0);
    // Every tap of these is inside the row.
    let last = if 2 * n < v.len() { n } else { n - 1 };
    let mut x = 1usize;
    // Eight outputs at a time: `madd` takes `v[2x-1] + 3v[2x]` from a load at
    // `2x - 1` and `3v[2x+1] + v[2x+2]` from one at `2x + 1`, as 32-bit sums,
    // which are the scalar loop's sums exactly. A load at `2x + 1` reads 16
    // values, to `2x + 17`. Only where the CPU has AVX2.
    #[cfg(target_arch = "x86_64")]
    if crate::simd::v3() && last >= 9 && v.len() >= 19 {
        while x + 8 <= last && 2 * x + 17 <= v.len() {
            unsafe { block(v, x, out) };
            x += 8;
        }
        // The rest in one block that ends as far right as the row allows,
        // overlapping outputs already made, which it makes again alike.
        if x < last {
            let end = (last - 8).min((v.len() - 17) / 2);
            if end >= 1 {
                unsafe { block(v, end, out) };
                x = x.max(end + 8);
            }
        }
    }
    for x in x..n {
        out[x] = tap(v, x);
    }
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
        // Stretched to the full byte range before it is rounded, so that the
        // pixel check's flatness bar is measured against the picture's own
        // contrast rather than against 255. A dim or foggy photograph spans a
        // few dozen codes; rounded as it stood, every block of it fell under
        // that bar, and an exact re-encode of it abstained its way to a score
        // of nothing. Min and max commute with inversion, so an inverted match
        // still compares like with like, and a picture that already spans the
        // whole range is rounded exactly as before.
        let (lo, hi) = t.px.iter().fold((1f32, 0f32), |(lo, hi), &v| {
            let v = v.clamp(0.0, 1.0);
            (lo.min(v), hi.max(v))
        });
        let (lo, inv) = if hi > lo { (lo, 1.0 / (hi - lo)) } else { (0.0, 1.0) };
        Thumb::new(
            t.w as u16,
            t.h as u16,
            scale,
            t.px.iter().map(|v| ((v.clamp(0.0, 1.0) - lo) * inv).clamp(0.0, 1.0) * 255.0 + 0.5).map(|v| v as u8).collect(),
        )
    }

    /// A thumbnail from its level zero, which is all there is of one.
    pub fn new(w: u16, h: u16, scale: f32, px: Vec<u8>) -> Thumb {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Thumb { w, h, scale, px: px.into(), id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
    }

    /// The same thumbnail, its pixels in the run's scratch file.
    pub fn spilled(self) -> Thumb {
        Thumb { px: crate::spill::share(crate::spill::Kind::Thumbs, self.px), ..self }
    }


    /// Where a coordinate lands in a level: the pixel below it and the
    /// fraction past it, with the clamp the tap used to carry.
    ///
    /// Written as comparisons rather than `clamp` so that the result is a
    /// number even when the input is not — `clamp` propagates a NaN, and the
    /// integer conversion below is only sound on a value known to be in range.
    /// For every finite input it is the same clamp and the same split.
    #[cfg_attr(dispatch, inline(always))]
    #[cfg_attr(not(dispatch), inline)]
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
    #[cfg_attr(dispatch, inline(always))]
    #[cfg_attr(not(dispatch), inline)]
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
    #[cfg_attr(dispatch, inline(always))]
    #[cfg_attr(not(dispatch), inline)]
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
    /// The levels above zero are built into `pyr` when they are needed at
    /// all, and read from there.
    #[cfg_attr(dispatch, inline(always))]
    fn lod<'s>(&'s self, footprint: f32, pyr: &'s mut Pyramid) -> Lod<'s> {
        let whole = Lod {
            lo: (&self.px[..], self.w as usize, self.h as usize, 1.0),
            hi: None,
            t: 0.0,
        };
        if !(footprint > 1.0) {
            return whole;
        }
        let l = footprint.log2();
        // The levels read are `li - 1` and `li`, so `li + 1` of them.
        pyr.fill(self, (l as usize).saturating_add(1));
        let mips: &Pyramid = pyr;
        if mips.len() == 0 {
            return whole;
        }
        let li = (l as usize).min(mips.len());
        let lo: (&[u8], usize, usize, f32) = if li == 0 {
            (&self.px[..], self.w as usize, self.h as usize, 1.0)
        } else {
            let (p, w, h) = mips.level(li - 1);
            (p, w, h, 1.0 / (1 << li) as f32)
        };
        if li >= mips.len() {
            return Lod { lo, hi: None, t: 0.0 };
        }
        // Blending into the next level keeps a footprint that drifts across a
        // power of two from stepping the measurement.
        let (p, w, h) = mips.level(li);
        Lod {
            lo,
            hi: Some((p, w, h, 1.0 / (1 << (li + 1)) as f32)),
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
    /// A tap at `(x, y)` in thumbnail coordinates — pixel `j` of level zero
    /// spans `[j, j + 1)` — which in a level of factor `f` is the pixel whose
    /// centre is `x * f - 0.5`. See `to_level`.
    #[cfg_attr(dispatch, inline(always))]
    #[cfg_attr(not(dispatch), inline)]
    fn at(&self, x: f32, y: f32) -> f32 {
        let (p, w, h, f) = self.lo;
        let a = Thumb::tap(p, w, h, to_level(x, f), to_level(y, f));
        match self.hi {
            None => a,
            Some((p, w, h, f)) => {
                let b = Thumb::tap(p, w, h, to_level(x, f), to_level(y, f));
                a + (b - a) * self.t
            }
        }
    }
}

/// A position in a thumbnail's coordinates as an index into a pyramid level.
///
/// Thumbnail coordinates are edge-based: pixel `j` of level zero covers
/// `[j, j + 1)`, because that is the box `resize_area` averaged into it, and a
/// pixel of level `i` covers `2^i` of those. An index is centre-based — `tap`
/// reads pixel `j` whole at `j` — so the index of a coordinate is `x * f - 0.5`.
/// Reading `x * f` instead put every tap up to half a pixel of its level low
/// and to the right. On both sides of an unrotated pair that is the same
/// shift and cancels; under a reflection it is the opposite shift on one
/// side, and the two thumbnails were compared a pixel out of register —
/// `rot180`'s exact rotations scored a median agreement of 0.69 where a
/// transpose scored 0.996.
#[inline(always)]
fn to_level(x: f32, f: f32) -> f32 {
    x * f - 0.5
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
    /// Sample `k` sits at `to_level((start + span * k / (GRID - 1) + 0.5) * scale, f)`.
    ///
    /// The `+ 0.5` is the frame's own convention meeting the thumbnail's:
    /// keypoints, and so the transform, put a working pixel's centre at its
    /// integer index, and the thumbnail's coordinates put its left edge there.
    ///
    /// Spelled in exactly that order, and with the thumbnail's scale and the
    /// level's kept apart, because multiplication of floats is not
    /// associative: folding the two into one factor moves the last bit of some
    /// sample positions, and a sample that lands a bit either side of a pixel
    /// boundary is read from a different pair of pixels. That is a different
    /// answer, not a rounder one.
    #[cfg_attr(dispatch, inline(always))]
    fn of(start: f32, span: f32, scale: f32, f: f32, n: usize) -> AxisTaps {
        let mut t = AxisTaps { i: [0; GRID], f: [0.0; GRID] };
        for k in 0..GRID {
            let p = to_level((start + span * k as f32 / (GRID - 1) as f32 + 0.5) * scale, f);
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
    #[cfg_attr(dispatch, inline(always))]
    fn of(lvl: (&[u8], usize, usize, f32), gx: (f32, f32), gy: (f32, f32), scale: (f32, f32)) -> LevelTaps {
        let (px, w, h, f) = lvl;
        if w < 2 || h < 2 {
            return LevelTaps::Degenerate(px.first().copied().unwrap_or(0) as f32);
        }
        LevelTaps::Grid {
            x: AxisTaps::of(gx.0, gx.1, scale.0, f, w),
            y: AxisTaps::of(gy.0, gy.1, scale.1, f, h),
        }
    }

    #[cfg_attr(dispatch, inline(always))]
    #[cfg_attr(not(dispatch), inline)]
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
    #[cfg_attr(dispatch, inline(always))]
    fn of(l: &Lod, gx: (f32, f32), gy: (f32, f32), scale: (f32, f32)) -> GridTaps {
        GridTaps {
            lo: LevelTaps::of(l.lo, gx, gy, scale),
            hi: l.hi.map(|h| LevelTaps::of(h, gx, gy, scale)),
        }
    }

    #[cfg_attr(dispatch, inline(always))]
    #[cfg_attr(not(dispatch), inline)]
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

/// The comparison grid's samples, eight at a time.
///
/// Reading the two thumbnails was nearly all of what the pixel check cost —
/// some forty of its fifty microseconds, measured by `pixel_timings`, against
/// a few for the block statistics — and three quarters of that was the B side,
/// whose samples land wherever the transform puts them: two clamps, two
/// truncations and a fraction per axis per pyramid level, four byte reads and
/// three interpolations, and a blend between the levels, for each of 2,304
/// samples, one at a time.
///
/// All of it is the same arithmetic in every lane, so it runs eight samples
/// to an instruction here. The reads are gathers, two per level: a bilinear
/// tap wants two adjacent bytes from each of two rows, and a four-byte read
/// at the top-left pixel covers the top pair while one ending at the
/// bottom-right pixel covers the bottom pair. Both stay inside the level,
/// because `split` never places a tap past its second-to-last row or column:
/// the top read ends at `(h-2)w + (w-2) + 3 = (h-1)w + 1`, which is inside a
/// plane of `hw` bytes whenever `w >= 2`, and the bottom one starts at
/// `w - 2 >= 0`.
///
/// **Every value is the float the scalar path computes.** Each lane performs
/// the same operations on the same operands in the same order — the clamps as
/// compares and blends that send NaN where the scalar comparisons send it,
/// the truncation and fraction as the scalar form takes them, each product
/// and sum rounded on its own with no fused multiply-add — so this is a
/// change in how many samples an instruction handles, not in what any sample
/// is. `pixel_timings` prints a checksum over every figure the check returns
/// to hold it to that.
///
/// Compiled on every x86-64 build and used only where the CPU has AVX2.
#[cfg(target_arch = "x86_64")]
mod wide {
    use super::*;
    use std::arch::x86_64::*;

    const L: usize = 8;
    const _: () = assert!(GRID % L == 0);

    /// Whether every level both sides read is one this path can: at least two
    /// pixels each way, and as many bytes as its size says.
    #[cfg_attr(dispatch, inline(always))]
    pub(super) fn fits(ga: &GridTaps, la: &Lod, lb: &Lod) -> bool {
        let level = |l: (&[u8], usize, usize, f32)| l.1 >= 2 && l.2 >= 2 && l.0.len() >= l.1 * l.2;
        let grid = |t: &LevelTaps| matches!(t, LevelTaps::Grid { .. });
        level(la.lo)
            && grid(&ga.lo)
            && match (la.hi, &ga.hi) {
                (None, None) => true,
                (Some(h), Some(t)) => level(h) && grid(t),
                _ => false,
            }
            && level(lb.lo)
            && lb.hi.is_none_or(level)
    }

    /// `Thumb::split`, a lane at a time.
    #[inline(always)]
    unsafe fn split(v: __m256, n: usize) -> (__m256i, __m256) {
        unsafe {
            let zero = _mm256_setzero_ps();
            let hi = _mm256_set1_ps(n as f32 - 1.001);
            // `v > 0 ? (v < hi ? v : hi) : 0`, with a NaN failing both tests
            // exactly as it does in the scalar comparisons.
            let below = _mm256_cmp_ps::<_CMP_LT_OQ>(v, hi);
            let above0 = _mm256_cmp_ps::<_CMP_GT_OQ>(v, zero);
            let c = _mm256_blendv_ps(zero, _mm256_blendv_ps(hi, v, below), above0);
            let i = _mm256_cvttps_epi32(c);
            (i, _mm256_sub_ps(c, _mm256_cvtepi32_ps(i)))
        }
    }

    /// `Thumb::lerp` over eight located taps.
    #[inline(always)]
    unsafe fn lerp(px: &[u8], w: usize, i: __m256i, fx: __m256, fy: __m256) -> __m256 {
        unsafe {
            let base = px.as_ptr() as *const i32;
            let top = _mm256_i32gather_epi32::<1>(base, i);
            let bot = _mm256_i32gather_epi32::<1>(base, _mm256_add_epi32(i, _mm256_set1_epi32(w as i32 - 2)));
            let m = _mm256_set1_epi32(0xff);
            let p00 = _mm256_cvtepi32_ps(_mm256_and_si256(top, m));
            let p01 = _mm256_cvtepi32_ps(_mm256_and_si256(_mm256_srli_epi32::<8>(top), m));
            let p10 = _mm256_cvtepi32_ps(_mm256_and_si256(_mm256_srli_epi32::<16>(bot), m));
            let p11 = _mm256_cvtepi32_ps(_mm256_srli_epi32::<24>(bot));
            let one = _mm256_set1_ps(1.0);
            let gx = _mm256_sub_ps(one, fx);
            let a = _mm256_add_ps(_mm256_mul_ps(p00, gx), _mm256_mul_ps(p01, fx));
            let b = _mm256_add_ps(_mm256_mul_ps(p10, gx), _mm256_mul_ps(p11, fx));
            _mm256_add_ps(_mm256_mul_ps(a, _mm256_sub_ps(one, fy)), _mm256_mul_ps(b, fy))
        }
    }

    /// `Thumb::tap` into one level, at `(to_level(x, f), to_level(y, f))`.
    #[inline(always)]
    unsafe fn tap(l: (&[u8], usize, usize, f32), x: __m256, y: __m256) -> __m256 {
        unsafe {
            let f = _mm256_set1_ps(l.3);
            let half = _mm256_set1_ps(0.5);
            let (x0, fx) = split(_mm256_sub_ps(_mm256_mul_ps(x, f), half), l.1);
            let (y0, fy) = split(_mm256_sub_ps(_mm256_mul_ps(y, f), half), l.2);
            let i = _mm256_add_epi32(_mm256_mullo_epi32(y0, _mm256_set1_epi32(l.1 as i32)), x0);
            lerp(l.0, l.1, i, fx, fy)
        }
    }

    /// `LevelTaps::at` for the eight grid columns from `ix`.
    #[inline(always)]
    unsafe fn grid_tap(t: &LevelTaps, px: &[u8], w: usize, ix: usize, iy: usize) -> __m256 {
        let LevelTaps::Grid { x, y } = t else { unreachable!("`fits` admits only grids") };
        unsafe {
            let xi = _mm256_loadu_si256(x.i.as_ptr().add(ix) as *const __m256i);
            let fx = _mm256_loadu_ps(x.f.as_ptr().add(ix));
            let row = _mm256_set1_epi32((y.i[iy] as usize * w) as i32);
            lerp(px, w, _mm256_add_epi32(row, xi), fx, _mm256_set1_ps(y.f[iy]))
        }
    }

    #[inline(always)]
    unsafe fn blend(a: __m256, b: __m256, t: f32) -> __m256 {
        unsafe { _mm256_add_ps(a, _mm256_mul_ps(_mm256_sub_ps(b, a), _mm256_set1_ps(t))) }
    }

    /// Fill `va`, `vb` and `ok` exactly as the scalar loop in `pixel_check`
    /// does. `rows[iy]` is the grid row's position in A's frame.
    ///
    /// # Safety
    /// `fits(ga, la, lb)` must hold.
    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(dispatch, inline(always))]
    pub(super) unsafe fn sample(
        ga: &GridTaps,
        la: &Lod,
        lb: &Lod,
        b_scale: (f32, f32),
        m: &Affine,
        gux: &[f32; GRID],
        gvx: &[f32; GRID],
        rows: &[f32; GRID],
        (bw, bh): (f32, f32),
        invert: bool,
        va: &mut [f32; GRID * GRID],
        vb: &mut [f32; GRID * GRID],
        ok: &mut [bool; GRID * GRID],
    ) {
        unsafe {
            let zero = _mm256_setzero_ps();
            let (vbw, vbh) = (_mm256_set1_ps(bw), _mm256_set1_ps(bh));
            let (m2, m5) = (_mm256_set1_ps(m[2]), _mm256_set1_ps(m[5]));
            let (bsx, bsy) = (_mm256_set1_ps(b_scale.0), _mm256_set1_ps(b_scale.1));
            let half = _mm256_set1_ps(0.5);
            let c255 = _mm256_set1_ps(255.0);
            for (iy, &y) in rows.iter().enumerate() {
                let (uy, vy) = (_mm256_set1_ps(m[1] * y), _mm256_set1_ps(m[4] * y));
                for ix in (0..GRID).step_by(L) {
                    let u = _mm256_add_ps(_mm256_add_ps(_mm256_loadu_ps(gux.as_ptr().add(ix)), uy), m2);
                    let v = _mm256_add_ps(_mm256_add_ps(_mm256_loadu_ps(gvx.as_ptr().add(ix)), vy), m5);
                    let out = _mm256_or_ps(
                        _mm256_or_ps(_mm256_cmp_ps::<_CMP_LT_OQ>(u, zero), _mm256_cmp_ps::<_CMP_GE_OQ>(u, vbw)),
                        _mm256_or_ps(_mm256_cmp_ps::<_CMP_LT_OQ>(v, zero), _mm256_cmp_ps::<_CMP_GE_OQ>(v, vbh)),
                    );
                    let outside = _mm256_movemask_ps(out) as u32;
                    if outside == 0xff {
                        continue;
                    }
                    let mut s = grid_tap(&ga.lo, la.lo.0, la.lo.1, ix, iy);
                    if let (Some(t), Some(h)) = (&ga.hi, la.hi) {
                        s = blend(s, grid_tap(t, h.0, h.1, ix, iy), la.t);
                    }
                    if invert {
                        s = _mm256_sub_ps(c255, s);
                    }
                    let (x, y) = (
                        _mm256_mul_ps(_mm256_add_ps(u, half), bsx),
                        _mm256_mul_ps(_mm256_add_ps(v, half), bsy),
                    );
                    let mut r = tap(lb.lo, x, y);
                    if let Some(h) = lb.hi {
                        r = blend(r, tap(h, x, y), lb.t);
                    }
                    let k = iy * GRID + ix;
                    // A sample outside B is left as the scalar loop leaves it:
                    // zero, and not `ok`.
                    _mm256_storeu_ps(va.as_mut_ptr().add(k), _mm256_andnot_ps(out, s));
                    _mm256_storeu_ps(vb.as_mut_ptr().add(k), _mm256_andnot_ps(out, r));
                    for lane in 0..L {
                        ok[k + lane] = outside >> lane & 1 == 0;
                    }
                }
            }
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
#[cfg_attr(dispatch, inline(always))]
fn pixel_check(
    ta: &Thumb,
    tb: &Thumb,
    m: &Affine,
    aw: f32,
    ah: f32,
    bw: f32,
    bh: f32,
    invert: bool,
) -> (f32, u32, f32, f32) {
    // Taken out of the thread-local and put back, rather than borrowed in a
    // closure handed to `with`: under `cfg(dispatch)` that closure, with the
    // whole check inlined into it, was kept out of line, and out of the
    // x86-64-v3 copy of `verify`. Out and back moves four pointers.
    let (mut a, mut b) = PYRAMIDS.with(|cell| std::mem::take(&mut *cell.borrow_mut()));
    let r = pixel_check_with(ta, tb, m, aw, ah, bw, bh, invert, (&mut a, &mut b));
    PYRAMIDS.with(|cell| *cell.borrow_mut() = (a, b));
    r
}

thread_local! {
    /// Each worker's pyramid scratch, one per side of a comparison.
    static PYRAMIDS: std::cell::RefCell<(Pyramid, Pyramid)> = std::cell::RefCell::new(Default::default());
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(dispatch, inline(always))]
fn pixel_check_with(
    ta: &Thumb,
    tb: &Thumb,
    m: &Affine,
    aw: f32,
    ah: f32,
    bw: f32,
    bh: f32,
    invert: bool,
    pyramids: (&mut Pyramid, &mut Pyramid),
) -> (f32, u32, f32, f32) {
    // Bounding box, in A's frame, of the part of A that lands inside B.
    //
    // A probe's column depends on `ix` alone and its row on `iy` alone, and so
    // do the two products the transform takes of them — `apply` is
    // `(m0*x + m1*y + m2, m3*x + m4*y + m5)`, which is one term per axis and a
    // constant. Both halves were being worked out again for every probe, and
    // the column carried a *division* with it: `ix / 24`, five hundred and
    // seventy-six times, and `ix / 47` two thousand three hundred times in the
    // grid below, where a division is ten cycles and nothing else in the loop
    // is. Each is now taken once per row and once per column.
    //
    // The order the terms are added in is the order `apply` added them —
    // `((m0*x) + (m1*y)) + m2` — because float addition is not associative and
    // a probe that lands a bit either side of the frame is a different
    // bounding box, not a rounder one.
    const P: usize = 24;
    let mut pax = [0f32; P];
    let mut pux = [0f32; P];
    let mut pvx = [0f32; P];
    for ix in 0..P {
        let x = (ix as f32 + 0.5) / P as f32 * aw;
        pax[ix] = x;
        pux[ix] = m[0] * x;
        pvx[ix] = m[3] * x;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for iy in 0..P {
        let y = (iy as f32 + 0.5) / P as f32 * ah;
        let (uy, vy) = (m[1] * y, m[4] * y);
        for ix in 0..P {
            let (u, v) = (pux[ix] + uy + m[2], pvx[ix] + vy + m[5]);
            if u >= 0.0 && u < bw && v >= 0.0 && v < bh {
                let x = pax[ix];
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !(x1 > x0 && y1 > y0) {
        return (0.0, 0, 0.0, 0.0);
    }
    let mut va = [0f32; GRID * GRID];
    let mut vb = [0f32; GRID * GRID];
    let mut ok = [false; GRID * GRID];

    // How far apart consecutive grid samples land in each thumbnail, and hence
    // how wide a box each sample stands for. The two are almost never equal:
    // a photograph matched into a slide is read densely on one side and across
    // a handful of pixels on the other.
    let step_a = ((x1 - x0) / (GRID - 1) as f32).max((y1 - y0) / (GRID - 1) as f32);
    let lin = linear_scale(&m);
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
    let (pyr_a, pyr_b) = pyramids;
    let lod_a = ta.lod(raw_a, pyr_a);
    let lod_b = tb.lod(raw_b, pyr_b);

    // Each axis at its own scale. `Thumb::scale` is the horizontal one; the
    // thumbnail's two sides are rounded separately, so the vertical one differs
    // by up to half a thumbnail pixel over the frame — a misregistration that
    // grows down the picture and, like the half pixel in `to_level`, cancels
    // only when both sides are the same way up.
    let sa = (ta.w as f32 / aw, ta.h as f32 / ah);
    let sb = (tb.w as f32 / bw, tb.h as f32 / bh);
    let grid_a = GridTaps::of(&lod_a, (x0, x1 - x0), (y0, y1 - y0), sa);

    // The same two reductions as the bounding box above: the column's own
    // position and the transform's term in it, taken once per column.
    let mut gux = [0f32; GRID];
    let mut gvx = [0f32; GRID];
    for ix in 0..GRID {
        let x = x0 + (x1 - x0) * ix as f32 / (GRID - 1) as f32;
        gux[ix] = m[0] * x;
        gvx[ix] = m[3] * x;
    }
    #[allow(unused_mut)]
    let mut sampled = false;
    #[cfg(target_arch = "x86_64")]
    if crate::simd::v3() && wide::fits(&grid_a, &lod_a, &lod_b) {
        let rows: [f32; GRID] = std::array::from_fn(|iy| y0 + (y1 - y0) * iy as f32 / (GRID - 1) as f32);
        // `fits` has checked every level this reads, and the CPU has AVX2.
        unsafe { wide::sample(&grid_a, &lod_a, &lod_b, sb, m, &gux, &gvx, &rows, (bw, bh), invert, &mut va, &mut vb, &mut ok) };
        sampled = true;
    }
    if !sampled {
        for iy in 0..GRID {
            let y = y0 + (y1 - y0) * iy as f32 / (GRID - 1) as f32;
            let (uy, vy) = (m[1] * y, m[4] * y);
            for ix in 0..GRID {
                let (u, v) = (gux[ix] + uy + m[2], gvx[ix] + vy + m[5]);
                if u < 0.0 || u >= bw || v < 0.0 || v >= bh {
                    continue;
                }
                let k = iy * GRID + ix;
                let s = grid_a.at(&lod_a, ix, iy);
                // An inverted match is compared against the inverse of A
                // rather than by keeping a second copy of every thumbnail.
                va[k] = if invert { 255.0 - s } else { s };
                vb[k] = lod_b.at((u + 0.5) * sb.0, (v + 0.5) * sb.1);
                ok[k] = true;
            }
        }
    }
    let mut agree = 0f32;
    let mut total = 0u32;
    let mut worst = f32::INFINITY;
    // Whole-overlap correlation, summed as the blocks go by rather than in a
    // pass of its own. The blocks tile the grid exactly and every sample is in
    // one of them, so the same samples are summed; only the order is a block
    // at a time instead of a row at a time, which moves the last bits of a
    // figure nothing decides anything on — `ncc` is reported and dumped and
    // read by no rule. The pass it replaces was two thousand three hundred
    // more samples of `f64` arithmetic per pair considered.
    let (mut ta, mut tb_, mut taa, mut tbb, mut tab, mut tn) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
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
            ta += sa;
            tb_ += sb;
            taa += saa;
            tbb += sbb;
            tab += sab;
            tn += n as f64;
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
            let r = (cov / (vara * varb).sqrt()).abs() as f32;
            agree += r;
            worst = worst.min(r);
        }
    }
    // Whole-overlap correlation, for reporting.
    let (sa, sb, saa, sbb, sab, n) = (ta, tb_, taa, tbb, tab, tn);
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
    if total == 0 {
        return (0.0, 0, ncc, 0.0);
    }
    (agree / total as f32, total, ncc, worst)
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
    pts: Vec<(i32, i32)>,
    /// The four coordinate arrays again, the best hypothesis's outliers first.
    /// See `best_transform`.
    order: [Vec<f32>; 4],
}

pub struct Pair<'a> {
    pub fa: &'a Features,
    pub fb: &'a Features,
    pub ta: &'a Thumb,
    pub tb: &'a Thumb,
}

crate::simd::dispatched! {
    /// Full verification from a candidate correspondence list.
    ///
    /// `gate` is the weakest (inliers, overlap) any consumer of this verdict
    /// will accept. Below it the verdict is discarded whatever the pixels say,
    /// so the pixels are not read: the check is the most expensive thing in
    /// the pipeline and a third of the pairs reaching it have already lost on
    /// geometry.
    pub fn verify(p: &Pair, cands: &[(u32, u32)], var: Variant, gate: (u32, f32), matches: &mut Vec<(u32, u32)>, scratch: &mut Scratch) -> Verdict => verify_any;
}

/// `verify`, for whichever copy `dispatched!` chose.
#[cfg_attr(dispatch, inline(always))]
fn verify_any(p: &Pair, cands: &[(u32, u32)], var: Variant, gate: (u32, f32), matches: &mut Vec<(u32, u32)>, scratch: &mut Scratch) -> Verdict {
    let mut v = Verdict { variant: var, ..Default::default() };
    timed!(16, correspond(p.fa, p.fb, cands, matches));
    v.n_match = matches.len() as u32;
    let (bw, bh) = (p.fb.w as f32, p.fb.h as f32);
    let (aw, ah) = (p.fa.w as f32, p.fa.h as f32);
    let Some((m, n_agreeing)) = timed!(17, best_transform(p.fa, p.fb, matches, bw, bh, scratch)) else { return v };
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
    v.scale = linear_scale(&m);
    v.rot_deg = m[3].atan2(m[0]).to_degrees();
    // Asked first, since a verdict that lands frame on frame is read below
    // the gate as well (`Policy::standing`): four corners, so it costs nothing,
    // and a transform from a few coincidental points almost never passes.
    v.framed = frame_onto_frame(&m, aw, ah, bw, bh);
    let floor = |gate: u32| if v.framed { 3 } else { gate.max(3) };
    // **Everything below is gated on the aligned points, not just the pixels.**
    //
    // `gate.0` is the fewest any consumer of this verdict will accept, and
    // `n_in` counts distinct positions among the correspondences this transform
    // explains, so it cannot exceed the count returned above: a transform that
    // explains too few is rejected whatever the frames and the pixels say. On a
    // corpus where most files have no duplicate that is **99.9%** of the
    // verdicts the mirrored pass takes — 3.94 M of them, against 4,551 that
    // reach ten aligned points — and what they were paying for was two
    // measurements nobody would read: `encloses_centre`, which transforms every
    // keypoint of both images and costs as much as the geometry fit that
    // preceded it, and the sort in `distinct_inliers`.
    if n_agreeing < floor(gate.0) as usize {
        return v;
    }
    v.n_in = distinct_inliers(p.fa, matches, &scratch.mask, &mut scratch.pts);
    if v.n_in < floor(gate.0) {
        return v;
    }
    v.centred = timed!(18, encloses_centre(p.fa, p.fb, &m_query, matches, &scratch.mask));
    let (oa, ob) = timed!(19, overlap(&m, aw, ah, bw, bh));
    v.ov_a = oa;
    v.ov_b = ob;
    if v.ov_a.max(v.ov_b) >= gate.1 {
        let (blk, n, ncc, worst) = timed!(20, pixel_check(p.ta, p.tb, &m, aw, ah, bw, bh, var.invert));
        v.blk = blk;
        v.blk_n = n;
        v.ncc = ncc;
        v.blk_min = worst;
    }
    v
}

crate::simd::dispatched! {
    /// Check a transform that came from somewhere else — composed through a
    /// third image — using pixels only. No descriptor matching, so it costs
    /// almost nothing, and it is still a real test of this pair rather than an
    /// assumption that matching is transitive.
    pub fn verify_transform(p: &Pair, m: &Affine, var: Variant, min_ov: f32) -> Verdict => verify_transform_any;
}

/// `verify_transform`, for whichever copy `dispatched!` chose.
#[cfg_attr(dispatch, inline(always))]
fn verify_transform_any(p: &Pair, m: &Affine, var: Variant, min_ov: f32) -> Verdict {
    let mut v = Verdict { m: *m, variant: var, ..Default::default() };
    let (aw, ah) = (p.fa.w as f32, p.fa.h as f32);
    let (bw, bh) = (p.fb.w as f32, p.fb.h as f32);
    v.scale = linear_scale(&m);
    if !(v.scale.is_finite() && v.scale > 1e-3) {
        return v;
    }
    v.rot_deg = m[3].atan2(m[0]).to_degrees();
    let (oa, ob) = overlap(m, aw, ah, bw, bh);
    v.ov_a = oa;
    v.ov_b = ob;
    if v.ov_a.max(v.ov_b) >= min_ov {
        let (blk, n, ncc, worst) = timed!(43, pixel_check(p.ta, p.tb, m, aw, ah, bw, bh, var.invert));
        v.blk = blk;
        v.blk_n = n;
        v.ncc = ncc;
        v.blk_min = worst;
    }
    v
}

// ---------------------------------------------------------------- kernel timings

/// `cargo test --release -- --ignored --nocapture match_timings`
///
/// The per-candidate half of the matcher, at the shape the second look really
/// asks for. On a corpus where most files have no duplicate, the mirrored pass
/// reaches `correspond` and `best_transform` some four million times, with a
/// median of 56 candidate keypoint pairs spanning 48 distinct query keypoints —
/// and almost none of those pairs is a duplicate, so the geometry finds two or
/// three inliers and the verdict is thrown away. That is the case to make fast,
/// and it is not the case a duplicate exercises.
#[cfg(test)]
mod bench {
    use super::*;
    use crate::sift::DESC_LEN;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0
        }
        fn byte(&mut self) -> u8 {
            (self.next() >> 33) as u8
        }
        fn f(&mut self, hi: f32) -> f32 {
            (self.next() >> 40) as f32 / (1 << 24) as f32 * hi
        }
    }

    fn feats(rng: &mut Lcg, n: usize, w: u32, h: u32) -> Features {
        let (mut kps, mut desc) = (Vec::with_capacity(n), Vec::with_capacity(n * DESC_LEN));
        for _ in 0..n {
            kps.push(Keypoint {
                x: rng.f(w as f32),
                y: rng.f(h as f32),
                sigma: 1.6 + rng.f(20.0),
                angle: rng.f(360.0),
            });
            // Where the keypoint's response used to be drawn, so that the
            // rest of the fixture is the same draw it always was.
            let _ = rng.f(1.0);
            for _ in 0..DESC_LEN {
                desc.push(rng.byte());
            }
        }
        Features { w, h, kps: kps.into(), desc: desc.into() }
    }

    fn ms(mut f: impl FnMut()) -> f64 {
        let mut best = f64::MAX;
        for _ in 0..7 {
            let t = std::time::Instant::now();
            f();
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
        }
        best
    }

    #[test]
    #[ignore]
    fn match_timings() {
        let mut rng = Lcg(0x2545_f491_4f6c_dd1d);
        // A pool of images, so that the descriptors a call reads are not the
        // ones the last call read: `correspond` gathers two cache lines per
        // candidate out of a 76 KB descriptor block, and on a real corpus those
        // are cold.
        const POOL: usize = 96;
        let imgs: Vec<Features> = (0..POOL).map(|_| feats(&mut rng, 600, 640, 480)).collect();
        for &n_cand in &[16usize, 56, 200] {
            // Candidate pairs as `shared` emits them: sorted, deduplicated,
            // several B keypoints for some A keypoints.
            let mut sets: Vec<Vec<(u32, u32)>> = Vec::new();
            for _ in 0..POOL {
                let mut v: Vec<(u32, u32)> = (0..n_cand)
                    .map(|_| ((rng.next() % 600) as u32, (rng.next() % 600) as u32))
                    .collect();
                v.sort_unstable();
                v.dedup();
                sets.push(v);
            }
            let mut matches = Vec::new();
            let mut scratch = Scratch::default();
            let t_corr = ms(|| {
                for k in 0..POOL {
                    correspond(&imgs[k], &imgs[(k + 1) % POOL], &sets[k], &mut matches);
                    std::hint::black_box(&matches);
                }
            });
            // Geometry on the correspondences those produce.
            let mut ms_sets: Vec<Vec<(u32, u32)>> = Vec::new();
            for k in 0..POOL {
                correspond(&imgs[k], &imgs[(k + 1) % POOL], &sets[k], &mut matches);
                ms_sets.push(matches.clone());
            }
            let n_match: usize = ms_sets.iter().map(|m| m.len()).sum::<usize>() / POOL;
            let t_geom = ms(|| {
                for k in 0..POOL {
                    let r = best_transform(&imgs[k], &imgs[(k + 1) % POOL], &ms_sets[k], 640.0, 480.0, &mut scratch);
                    std::hint::black_box(&r);
                }
            });
            // And the two tests the verdict then faces, which run per verdict
            // whatever the geometry said.
            let m = [1.01f32, 0.02, 3.0, -0.02, 1.01, -4.0];
            let mask = vec![true; ms_sets[0].len()];
            let t_encl = ms(|| {
                for k in 0..POOL {
                    let mask = &mask[..ms_sets[k].len().min(mask.len())];
                    std::hint::black_box(encloses_centre(&imgs[k], &imgs[(k + 1) % POOL], &m, &ms_sets[k][..mask.len()], mask));
                }
            });
            println!(
                "{n_cand:4} candidates -> {n_match:3} matches: correspond {:7.1} us, best_transform {:7.1} us, encloses {:7.1} us  (per call)",
                t_corr * 1000.0 / POOL as f64,
                t_geom * 1000.0 / POOL as f64,
                t_encl * 1000.0 / POOL as f64
            );
        }
    }

    /// `cargo test --release -- --ignored --nocapture pixel_timings`
    ///
    /// The pixel check at the shapes it meets: a near-identity pair, a
    /// photograph found at a third of its size inside another, and a rotated
    /// one, each read through thumbnails of the size the tool keeps. It prints
    /// a checksum of every figure the check returned, so a change meant to be
    /// exact can be held to that in one line.
    #[test]
    #[ignore]
    fn pixel_timings() {
        let mut rng = Lcg(0x6a09_e667_f3bc_c908);
        let thumb = |rng: &mut Lcg, w: usize, h: usize| {
            // Smooth structure with some grain, so blocks are neither flat nor
            // pure noise.
            let (a, b, c) = (rng.f(0.2) + 0.02, rng.f(0.2) + 0.02, rng.f(6.0));
            let px: Vec<u8> = (0..w * h)
                .map(|i| {
                    let (x, y) = ((i % w) as f32, (i / w) as f32);
                    let v = 128.0 + 90.0 * (a * x + c).sin() * (b * y).cos() + (rng.byte() % 24) as f32;
                    v.clamp(0.0, 255.0) as u8
                })
                .collect();
            Thumb::new(w as u16, h as u16, w as f32 / 384.0, px)
        };
        const POOL: usize = 64;
        let ta: Vec<Thumb> = (0..POOL).map(|_| thumb(&mut rng, 128, 96)).collect();
        let tb: Vec<Thumb> = (0..POOL).map(|_| thumb(&mut rng, 128, 85)).collect();
        let (aw, ah, bw, bh) = (384.0f32, 288.0f32, 384.0f32, 255.0f32);
        let cases: [(&str, Affine); 4] = [
            ("near-identity", [1.01, 0.01, 2.0, -0.01, 0.99, -3.0]),
            ("third, inside", [0.33, 0.0, 120.0, 0.0, 0.33, 60.0]),
            ("rotated 30", [0.75, -0.43, 150.0, 0.43, 0.75, -40.0]),
            ("enlarged", [2.4, 0.1, -200.0, -0.1, 2.4, -150.0]),
        ];
        let mut sum = 0u64;
        for (name, m) in cases.iter() {
            for inv in [false, true] {
                for k in 0..POOL {
                    let (b, n, c, _) = pixel_check(&ta[k], &tb[k], m, aw, ah, bw, bh, inv);
                    sum = sum.wrapping_mul(0x100000001b3).wrapping_add(b.to_bits() as u64 ^ (n as u64) << 32 ^ c.to_bits() as u64);
                }
            }
            let t = ms(|| {
                for k in 0..POOL {
                    std::hint::black_box(pixel_check(&ta[k], &tb[k], m, aw, ah, bw, bh, false));
                }
            });
            println!("pixel_check {name:14}: {:7.2} us per call", t * 1000.0 / POOL as f64);
        }
        println!("pixel_check checksum {sum:016x}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame lands on a frame resized; turned, flipped, cropped, shifted or
    /// inside the other, it does not.
    #[test]
    fn a_frame_onto_a_frame_is_the_whole_picture_resized() {
        let (aw, ah) = (512.0, 384.0);
        // A quarter the size, and squashed to another shape.
        assert!(frame_onto_frame(&[0.25, 0.0, 0.0, 0.0, 0.25, 0.0], aw, ah, 128.0, 96.0));
        assert!(frame_onto_frame(&[0.5, 0.0, 0.0, 0.0, 1.0, 0.0], aw, ah, 256.0, 384.0));
        // Mirrored, turned a quarter, turned a half.
        assert!(!frame_onto_frame(&[-1.0, 0.0, aw, 0.0, 1.0, 0.0], aw, ah, aw, ah));
        assert!(!frame_onto_frame(&[0.0, -1.0, ah, 1.0, 0.0, 0.0], aw, ah, ah, aw));
        assert!(!frame_onto_frame(&[-1.0, 0.0, aw, 0.0, -1.0, ah], aw, ah, aw, ah));
        // A few pixels out, as a fitted transform is.
        assert!(frame_onto_frame(&[1.005, 0.0, 2.0, 0.0, 0.995, -1.5], aw, ah, aw, ah));
        // A crop of half of it, a shift, a frame inside another.
        assert!(!frame_onto_frame(&[2.0, 0.0, 0.0, 0.0, 2.0, 0.0], aw, ah, aw, ah));
        assert!(!frame_onto_frame(&[1.0, 0.0, 40.0, 0.0, 1.0, 0.0], aw, ah, aw, ah));
        assert!(!frame_onto_frame(&[0.5, 0.0, 100.0, 0.0, 0.5, 100.0], aw, ah, aw, ah));
        // Squashed inside the other's frame.
        assert!(!frame_onto_frame(&[1.0, 0.0, 0.0, 0.0, 0.1, 0.0], aw, ah, aw, ah));
    }

    /// Below the aligned-points bar, a verdict is an anchor only when it lands
    /// frame on frame and every detailed block agrees, and never clean.
    #[test]
    fn a_whole_frame_match_needs_fewer_points_and_every_block() {
        let policy = Policy::new(10, 0.85, 0.6);
        let at = |v: Verdict| policy.standing(&v);
        let v = Verdict { n_in: 7, framed: true, centred: true, ov_a: 1.0, ov_b: 1.0, scale: 1.0, blk: 0.99, blk_min: 0.95, ..Default::default() };
        assert_eq!(at(v), Standing::Anchor, "it may add a file to a cluster and never join two");
        assert!(at(Verdict { framed: false, ..v }) < Standing::Anchor);
        assert!(at(Verdict { blk_min: 0.85, ..v }) < Standing::Anchor, "one block disagreeing");
        assert!(at(Verdict { n_in: 2, ..v }) < Standing::Anchor);
        assert_eq!(at(Verdict { n_in: 10, framed: false, blk_min: 0.5, ..v }), Standing::Anchor, "the bar met is the bar as it was");
    }

    /// The pyramid one output at a time, each pixel the [1 3 3 1]/8 filter
    /// both ways over the level below: the reference `Pyramid::fill` is held
    /// to.
    fn pyramid_reference(w: u16, h: u16, px: &[u8]) -> Vec<(u16, u16, Vec<u8>)> {
        let mut mips: Vec<(u16, u16, Vec<u8>)> = Vec::new();
        let (mut cw, mut ch) = (w as usize, h as usize);
        while cw >= 4 && ch >= 4 {
            let (nw, nh) = (cw / 2, ch / 2);
            let cur: &[u8] = match mips.last() {
                None => px,
                Some((_, _, p)) => p,
            };
            let mut next = Vec::with_capacity(nw * nh);
            let k = [1u32, 3, 3, 1];
            let at = |x: isize, y: isize| cur[y.clamp(0, ch as isize - 1) as usize * cw + x.clamp(0, cw as isize - 1) as usize] as u32;
            for y in 0..nh {
                next.extend((0..nw).map(|x| {
                    let mut s = 0;
                    for (j, kj) in k.iter().enumerate() {
                        for (i, ki) in k.iter().enumerate() {
                            s += kj * ki * at(2 * x as isize - 1 + i as isize, 2 * y as isize - 1 + j as isize);
                        }
                    }
                    ((s + 32) / 64) as u8
                }));
            }
            mips.push((nw as u16, nh as u16, next));
            cw = nw;
            ch = nh;
        }
        mips
    }

    /// Every level, at every shape the interior loop and the clamped edges
    /// divide differently: under sixteen outputs, odd widths and heights, exact
    /// multiples, the 128-pixel side thumbnails really have, and the
    /// extremes of the byte range.
    #[test]
    fn the_pyramid_is_built_as_its_reference() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut byte = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 56) as u8
        };
        let mut pyr = Pyramid::default();
        for &(w, h) in [(1u16, 1u16), (3, 9), (4, 4), (5, 7), (31, 4), (32, 32), (33, 17), (63, 65), (128, 96), (96, 128), (127, 85), (128, 128)].iter() {
            for fill in 0..3 {
                let px: Vec<u8> = (0..w as usize * h as usize)
                    .map(|_| match fill {
                        0 => byte(),
                        1 => 255,
                        _ => byte() & 1,
                    })
                    .collect();
                let t = Thumb::new(w, h, 1.0, px);
                pyr.fill(&t, usize::MAX);
                let want = pyramid_reference(w, h, &t.px);
                assert_eq!(pyr.len(), want.len(), "{w}x{h}");
                for (i, (ww, wh, wp)) in want.iter().enumerate() {
                    let (p, gw, gh) = pyr.level(i);
                    assert_eq!((gw, gh), (*ww as usize, *wh as usize));
                    assert_eq!(p, &wp[..], "{w}x{h} level {i}");
                }
            }
        }
    }

    /// A verdict read the other way round is the same claim about the same two
    /// files: the overlaps swap, the scale is the reciprocal, the rotation is
    /// undone, and reversing twice gives back what it started from.
    #[test]
    fn a_reversed_verdict_describes_the_same_pair_from_the_other_side() {
        let (s, t) = (0.5f32, 30f32.to_radians());
        let m: Affine = [s * t.cos(), -s * t.sin(), 12.0, s * t.sin(), s * t.cos(), -4.0];
        let v = Verdict { m, ov_a: 0.9, ov_b: 0.3, scale: s, rot_deg: 30.0, n_in: 17, blk: 0.8, centred: true, ..Default::default() };
        let r = v.reversed().unwrap();
        assert_eq!((r.ov_a, r.ov_b), (0.3, 0.9));
        assert!((r.scale - 2.0).abs() < 1e-4, "{}", r.scale);
        assert!((r.rot_deg + 30.0).abs() < 1e-3, "{}", r.rot_deg);
        assert_eq!((r.n_in, r.blk, r.centred), (17, 0.8, true));
        let back = r.reversed().unwrap();
        for (a, b) in back.m.iter().zip(m.iter()) {
            assert!((a - b).abs() < 1e-4);
        }
        assert!(Verdict { m: [0.0; 6], ..Default::default() }.reversed().is_none());
    }
}
