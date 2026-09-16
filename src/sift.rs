//! Scale-invariant local features: difference-of-Gaussians keypoints with
//! gradient-orientation descriptors, in the SIFT family.
//!
//! Conventions follow OpenCV's implementation closely on purpose, so that the
//! descriptors here can be checked against a reference: angles are degrees,
//! `dy` is `img(y-1) - img(y+1)`, the descriptor is 4x4 cells x 8 bins,
//! L2-normalised, clipped at 0.2, renormalised and scaled by 512 into a u8.
//!
//! What matters for the matcher downstream:
//!   * `Keypoint::sigma` is in working-image pixels, `angle` in degrees, and a
//!     single correspondence between two keypoints therefore fixes a
//!     similarity transform (scale, rotation, translation).
//!   * The mirror and inversion of an image have descriptors that are fixed
//!     permutations of the original's bins (`MIRROR_PERM`, `INVERT_PERM`), so
//!     those hypotheses cost no extra extraction.

use crate::decode::Gray;

pub const DESC_LEN: usize = 128;
const D: usize = 4; // descriptor grid
const N: usize = 8; // orientation bins per cell
const ORI_BINS: usize = 36;
const ORI_SIG_FCTR: f32 = 1.5;
const ORI_RADIUS: f32 = 3.0 * ORI_SIG_FCTR;
const ORI_PEAK_RATIO: f32 = 0.8;
const DESCR_SCL_FCTR: f32 = 3.0;
const DESCR_MAG_THR: f32 = 0.2;
const INT_DESCR_FCTR: f32 = 512.0;
const IMG_BORDER: i32 = 5;
const MAX_INTERP_STEPS: usize = 5;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub n_layers: usize,
    pub sigma: f32,
    /// Minimum |DoG| response, in units of the input's own quantisation.
    ///
    /// Deliberately far below the 0.04 a standard SIFT uses, and deliberately
    /// not a tuned number: keypoints are ranked by response and cut to
    /// `max_features`, so on a textured image the threshold decides nothing at
    /// all — the ranking does. What it must not do is starve a dark or nearly
    /// flat image, where a standard threshold returns almost nothing and an
    /// image with no features cannot be matched to anything. So it is pinned
    /// to the smallest difference the input can actually carry: two 8-bit
    /// codes, below which a lossy codec preserves nothing anyway.
    pub contrast: f32,
    pub edge: f32,
    pub max_features: usize,
    /// Images are doubled until their long side reaches this.
    pub upsample_below: usize,
    /// Detected extrema considered, as a multiple of `max_features`. A wider
    /// pool costs only the ranking, since the losers are never described, and
    /// buys a better-chosen set of keypoints. Measured flat from 2 upwards, so
    /// it is a constant rather than an option.
    pub candidate_pool: usize,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            n_layers: 3,
            sigma: 1.6,
            contrast: 2.0 / 255.0,
            edge: 10.0,
            max_features: 800,
            upsample_below: 512,
            candidate_pool: 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Keypoint {
    pub x: f32,
    pub y: f32,
    /// Scale in working-image pixels.
    pub sigma: f32,
    /// Degrees, 0..360, OpenCV convention.
    pub angle: f32,
    pub response: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Features {
    pub w: u32,
    pub h: u32,
    pub kps: Vec<Keypoint>,
    /// `kps.len() * DESC_LEN` bytes.
    pub desc: Vec<u8>,
}

impl Features {
    #[inline]
    pub fn d(&self, i: usize) -> &[u8] {
        &self.desc[i * DESC_LEN..(i + 1) * DESC_LEN]
    }
    pub fn len(&self) -> usize {
        self.kps.len()
    }
}

// ---------------------------------------------------------------- math

/// exp(-t) for t in [0, EXP_RANGE), sampled; every Gaussian weight in the
/// extractor goes through here instead of calling exp per pixel.
const EXP_RANGE: f32 = 40.0;
const EXP_N: usize = 8192;
struct ExpTable([f32; EXP_N]);
static EXP_TABLE: std::sync::LazyLock<ExpTable> = std::sync::LazyLock::new(|| {
    let mut t = [0f32; EXP_N];
    for (i, v) in t.iter_mut().enumerate() {
        *v = (-(i as f32 + 0.5) * EXP_RANGE / EXP_N as f32).exp();
    }
    ExpTable(t)
});
#[inline]
fn exp_neg(t: f32) -> f32 {
    if t >= EXP_RANGE {
        return 0.0;
    }
    EXP_TABLE.0[(t * (EXP_N as f32 / EXP_RANGE)) as usize]
}

/// OpenCV's fastAtan2: degrees in 0..360, max error ~0.3 degrees.
#[inline]
pub fn fast_atan2_deg(y: f32, x: f32) -> f32 {
    const P1: f32 = 0.999_787_8 * (180.0 / std::f32::consts::PI);
    const P3: f32 = -0.325_808_4 * (180.0 / std::f32::consts::PI);
    const P5: f32 = 0.155_578_65 * (180.0 / std::f32::consts::PI);
    const P7: f32 = -0.044_326_555 * (180.0 / std::f32::consts::PI);
    let ax = x.abs();
    let ay = y.abs();
    let a = if ax >= ay {
        let c = ay / (ax + f32::EPSILON);
        let c2 = c * c;
        (((P7 * c2 + P5) * c2 + P3) * c2 + P1) * c
    } else {
        let c = ax / (ay + f32::EPSILON);
        let c2 = c * c;
        90.0 - (((P7 * c2 + P5) * c2 + P3) * c2 + P1) * c
    };
    let a = if x < 0.0 { 180.0 - a } else { a };
    if y < 0.0 { 360.0 - a } else { a }
}

/// Gradient magnitude and orientation (degrees) of a layer, computed once and
/// shared by every keypoint that lands on it.
struct Grad {
    w: usize,
    mag: Vec<f32>,
    ori: Vec<f32>,
}

impl Grad {
    fn of(l: &Layer) -> Grad {
        let (w, h) = (l.w, l.h);
        let mut mag = vec![0.0f32; w * h];
        let mut ori = vec![0.0f32; w * h];
        for y in 1..h.saturating_sub(1) {
            let up = &l.px[(y - 1) * w..y * w];
            let row = &l.px[y * w..(y + 1) * w];
            let dn = &l.px[(y + 1) * w..(y + 2) * w];
            for x in 1..w - 1 {
                let dx = row[x + 1] - row[x - 1];
                let dy = up[x] - dn[x];
                mag[y * w + x] = (dx * dx + dy * dy).sqrt();
                ori[y * w + x] = fast_atan2_deg(dy, dx);
            }
        }
        Grad { w, mag, ori }
    }
    #[inline]
    fn at(&self, x: i32, y: i32) -> (f32, f32) {
        let i = y as usize * self.w + x as usize;
        (self.mag[i], self.ori[i])
    }
}

// ---------------------------------------------------------------- pyramid

struct Layer {
    w: usize,
    h: usize,
    px: Vec<f32>,
}

impl Layer {
    #[inline]
    fn at(&self, x: i32, y: i32) -> f32 {
        self.px[y as usize * self.w + x as usize]
    }
}

fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let radius = (sigma * 3.0).ceil().max(1.0) as usize;
    let mut k = vec![0.0f32; 2 * radius + 1];
    let mut sum = 0.0;
    for i in 0..k.len() {
        let x = i as f32 - radius as f32;
        let v = (-x * x / (2.0 * sigma * sigma)).exp();
        k[i] = v;
        sum += v;
    }
    for v in k.iter_mut() {
        *v /= sum;
    }
    k
}

/// Separable Gaussian blur with reflect-101 borders.
fn blur(src: &Layer, sigma: f32) -> Layer {
    let k = gaussian_kernel(sigma);
    let r = k.len() / 2;
    let (w, h) = (src.w, src.h);
    let mut tmp = vec![0.0f32; w * h];
    // horizontal: symmetric kernel, 8 outputs at a time so the tap loop
    // stays in registers.
    let mut padded = vec![0.0f32; w + 2 * r];
    let kc = k[r];
    let ks: Vec<f32> = (1..=r).map(|t| k[r + t]).collect();
    for y in 0..h {
        let row = &src.px[y * w..(y + 1) * w];
        for i in 0..r {
            padded[i] = row[reflect101(i as i32 - r as i32, w)];
            padded[w + r + i] = row[reflect101((w + i) as i32, w)];
        }
        padded[r..r + w].copy_from_slice(row);
        let out = &mut tmp[y * w..(y + 1) * w];
        let mut x = 0;
        while x + 8 <= w {
            let mut acc = [0f32; 8];
            let c = &padded[x + r..x + r + 8];
            for i in 0..8 {
                acc[i] = c[i] * kc;
            }
            for (t, &kv) in ks.iter().enumerate() {
                let l = &padded[x + r - t - 1..x + r - t - 1 + 8];
                let rr = &padded[x + r + t + 1..x + r + t + 1 + 8];
                for i in 0..8 {
                    acc[i] += (l[i] + rr[i]) * kv;
                }
            }
            out[x..x + 8].copy_from_slice(&acc);
            x += 8;
        }
        while x < w {
            let mut acc = padded[x + r] * kc;
            for (t, &kv) in ks.iter().enumerate() {
                acc += (padded[x + r - t - 1] + padded[x + r + t + 1]) * kv;
            }
            out[x] = acc;
            x += 1;
        }
    }
    // vertical: symmetric pairs of rows
    let mut dst = vec![0.0f32; w * h];
    for y in 0..h {
        let out = &mut dst[y * w..(y + 1) * w];
        let c = &tmp[y * w..(y + 1) * w];
        for x in 0..w {
            out[x] = c[x] * kc;
        }
        for (t, &kv) in ks.iter().enumerate() {
            let ya = reflect101(y as i32 - t as i32 - 1, h);
            let yb = reflect101(y as i32 + t as i32 + 1, h);
            let a = &tmp[ya * w..(ya + 1) * w];
            let b = &tmp[yb * w..(yb + 1) * w];
            for x in 0..w {
                out[x] += (a[x] + b[x]) * kv;
            }
        }
    }
    Layer { w, h, px: dst }
}

#[inline]
fn reflect101(i: i32, n: usize) -> usize {
    let n = n as i32;
    if n == 1 {
        return 0;
    }
    let mut i = i;
    while i < 0 || i >= n {
        if i < 0 {
            i = -i;
        }
        if i >= n {
            i = 2 * (n - 1) - i;
        }
    }
    i as usize
}

/// Bilinear enlargement by an integer factor.
fn upsample(g: &Gray, f: usize) -> Layer {
    let (w, h) = (g.w * f, g.h * f);
    let ff = f as f32;
    let mut px = vec![0.0f32; w * h];
    for y in 0..h {
        let sy = (y as f32 + 0.5) / ff - 0.5;
        let y0 = sy.floor().max(0.0) as usize;
        let y1 = (y0 + 1).min(g.h - 1);
        let fy = (sy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..w {
            let sx = (x as f32 + 0.5) / ff - 0.5;
            let x0 = sx.floor().max(0.0) as usize;
            let x1 = (x0 + 1).min(g.w - 1);
            let fx = (sx - x0 as f32).clamp(0.0, 1.0);
            let a = g.px[y0 * g.w + x0] * (1.0 - fx) + g.px[y0 * g.w + x1] * fx;
            let b = g.px[y1 * g.w + x0] * (1.0 - fx) + g.px[y1 * g.w + x1] * fx;
            px[y * w + x] = a * (1.0 - fy) + b * fy;
        }
    }
    Layer { w, h, px }
}

fn halve(src: &Layer) -> Layer {
    let (w, h) = ((src.w / 2).max(1), (src.h / 2).max(1));
    let mut px = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            px[y * w + x] = src.px[(y * 2) * src.w + x * 2];
        }
    }
    Layer { w, h, px }
}

// ---------------------------------------------------------------- extraction

pub fn extract(g: &Gray, p: &Params) -> Features {
    let mut feats = Features { w: g.w as u32, h: g.h as u32, ..Default::default() };
    if g.w < 8 || g.h < 8 {
        return feats;
    }
    // A small image is enlarged until it is worth analysing. A 225x225 seed
    // scaled to 12% is 28 pixels across and has essentially no scale-space to
    // search: doubling it once, as a standard SIFT does, still leaves 56. The
    // enlargement invents no detail, but it gives the octaves room, and
    // matching a thumbnail to the photograph it came from is most of what is
    // left to win on this corpus.
    let mut factor = 1usize;
    while g.w.max(g.h) * factor * 2 <= p.upsample_below.max(2) {
        factor *= 2;
    }
    let (base, init_sigma, coord_scale) = if factor > 1 {
        (upsample(g, factor), 0.5 * factor as f32, 1.0 / factor as f32)
    } else {
        (Layer { w: g.w, h: g.h, px: g.px.clone() }, 0.5f32, 1.0f32)
    };
    let sig_diff = (p.sigma * p.sigma - init_sigma * init_sigma).max(0.01).sqrt();
    let base = blur(&base, sig_diff);

    let min_side = base.w.min(base.h) as f32;
    let n_octaves = ((min_side.ln() / 2f32.ln()).round() as i32 - 2).max(1) as usize;
    let s = p.n_layers;
    let k = 2f32.powf(1.0 / s as f32);
    let mut sig = vec![p.sigma; s + 3];
    for i in 1..s + 3 {
        let prev = p.sigma * k.powi(i as i32 - 1);
        let total = prev * k;
        sig[i] = (total * total - prev * prev).sqrt();
    }

    let thr_pre = 0.5 * p.contrast / s as f32;

    // Detection and description are separated deliberately.
    //
    // The obvious structure is to describe each octave's keypoints while that
    // octave's pyramid is still in hand, and then keep the best `max_features`
    // overall. That describes roughly ten times as many keypoints as it keeps:
    // every octave finds its own budget's worth, and all but a fraction are
    // thrown away afterwards. Since describing a keypoint costs far more than
    // finding one, the whole pyramid is detected first, ranked once, and only
    // the survivors are described. The gradient layers are kept for that
    // second pass; the Gaussian and difference-of-Gaussian layers are not
    // needed again and are dropped as each octave finishes.
    let mut cands: Vec<Cand> = Vec::new();
    let mut grads: Vec<Vec<Option<Grad>>> = Vec::with_capacity(n_octaves);
    let mut heights: Vec<usize> = Vec::with_capacity(n_octaves);

    let mut octave_base = base;
    for o in 0..n_octaves {
        let mut gauss: Vec<Layer> = Vec::with_capacity(s + 3);
        gauss.push(std::mem::replace(&mut octave_base, Layer { w: 0, h: 0, px: vec![] }));
        for i in 1..s + 3 {
            let l = blur(&gauss[i - 1], sig[i]);
            gauss.push(l);
        }
        let dog: Vec<Layer> = (0..s + 2)
            .map(|i| {
                let a = &gauss[i + 1];
                let b = &gauss[i];
                Layer { w: a.w, h: a.h, px: a.px.iter().zip(&b.px).map(|(x, y)| x - y).collect() }
            })
            .collect();
        find_extrema(&dog, o, p, thr_pre, coord_scale, &mut cands);
        heights.push(gauss[0].h);
        grads.push(
            (0..s + 3)
                .map(|i| if (1..=s).contains(&i) { Some(Grad::of(&gauss[i])) } else { None })
                .collect(),
        );
        if o + 1 < n_octaves {
            octave_base = halve(&gauss[s]);
        }
    }

    // Drop repeats of one extremum found from two adjacent scales *within* an
    // octave. Across octaves the same point is not a repeat: the two
    // detections are described at different resolutions and match different
    // views of the picture, which is exactly what makes a thumbnail findable
    // in a full-size photograph.
    cands.sort_by(|a, b| {
        (a.octave, key3(&a.kp))
            .cmp(&(b.octave, key3(&b.kp)))
            .then(b.kp.response.partial_cmp(&a.kp.response).unwrap())
    });
    cands.dedup_by(|a, b| a.octave == b.octave && key3(&a.kp) == key3(&b.kp));

    // Then rank once, across the whole pyramid, and describe only the best.
    cands.sort_by(|a, b| {
        b.kp.response
            .partial_cmp(&a.kp.response)
            .unwrap()
            .then((a.octave, key3(&a.kp)).cmp(&(b.octave, key3(&b.kp))))
    });
    // A keypoint can carry more than one dominant orientation, so a few more
    // are described than are finally kept.
    cands.truncate(p.max_features * p.candidate_pool + 8);

    for c in cands.iter() {
        let oct_scale = (1u32 << c.octave) as f32 * coord_scale;
        let grad = grads[c.octave][c.layer].as_ref().unwrap();
        let h = heights[c.octave];
        let scl_octv = c.kp.sigma / oct_scale;
        let px = c.kp.x / oct_scale;
        let py = c.kp.y / oct_scale;
        let mut hist = [0f32; ORI_BINS];
        let radius = (ORI_RADIUS * scl_octv).round() as i32;
        let omax = orientation_hist(grad, h, px, py, radius, ORI_SIG_FCTR * scl_octv, &mut hist);
        let mag_thr = omax * ORI_PEAK_RATIO;
        for j in 0..ORI_BINS {
            let l = if j > 0 { j - 1 } else { ORI_BINS - 1 };
            let r2 = if j < ORI_BINS - 1 { j + 1 } else { 0 };
            if hist[j] > hist[l] && hist[j] > hist[r2] && hist[j] >= mag_thr {
                let mut bin = j as f32 + 0.5 * (hist[l] - hist[r2]) / (hist[l] - 2.0 * hist[j] + hist[r2]);
                if bin < 0.0 {
                    bin += ORI_BINS as f32;
                } else if bin >= ORI_BINS as f32 {
                    bin -= ORI_BINS as f32;
                }
                let mut angle = 360.0 - (360.0 / ORI_BINS as f32) * bin;
                if (angle - 360.0).abs() < 1e-5 {
                    angle = 0.0;
                }
                let mut kp = c.kp;
                kp.angle = angle;
                let mut d = [0u8; DESC_LEN];
                descriptor(grad, h, px, py, angle, scl_octv, &mut d);
                feats.kps.push(kp);
                feats.desc.extend_from_slice(&d);
            }
        }
    }
    retain_best(&mut feats, p.max_features);
    feats
}

/// The three rows of a layer centred on `y`.
#[inline]
fn rows3(l: &Layer, y: usize, w: usize) -> (&[f32], &[f32], &[f32]) {
    (&l.px[(y - 1) * w..y * w], &l.px[y * w..(y + 1) * w], &l.px[(y + 1) * w..(y + 2) * w])
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn is_extreme(
    v: f32,
    x: usize,
    c0: &[f32], c2: &[f32],
    p0: &[f32], p1: &[f32], p2: &[f32],
    n0: &[f32], n1: &[f32], n2: &[f32],
    max: bool,
) -> bool {
    let rows = [c0, c2, p0, p1, p2, n0, n1, n2];
    if max {
        for r in rows {
            if v < r[x - 1] || v < r[x] || v < r[x + 1] {
                return false;
            }
        }
    } else {
        for r in rows {
            if v > r[x - 1] || v > r[x] || v > r[x + 1] {
                return false;
            }
        }
    }
    true
}

/// A detected extremum, before it has an orientation or a descriptor.
struct Cand {
    kp: Keypoint,
    octave: usize,
    layer: usize,
}

/// Quantised position and scale, for spotting the same extremum twice.
#[inline]
fn key3(k: &Keypoint) -> (i32, i32, i32) {
    ((k.x * 4.0) as i32, (k.y * 4.0) as i32, (k.sigma * 16.0) as i32)
}

fn find_extrema(dog: &[Layer], octave: usize, p: &Params, thr_pre: f32, coord_scale: f32, out: &mut Vec<Cand>) {
    let s = p.n_layers;
    let (w, h) = (dog[0].w as i32, dog[0].h as i32);
    if w <= 2 * IMG_BORDER || h <= 2 * IMG_BORDER {
        return;
    }
    let oct_scale = (1u32 << octave) as f32 * coord_scale;
    for layer in 1..=s {
        let cur = &dog[layer];
        let prv = &dog[layer - 1];
        let nxt = &dog[layer + 1];
        let wu = w as usize;
        for y in IMG_BORDER..h - IMG_BORDER {
            let yu = y as usize;
            let (c0, c1, c2) = rows3(cur, yu, wu);
            let (p0, p1, p2) = rows3(prv, yu, wu);
            let (n0, n1, n2) = rows3(nxt, yu, wu);
            for x in IMG_BORDER..w - IMG_BORDER {
                let xu = x as usize;
                let v = c1[xu];
                if v.abs() <= thr_pre {
                    continue;
                }
                // The two horizontal neighbours reject most candidates, and
                // they are already in cache, so they are tested before the
                // other twenty-four.
                let positive = v > 0.0;
                if positive {
                    if v < c1[xu - 1] || v < c1[xu + 1] {
                        continue;
                    }
                } else if v > c1[xu - 1] || v > c1[xu + 1] {
                    continue;
                }
                // The current row's own x-1 and x+1 are already done above;
                // `is_extreme` covers the other twenty-four.
                let ok = is_extreme(v, xu, c0, c2, p0, p1, p2, n0, n1, n2, positive);
                if !ok {
                    continue;
                }
                if let Some((kp, lay)) = adjust(dog, octave, layer, x, y, p, oct_scale) {
                    out.push(Cand { kp, octave, layer: lay });
                }
            }
        }
    }
}

/// Sub-pixel/scale refinement, contrast and edge tests. Returns the keypoint
/// in working-image coordinates and the Gaussian layer index to use.
fn adjust(
    dog: &[Layer],
    _octave: usize,
    layer0: usize,
    x0: i32,
    y0: i32,
    p: &Params,
    oct_scale: f32,
) -> Option<(Keypoint, usize)> {
    let s = p.n_layers;
    let (mut layer, mut x, mut y) = (layer0 as i32, x0, y0);
    let (w, h) = (dog[0].w as i32, dog[0].h as i32);
    let mut xi = 0.0f32;
    let mut xr = 0.0f32;
    let mut xc = 0.0f32;
    let mut converged = false;
    for _ in 0..MAX_INTERP_STEPS {
        let cur = &dog[layer as usize];
        let prv = &dog[layer as usize - 1];
        let nxt = &dog[layer as usize + 1];
        let dx = (cur.at(x + 1, y) - cur.at(x - 1, y)) * 0.5;
        let dy = (cur.at(x, y + 1) - cur.at(x, y - 1)) * 0.5;
        let ds = (nxt.at(x, y) - prv.at(x, y)) * 0.5;
        let v2 = cur.at(x, y) * 2.0;
        let dxx = cur.at(x + 1, y) + cur.at(x - 1, y) - v2;
        let dyy = cur.at(x, y + 1) + cur.at(x, y - 1) - v2;
        let dss = nxt.at(x, y) + prv.at(x, y) - v2;
        let dxy = (cur.at(x + 1, y + 1) - cur.at(x - 1, y + 1) - cur.at(x + 1, y - 1) + cur.at(x - 1, y - 1)) * 0.25;
        let dxs = (nxt.at(x + 1, y) - nxt.at(x - 1, y) - prv.at(x + 1, y) + prv.at(x - 1, y)) * 0.25;
        let dys = (nxt.at(x, y + 1) - nxt.at(x, y - 1) - prv.at(x, y + 1) + prv.at(x, y - 1)) * 0.25;
        // solve H X = -g
        let hm = [[dxx, dxy, dxs], [dxy, dyy, dys], [dxs, dys, dss]];
        let g = [dx, dy, ds];
        let sol = solve3(hm, g)?;
        xc = -sol[0];
        xr = -sol[1];
        xi = -sol[2];
        if xc.abs() < 0.5 && xr.abs() < 0.5 && xi.abs() < 0.5 {
            converged = true;
            break;
        }
        if xc.abs() > 1e6 || xr.abs() > 1e6 || xi.abs() > 1e6 {
            return None;
        }
        x += xc.round() as i32;
        y += xr.round() as i32;
        layer += xi.round() as i32;
        if layer < 1 || layer > s as i32 || x < IMG_BORDER || x >= w - IMG_BORDER || y < IMG_BORDER || y >= h - IMG_BORDER {
            return None;
        }
    }
    if !converged {
        return None;
    }
    let cur = &dog[layer as usize];
    let prv = &dog[layer as usize - 1];
    let nxt = &dog[layer as usize + 1];
    let dx = (cur.at(x + 1, y) - cur.at(x - 1, y)) * 0.5;
    let dy = (cur.at(x, y + 1) - cur.at(x, y - 1)) * 0.5;
    let ds = (nxt.at(x, y) - prv.at(x, y)) * 0.5;
    let contr = cur.at(x, y) + 0.5 * (dx * xc + dy * xr + ds * xi);
    if contr.abs() * (s as f32) < p.contrast {
        return None;
    }
    let v2 = cur.at(x, y) * 2.0;
    let dxx = cur.at(x + 1, y) + cur.at(x - 1, y) - v2;
    let dyy = cur.at(x, y + 1) + cur.at(x, y - 1) - v2;
    let dxy = (cur.at(x + 1, y + 1) - cur.at(x - 1, y + 1) - cur.at(x + 1, y - 1) + cur.at(x - 1, y - 1)) * 0.25;
    let tr = dxx + dyy;
    let det = dxx * dyy - dxy * dxy;
    if det <= 0.0 || tr * tr * p.edge >= (p.edge + 1.0) * (p.edge + 1.0) * det {
        return None;
    }
    let kp = Keypoint {
        x: (x as f32 + xc) * oct_scale,
        y: (y as f32 + xr) * oct_scale,
        sigma: p.sigma * 2f32.powf((layer as f32 + xi) / s as f32) * oct_scale,
        angle: 0.0,
        response: contr.abs(),
    };
    Some((kp, layer as usize))
}

fn solve3(a: [[f32; 3]; 3], b: [f32; 3]) -> Option<[f32; 3]> {
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let mut x = [0f32; 3];
    for i in 0..3 {
        let mut m = a;
        for r in 0..3 {
            m[r][i] = b[r];
        }
        let d = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        x[i] = d * inv;
    }
    Some(x)
}

fn orientation_hist(g: &Grad, h: usize, px: f32, py: f32, radius: i32, sigma: f32, hist: &mut [f32; ORI_BINS]) -> f32 {
    let expf_scale = -1.0 / (2.0 * sigma * sigma);
    let mut temphist = [0f32; ORI_BINS];
    let (w, h) = (g.w as i32, h as i32);
    let cx = px.round() as i32;
    let cy = py.round() as i32;
    let neg_scale = -expf_scale;
    for i in -radius..=radius {
        let y = cy + i;
        if y <= 0 || y >= h - 1 {
            continue;
        }
        for j in -radius..=radius {
            let x = cx + j;
            if x <= 0 || x >= w - 1 {
                continue;
            }
            let (mag, ori) = g.at(x, y);
            let wgt = exp_neg((i * i + j * j) as f32 * neg_scale);
            let mut bin = (ori * ORI_BINS as f32 / 360.0).round() as i32;
            if bin >= ORI_BINS as i32 {
                bin -= ORI_BINS as i32;
            }
            if bin < 0 {
                bin += ORI_BINS as i32;
            }
            temphist[bin as usize] += wgt * mag;
        }
    }
    let n = ORI_BINS;
    let mut maxval = 0.0f32;
    for i in 0..n {
        let v = (temphist[(i + n - 2) % n] + temphist[(i + 2) % n]) * (1.0 / 16.0)
            + (temphist[(i + n - 1) % n] + temphist[(i + 1) % n]) * (4.0 / 16.0)
            + temphist[i] * (6.0 / 16.0);
        hist[i] = v;
        maxval = maxval.max(v);
    }
    maxval
}

fn descriptor(g: &Grad, h: usize, px: f32, py: f32, kp_angle: f32, scl: f32, dst: &mut [u8; DESC_LEN]) {
    let mut ori = 360.0 - kp_angle;
    if (ori - 360.0).abs() < 1e-5 {
        ori = 0.0;
    }
    let (rows, cols) = (h as i32, g.w as i32);
    let pt_x = px.round() as i32;
    let pt_y = py.round() as i32;
    let mut cos_t = ori.to_radians().cos();
    let mut sin_t = ori.to_radians().sin();
    let bins_per_rad = N as f32 / 360.0;
    let neg_exp_scale = 1.0 / (D as f32 * D as f32 * 0.5);
    let hist_width = DESCR_SCL_FCTR * scl;
    let mut radius = (hist_width * 2f32.sqrt() * (D as f32 + 1.0) * 0.5).round() as i32;
    let diag = ((cols * cols + rows * rows) as f32).sqrt() as i32;
    radius = radius.min(diag);
    cos_t /= hist_width;
    sin_t /= hist_width;

    let hlen = (D + 2) * (D + 2) * (N + 2);
    let mut hist = [0f32; (D + 2) * (D + 2) * (N + 2)];
    debug_assert_eq!(hlen, hist.len());

    for i in -radius..=radius {
        for j in -radius..=radius {
            let c_rot = j as f32 * cos_t - i as f32 * sin_t;
            let r_rot = j as f32 * sin_t + i as f32 * cos_t;
            let rbin = r_rot + (D / 2) as f32 - 0.5;
            let cbin = c_rot + (D / 2) as f32 - 0.5;
            let r = pt_y + i;
            let c = pt_x + j;
            if rbin > -1.0 && rbin < D as f32 && cbin > -1.0 && cbin < D as f32 && r > 0 && r < rows - 1 && c > 0 && c < cols - 1 {
                let (m, o) = g.at(c, r);
                let wgt = exp_neg((c_rot * c_rot + r_rot * r_rot) * neg_exp_scale);
                let mag = m * wgt;
                let obin = (o - ori) * bins_per_rad;
                let r0 = rbin.floor();
                let c0 = cbin.floor();
                let o0 = obin.floor();
                let rb = rbin - r0;
                let cb = cbin - c0;
                let ob = obin - o0;
                let mut o0i = o0 as i32;
                if o0i < 0 {
                    o0i += N as i32;
                }
                if o0i >= N as i32 {
                    o0i -= N as i32;
                }
                let r0i = r0 as i32;
                let c0i = c0 as i32;
                // trilinear
                let v_r1 = mag * rb;
                let v_r0 = mag - v_r1;
                let v_rc11 = v_r1 * cb;
                let v_rc10 = v_r1 - v_rc11;
                let v_rc01 = v_r0 * cb;
                let v_rc00 = v_r0 - v_rc01;
                let v_rco111 = v_rc11 * ob;
                let v_rco110 = v_rc11 - v_rco111;
                let v_rco101 = v_rc10 * ob;
                let v_rco100 = v_rc10 - v_rco101;
                let v_rco011 = v_rc01 * ob;
                let v_rco010 = v_rc01 - v_rco011;
                let v_rco001 = v_rc00 * ob;
                let v_rco000 = v_rc00 - v_rco001;
                let idx = ((r0i + 1) * (D as i32 + 2) + c0i + 1) * (N as i32 + 2) + o0i;
                let idx = idx as usize;
                let stride_c = N + 2;
                let stride_r = (D + 2) * (N + 2);
                hist[idx] += v_rco000;
                hist[idx + 1] += v_rco001;
                hist[idx + stride_c] += v_rco010;
                hist[idx + stride_c + 1] += v_rco011;
                hist[idx + stride_r] += v_rco100;
                hist[idx + stride_r + 1] += v_rco101;
                hist[idx + stride_r + stride_c] += v_rco110;
                hist[idx + stride_r + stride_c + 1] += v_rco111;
            }
        }
    }
    // finalize: fold wrapped orientation bins, gather d*d*n
    let mut out = [0f32; DESC_LEN];
    for i in 0..D {
        for j in 0..D {
            let idx = ((i + 1) * (D + 2) + (j + 1)) * (N + 2);
            hist[idx] += hist[idx + N];
            hist[idx + 1] += hist[idx + N + 1];
            for k2 in 0..N {
                out[(i * D + j) * N + k2] = hist[idx + k2];
            }
        }
    }
    let nrm2: f32 = out.iter().map(|v| v * v).sum();
    let thr = nrm2.sqrt() * DESCR_MAG_THR;
    let mut nrm2b = 0.0f32;
    for v in out.iter_mut() {
        if *v > thr {
            *v = thr;
        }
        nrm2b += *v * *v;
    }
    let scale = INT_DESCR_FCTR / nrm2b.sqrt().max(1e-12);
    for (i, v) in out.iter().enumerate() {
        dst[i] = (v * scale).round().clamp(0.0, 255.0) as u8;
    }
}

/// Keep the strongest `n` keypoints (by DoG contrast), dropping exact
/// duplicates. Order is deterministic: response desc, then position.
fn retain_best(f: &mut Features, n: usize) {
    let mut idx: Vec<usize> = (0..f.kps.len()).collect();
    idx.sort_by(|&a, &b| {
        let (ka, kb) = (&f.kps[a], &f.kps[b]);
        kb.response
            .partial_cmp(&ka.response)
            .unwrap()
            .then(ka.x.partial_cmp(&kb.x).unwrap())
            .then(ka.y.partial_cmp(&kb.y).unwrap())
            .then(ka.sigma.partial_cmp(&kb.sigma).unwrap())
            .then(ka.angle.partial_cmp(&kb.angle).unwrap())
    });
    let mut kps = Vec::with_capacity(n.min(idx.len()));
    let mut desc = Vec::with_capacity(n.min(idx.len()) * DESC_LEN);
    let mut last: Option<(i32, i32, i32, i32)> = None;
    for &i in &idx {
        let k = f.kps[i];
        let key = ((k.x * 4.0) as i32, (k.y * 4.0) as i32, (k.sigma * 16.0) as i32, (k.angle * 2.0) as i32);
        if last == Some(key) {
            continue;
        }
        last = Some(key);
        kps.push(k);
        desc.extend_from_slice(f.d(i));
        if kps.len() >= n {
            break;
        }
    }
    f.kps = kps;
    f.desc = desc;
}

// ---------------------------------------------------------------- variants

/// Permutation such that `desc_of_mirrored_image[i] == desc[MIRROR_PERM[i]]`
/// for the mirrored keypoint. Rows (the axis perpendicular to the keypoint
/// orientation) flip and orientation bins reverse.
pub fn mirror_perm() -> [u8; DESC_LEN] {
    let mut p = [0u8; DESC_LEN];
    for r in 0..D {
        for c in 0..D {
            for o in 0..N {
                let src = ((D - 1 - r) * D + c) * N + ((N - o) % N);
                p[(r * D + c) * N + o] = src as u8;
            }
        }
    }
    p
}

/// Inversion negates every gradient: orientation bins unchanged relative to
/// the (also rotated) dominant orientation, spatial grid rotated 180 degrees.
pub fn invert_perm() -> [u8; DESC_LEN] {
    let mut p = [0u8; DESC_LEN];
    for r in 0..D {
        for c in 0..D {
            for o in 0..N {
                let src = ((D - 1 - r) * D + (D - 1 - c)) * N + o;
                p[(r * D + c) * N + o] = src as u8;
            }
        }
    }
    p
}

pub fn permute(desc: &[u8], perm: &[u8; DESC_LEN], out: &mut [u8]) {
    for i in 0..DESC_LEN {
        out[i] = desc[perm[i] as usize];
    }
}
