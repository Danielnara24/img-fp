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
use crate::timed;

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

impl ExpTable {
    /// The sampled `exp(-t)`, for a `t` the caller knows to be non-negative.
    ///
    /// The index is the same one the lookup always computed. What is gone is
    /// the saturating float-to-integer conversion Rust puts behind `as usize`
    /// — a compare and a conditional move — and the slice bounds check, both
    /// of which are answered by the range test that is already here. Every
    /// Gaussian weight in the extractor comes through this, once per sample of
    /// every orientation histogram and every descriptor.
    #[inline(always)]
    fn at(&self, t: f32) -> f32 {
        if t >= EXP_RANGE {
            return 0.0;
        }
        let f = t * (EXP_N as f32 / EXP_RANGE);
        // `t` is in [0, EXP_RANGE) and finite, so `f` is in [0, EXP_N).
        let i = unsafe { f.to_int_unchecked::<u32>() as usize };
        debug_assert!(i < EXP_N);
        unsafe { *self.0.get_unchecked(i) }
    }
}

/// OpenCV's fastAtan2: degrees in 0..360, max error ~0.3 degrees.
#[inline]
pub fn fast_atan2_deg(y: f32, x: f32) -> f32 {
    const P1: f32 = 0.999_787_8 * (180.0 / std::f32::consts::PI);
    const P3: f32 = -0.325_808_4 * (180.0 / std::f32::consts::PI);
    const P5: f32 = 0.155_578_65 * (180.0 / std::f32::consts::PI);
    const P7: f32 = -0.044_326_555 * (180.0 / std::f32::consts::PI);
    // Written as selects over one polynomial rather than as two branches over
    // two. Both arms always divided the smaller magnitude by the larger and
    // evaluated the same series on it; saying so directly lets the compiler
    // run a whole row of gradients at once, where a branch on every pixel
    // stopped it. The arithmetic is unchanged, term for term.
    let ax = x.abs();
    let ay = y.abs();
    let steep = ax < ay;
    let num = if steep { ax } else { ay };
    let den = if steep { ay } else { ax };
    let c = num / (den + f32::EPSILON);
    let c2 = c * c;
    let a = (((P7 * c2 + P5) * c2 + P3) * c2 + P1) * c;
    let a = if steep { 90.0 - a } else { a };
    let a = if x < 0.0 { 180.0 - a } else { a };
    if y < 0.0 { 360.0 - a } else { a }
}

/// Gradient magnitude and orientation (degrees) of a layer, computed once and
/// shared by every keypoint that lands on it.
///
/// The two are interleaved, a pair per pixel, because every reader wants both
/// halves of the same pixel: the orientation histogram and the descriptor each
/// walk a run of pixels taking `[mag, ori]` from each. Two parallel planes made
/// that two streams a page apart — twice the cache lines and twice the prefetch
/// streams for data that is never used singly.
struct Grad {
    w: usize,
    /// `[magnitude, orientation]` per pixel, row-major.
    px: Vec<[f32; 2]>,
}

impl Grad {
    fn of(l: &Layer) -> Grad {
        let (w, h) = (l.w, l.h);
        let mut px = vec![[0.0f32; 2]; w * h];
        for y in 1..h.saturating_sub(1) {
            let up = &l.px[(y - 1) * w..y * w];
            let row = &l.px[y * w..(y + 1) * w];
            let dn = &l.px[(y + 1) * w..(y + 2) * w];
            let out = &mut px[y * w..(y + 1) * w];
            for x in 1..w - 1 {
                let dx = row[x + 1] - row[x - 1];
                let dy = up[x] - dn[x];
                out[x] = [(dx * dx + dy * dy).sqrt(), fast_atan2_deg(dy, dx)];
            }
        }
        Grad { w, px }
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

/// Per-thread working buffers for the separable blur.
///
/// All three are pure scratch: every element is written before it is read, so
/// they are reused across calls and across images rather than allocated and
/// zeroed each time. A blur on a 640x480 layer allocated 2.4 MB, and the
/// pyramid runs twenty of them per image; the zeroing alone was tens of
/// gigabytes of memory traffic over a corpus, all of it overwritten
/// immediately.
struct BlurScratch {
    /// The last `2 * radius + 1` horizontally filtered rows, by row modulo
    /// that count. See `blur_into`.
    ring: Vec<f32>,
    padded: Vec<f32>,
    row: Vec<f32>,
}

thread_local! {
    static BLUR_SCRATCH: std::cell::RefCell<BlurScratch> =
        const { std::cell::RefCell::new(BlurScratch { ring: Vec::new(), padded: Vec::new(), row: Vec::new() }) };
}

/// Drop this thread's blur scratch. Called once the analysis phase is over,
/// since nothing after it extracts features.
pub fn release_scratch() {
    let _ = BLUR_SCRATCH.try_with(|s| {
        let mut s = s.borrow_mut();
        s.ring = Vec::new();
        s.padded = Vec::new();
        s.row = Vec::new();
    });
}

/// Separable Gaussian blur with reflect-101 borders.
fn blur(src: &Layer, sigma: f32) -> Layer {
    blur_plane(src.w, src.h, &src.px, sigma)
}

/// The same, over a plane that is not a `Layer` — the working image itself,
/// which the pyramid's base is a blur of.
fn blur_plane(w: usize, h: usize, px: &[f32], sigma: f32) -> Layer {
    BLUR_SCRATCH.with(|s| blur_into(w, h, px, sigma, &mut s.borrow_mut(), false).0)
}

/// Blur, and the difference-of-Gaussians it forms with the layer it blurred.
///
/// The difference used to be a pass of its own: read the two Gaussian layers,
/// write a third. But the blur's own last act is to hold a finished output row
/// in registers, and the row it was made from was read a few rows ago and is
/// still in cache — so the subtraction costs one store and the two reads it
/// used to make are gone. Same two floats, same subtraction, same order.
fn blur_dog(src: &Layer, sigma: f32) -> (Layer, Layer) {
    let (g, d) = BLUR_SCRATCH.with(|s| blur_into(src.w, src.h, &src.px, sigma, &mut s.borrow_mut(), true));
    // The `true` above is what makes the difference exist.
    (g, d.unwrap())
}

/// One row of the horizontal pass: symmetric kernel, eight outputs at a time
/// so the tap loop stays in registers.
#[inline]
fn blur_row(row: &[f32], padded: &mut [f32], out: &mut [f32], kc: f32, ks: &[f32], r: usize, w: usize) {
    for i in 0..r {
        padded[i] = row[reflect101(i as i32 - r as i32, w)];
        padded[w + r + i] = row[reflect101((w + i) as i32, w)];
    }
    padded[r..r + w].copy_from_slice(row);
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

fn blur_into(w: usize, h: usize, src: &[f32], sigma: f32, s: &mut BlurScratch, want_dog: bool) -> (Layer, Option<Layer>) {
    let k = gaussian_kernel(sigma);
    let r = k.len() / 2;
    let kc = k[r];
    let ks: &[f32] = &k[r + 1..];
    // The two passes are interleaved through a ring of the last `2r + 1`
    // filtered rows, rather than run one after the other through a whole
    // intermediate plane.
    //
    // The vertical pass of row `y` reads filtered rows `y-r ..= y+r` and
    // nothing else — reflection at the edges maps a tap back inside that
    // window, never outside it — so those rows are all that ever needs to
    // exist. A plane held them instead: a megabyte and a half written out to
    // memory and read back for every blur, twenty times an image, when
    // fifty kilobytes would stay in cache. It is the same arithmetic on the
    // same values in the same order; only the storage between the passes is
    // gone.
    //
    // Rows live at `row % ring_rows`, and the window is exactly `ring_rows`
    // wide, so a row is overwritten only once it can no longer be read.
    let ring_rows = (2 * r + 1).min(h);
    if s.ring.len() < ring_rows * w {
        s.ring.resize(ring_rows * w, 0.0);
    }
    if s.padded.len() < w + 2 * r {
        s.padded.resize(w + 2 * r, 0.0);
    }
    if s.row.len() < w {
        s.row.resize(w, 0.0);
    }
    let ring = &mut s.ring[..ring_rows * w];
    let padded = &mut s.padded[..w + 2 * r];
    let acc = &mut s.row[..w];
    let mut dst: Vec<f32> = Vec::with_capacity(w * h);
    let mut dog: Vec<f32> = Vec::with_capacity(if want_dog { w * h } else { 0 });
    let mut filtered = 0usize; // rows of `src` already through the horizontal pass
    for y in 0..h {
        let want = (y + r).min(h - 1);
        while filtered <= want {
            let slot = filtered % ring_rows;
            blur_row(
                &src[filtered * w..(filtered + 1) * w],
                padded,
                &mut ring[slot * w..(slot + 1) * w],
                kc,
                ks,
                r,
                w,
            );
            filtered += 1;
        }
        let c = &ring[(y % ring_rows) * w..(y % ring_rows + 1) * w];
        for x in 0..w {
            acc[x] = c[x] * kc;
        }
        for (t, &kv) in ks.iter().enumerate() {
            let ya = reflect101(y as i32 - t as i32 - 1, h) % ring_rows;
            let yb = reflect101(y as i32 + t as i32 + 1, h) % ring_rows;
            let a = &ring[ya * w..(ya + 1) * w];
            let b = &ring[yb * w..(yb + 1) * w];
            for x in 0..w {
                acc[x] += (a[x] + b[x]) * kv;
            }
        }
        if want_dog {
            let below = &src[y * w..(y + 1) * w];
            dog.extend(acc.iter().zip(below).map(|(a, b)| a - b));
        }
        dst.extend_from_slice(acc);
    }
    let g = Layer { w, h, px: dst };
    let d = want_dog.then(|| Layer { w, h, px: dog });
    (g, d)
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
    let mut px: Vec<f32> = Vec::with_capacity(w * h);
    for y in 0..h {
        let sy = (y as f32 + 0.5) / ff - 0.5;
        let y0 = sy.floor().max(0.0) as usize;
        let y1 = (y0 + 1).min(g.h - 1);
        let fy = (sy - y0 as f32).clamp(0.0, 1.0);
        px.extend((0..w).map(|x| {
            let sx = (x as f32 + 0.5) / ff - 0.5;
            let x0 = sx.floor().max(0.0) as usize;
            let x1 = (x0 + 1).min(g.w - 1);
            let fx = (sx - x0 as f32).clamp(0.0, 1.0);
            let a = g.px[y0 * g.w + x0] * (1.0 - fx) + g.px[y0 * g.w + x1] * fx;
            let b = g.px[y1 * g.w + x0] * (1.0 - fx) + g.px[y1 * g.w + x1] * fx;
            a * (1.0 - fy) + b * fy
        }));
    }
    Layer { w, h, px }
}

fn halve(src: &Layer) -> Layer {
    let (w, h) = ((src.w / 2).max(1), (src.h / 2).max(1));
    let mut px: Vec<f32> = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = &src.px[(y * 2) * src.w..];
        px.extend((0..w).map(|x| row[x * 2]));
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
    // The base of the pyramid is the working image blurred up to `sigma`, or
    // an enlargement of it when the picture is small. Where no enlargement is
    // wanted the blur reads the working image where it lies: copying it first
    // held a second megabyte per worker and moved it for nothing, since the
    // blur's output is a new plane either way.
    let (init_sigma, coord_scale) = if factor > 1 { (0.5 * factor as f32, 1.0 / factor as f32) } else { (0.5f32, 1.0f32) };
    let sig_diff = (p.sigma * p.sigma - init_sigma * init_sigma).max(0.01).sqrt();
    let base = timed!(5, {
        if factor > 1 {
            blur(&upsample(g, factor), sig_diff)
        } else {
            blur_plane(g.w, g.h, &g.px, sig_diff)
        }
    });

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
        // A Gaussian layer is kept only as long as something still reads it.
        //
        // Three of the `s + 3` are dead the moment the differences are taken:
        // layer 0 is the octave's own base, layers `s+1` and `s+2` exist only
        // to make the top two differences, and none of the three carries a
        // gradient. Holding all of them alongside all `s + 2` differences was
        // eleven full-size planes per worker at the widest point of the
        // pyramid, on eight workers at once, for three planes nothing would
        // read again. `None` in their place says so.
        let height = octave_base.h;
        let mut gauss: Vec<Option<Layer>> = Vec::with_capacity(s + 3);
        gauss.push(Some(std::mem::replace(&mut octave_base, Layer { w: 0, h: 0, px: vec![] })));
        let mut dog: Vec<Layer> = Vec::with_capacity(s + 2);
        for i in 1..s + 3 {
            let (l, d) = timed!(6, blur_dog(gauss[i - 1].as_ref().unwrap(), sig[i]));
            gauss.push(Some(l));
            dog.push(d);
            // `gauss[i - 1]` has produced its difference. It is read again
            // only if a gradient is taken from it, or if the next octave
            // starts from it.
            if !(1..=s).contains(&(i - 1)) {
                gauss[i - 1] = None;
            }
        }
        // The top layer made the last difference and carries no gradient.
        gauss[s + 2] = None;
        timed!(7, find_extrema(&dog, o, p, thr_pre, coord_scale, &mut cands));
        // The differences have said all they have to say; the gradients below
        // need only the Gaussians, and this is the largest thing a worker
        // holds after the decode.
        drop(dog);
        heights.push(height);
        if o + 1 < n_octaves {
            octave_base = halve(gauss[s].as_ref().unwrap());
        }
        grads.push(timed!(8,
            (0..s + 3)
                .map(|i| gauss[i].take().filter(|_| (1..=s).contains(&i)).map(|l| Grad::of(&l)))
                .collect::<Vec<_>>()
        ));
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

    // Describing stops as soon as no remaining candidate can reach the kept
    // set. The candidates are in response order, every descriptor inherits its
    // candidate's response, and `retain_best` keeps the `max_features` highest
    // responses — so once that many exist and the next candidate is weaker
    // than the weakest of them, nothing later can displace one. The margin
    // covers the handful `retain_best` may drop as exact duplicates.
    //
    // This is what makes `candidate_pool` as cheap as it claims to be. The
    // pool widens what may be *chosen*; describing is the expensive half, and
    // a textured image was describing three keypoints for every one it kept.
    const KEEP_MARGIN: usize = 8;
    let stop_at = p.max_features + KEEP_MARGIN;
    timed!(9, {
        for c in cands.iter() {
            if feats.kps.len() >= stop_at && c.kp.response < feats.kps[stop_at - 1].response {
                break;
            }
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
    });
    timed!(10, retain_best(&mut feats, p.max_features));
    feats
}

/// Larger of two, written as the comparison it is so that the compiler emits
/// one instruction. `f32::max` carries NaN rules the scale space cannot
/// produce and pays for them in every lane.
#[inline(always)]
fn fmax(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

#[inline(always)]
fn fmin(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// The three rows of a layer centred on `y`.
#[inline]
fn rows3(l: &Layer, y: usize, w: usize) -> (&[f32], &[f32], &[f32]) {
    (&l.px[(y - 1) * w..y * w], &l.px[y * w..(y + 1) * w], &l.px[(y + 1) * w..(y + 2) * w])
}

/// The neighbours on the two adjacent scales: the half of the 3x3x3
/// neighbourhood the row sweep has not already ruled on.
#[inline]
fn is_extreme(v: f32, x: usize, rows: [&[f32]; 6], max: bool) -> bool {
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
    let wu = w as usize;
    let (lo, hi) = (IMG_BORDER as usize, (w - IMG_BORDER) as usize);
    let span = hi - lo;
    // Which pixels of the row are still in the running, decided without a
    // branch. Almost two thirds of a layer clears the contrast threshold and
    // barely a tenth of that survives its own row, so the test that used to
    // stand at the top of the sweep was a coin-toss branch taken once per
    // pixel of the pyramid — hundreds of millions of mispredictions over a
    // corpus. Settling the whole row of nine comparisons as arithmetic and
    // then walking the survivors leaves one branch that is almost always not
    // taken, and the comparisons themselves run eight to an instruction.
    let mut alive = vec![false; span];
    for layer in 1..=s {
        let cur = &dog[layer];
        let prv = &dog[layer - 1];
        let nxt = &dog[layer + 1];
        for y in IMG_BORDER..h - IMG_BORDER {
            let yu = y as usize;
            let (c0, c1, c2) = rows3(cur, yu, wu);
            let (p0, p1, p2) = rows3(prv, yu, wu);
            let (n0, n1, n2) = rows3(nxt, yu, wu);
            // Equal-length windows of the three rows of this scale, so the
            // sweep below indexes nothing it has to check.
            let (vc, cl, cr) = (&c1[lo..hi], &c1[lo - 1..hi - 1], &c1[lo + 1..hi + 1]);
            let (ul, um, ur) = (&c0[lo - 1..hi - 1], &c0[lo..hi], &c0[lo + 1..hi + 1]);
            let (dl, dm, dr) = (&c2[lo - 1..hi - 1], &c2[lo..hi], &c2[lo + 1..hi + 1]);
            for i in 0..span {
                let v = vc[i];
                // "At least as large as all eight neighbours" is "at least as
                // large as the largest of them", and the largest of eight is
                // seven comparisons where eight separate tests and their seven
                // conjunctions are fifteen. Same answer for every input the
                // pyramid can hold — a difference of two finite blurs is
                // finite, so there is no NaN for the two forms to disagree
                // about — over half the instructions.
                let biggest = fmax(
                    fmax(fmax(cl[i], cr[i]), fmax(ul[i], um[i])),
                    fmax(fmax(ur[i], dl[i]), fmax(dm[i], dr[i])),
                );
                let smallest = fmin(
                    fmin(fmin(cl[i], cr[i]), fmin(ul[i], um[i])),
                    fmin(fmin(ur[i], dl[i]), fmin(dm[i], dr[i])),
                );
                alive[i] = ((v > thr_pre) & (v >= biggest)) | ((v < -thr_pre) & (v <= smallest));
            }
            for i in 0..span {
                if !alive[i] {
                    continue;
                }
                let xu = lo + i;
                let v = c1[xu];
                let positive = v > 0.0;
                // This scale's own 3x3 is settled; the two neighbouring
                // scales are not.
                if !is_extreme(v, xu, [p0, p1, p2, n0, n1, n2], positive) {
                    continue;
                }
                if let Some((kp, lay)) = adjust(dog, octave, layer, xu as i32, y, p, oct_scale) {
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
    // The weight table is resolved once rather than on every sample: it is a
    // lazily initialised static, and this loop runs a few hundred times for
    // each of a corpus's millions of candidate keypoints.
    let tbl = &*EXP_TABLE;
    for i in -radius..=radius {
        let y = cy + i;
        if y <= 0 || y >= h - 1 {
            continue;
        }
        // The same bound on x, hoisted out of the row: 1 <= x < w - 1.
        let j0 = (-radius).max(1 - cx);
        let j1 = radius.min(w - 2 - cx);
        // The row's useful span, taken once: the bounds were settled above, so
        // the samples come out of a slice rather than out of an index whose
        // range has to be re-proved on every one of them.
        if j1 < j0 {
            continue;
        }
        let row = y as usize * g.w;
        let span = &g.px[row + (cx + j0) as usize..row + (cx + j1) as usize + 1];
        for (n, &[mag, ori]) in span.iter().enumerate() {
            let j = j0 + n as i32;
            let t = (i * i + j * j) as f32 * neg_scale;
            let wgt = tbl.at(t);
            // `ori` came out of `fast_atan2_deg` in 0..=360, so the rounded
            // bin is in 0..=ORI_BINS: the conversion cannot saturate, the
            // wrap can only ever fire at the top end, and the result indexes
            // the histogram. The test for a negative bin that used to stand
            // here could not fire at all.
            let mut bin = unsafe { (ori * ORI_BINS as f32 / 360.0).round().to_int_unchecked::<u32>() as usize };
            if bin >= ORI_BINS {
                bin -= ORI_BINS;
            }
            debug_assert!(bin < ORI_BINS);
            unsafe { *temphist.get_unchecked_mut(bin) += wgt * mag };
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

/// The `j` interval where `a * j` lies between `l` and `u`, unordered ends
/// sorted. A zero coefficient yields infinities, which the caller's `max`/`min`
/// turn into "no constraint" or "no solutions" correctly; a 0/0 yields NaN,
/// which `max`/`min` drop, leaving the constraint to the per-sample test.
#[inline]
fn j_span(a: f32, l: f32, u: f32) -> (f32, f32) {
    let (p, q) = (l / a, u / a);
    if p <= q { (p, q) } else { (q, p) }
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

    let tbl = &*EXP_TABLE;
    // How many of a row's samples are worked out before any of them is added
    // in. Sixteen is two vectors' worth on any machine this runs on and a
    // quarter of a kilobyte of stack: wide enough that the first sweep is
    // worth vectorising, narrow enough to stay in the first-level cache.
    const SWEEP: usize = 16;
    let mut sw_mag = [0f32; SWEEP];
    let mut sw_ori = [0f32; SWEEP];
    let mut sw_rb = [0f32; SWEEP];
    let mut sw_cb = [0f32; SWEEP];
    let mut sw_in = [false; SWEEP];
    for i in -radius..=radius {
        let r = pt_y + i;
        if r <= 0 || r >= rows - 1 {
            continue;
        }
        // The search square has side 2*radius = 7.07 hist_widths; the rotated
        // square that can actually land in the descriptor grid has side 4. Two
        // thirds of the iterations below therefore cannot pass the test, and
        // the test is the only thing that was rejecting them. The same
        // inequalities solved for `j` give the row's useful span instead:
        // cbin in (-1, 4) is j*cos_t in (-2.5 + i*sin_t, 2.5 + i*sin_t), and
        // rbin likewise against sin_t. The span is widened by a pixel at each
        // end and every sample still faces the original test, so the set of
        // contributing samples — and each sum built from it, in order — is
        // exactly what the full sweep produced.
        let fi = i as f32;
        let (p1, q1) = j_span(cos_t, -2.5 + fi * sin_t, 2.5 + fi * sin_t);
        let (p2, q2) = j_span(sin_t, -2.5 - fi * cos_t, 2.5 - fi * cos_t);
        let lo = p1.max(p2).max(-radius as f32);
        let hi = q1.min(q2).min(radius as f32);
        if !(hi >= lo) {
            continue;
        }
        let j0 = (lo.floor() as i32 - 1).max(-radius).max(1 - pt_x);
        let j1 = (hi.ceil() as i32 + 1).min(radius).min(cols - 2 - pt_x);
        // `j0`/`j1` already hold the sample inside the image, and `r` was
        // checked above, so the row's pixels are exactly the ones the original
        // bounds test admitted.
        if j1 < j0 {
            continue;
        }
        let row = r as usize * g.w;
        let span = &g.px[row + (pt_x + j0) as usize..row + (pt_x + j1) as usize + 1];
        // The row is swept twice.
        //
        // The first sweep works out what each of its samples contributes:
        // where the sample falls in the descriptor's grid, and how much of the
        // gradient it carries there. That is the same short chain of multiplies
        // for every sample and nothing a compiler cannot run eight at a time.
        // The second adds those contributions into the histogram, which is a
        // scatter and has to go one sample after another. Interleaved, the
        // scatter was holding the arithmetic to one sample at a time too.
        //
        // The column counter is carried as a float. It only ever grows by one
        // and stays inside the search radius, so each value is exactly the
        // integer it stands for — and the integer-to-float conversion that
        // opened this loop, once for every sample of every descriptor in the
        // corpus, is gone.
        let mut jf = j0 as f32;
        for block in span.chunks(SWEEP) {
            for (u, &[m, o]) in block.iter().enumerate() {
                let jj = jf + u as f32;
                let c_rot = jj * cos_t - fi * sin_t;
                let r_rot = jj * sin_t + fi * cos_t;
                let rbin = r_rot + (D / 2) as f32 - 0.5;
                let cbin = c_rot + (D / 2) as f32 - 0.5;
                let t = (c_rot * c_rot + r_rot * r_rot) * neg_exp_scale;
                // The weight is taken for every sample, inside the grid or
                // not: `at` is defined for any non-negative `t` and a sample
                // that falls outside is thrown away below, unweighed.
                sw_mag[u] = m * tbl.at(t);
                sw_ori[u] = (o - ori) * bins_per_rad;
                sw_rb[u] = rbin;
                sw_cb[u] = cbin;
                sw_in[u] = rbin > -1.0 && rbin < D as f32 && cbin > -1.0 && cbin < D as f32;
            }
            jf += block.len() as f32;
            for u in 0..block.len() {
                if sw_in[u] {
                    let (rbin, cbin) = (sw_rb[u], sw_cb[u]);
                    let mag = sw_mag[u];
                    let obin = sw_ori[u];
                    let r0 = rbin.floor();
                let c0 = cbin.floor();
                let o0 = obin.floor();
                let rb = rbin - r0;
                let cb = cbin - c0;
                let ob = obin - o0;
                // `rbin` and `cbin` are inside (-1, D) — the test above says
                // so — and `obin` is inside (-N, N], since it is an angle
                // difference scaled into bins. So all three floors are small
                // integers and the conversions cannot saturate; saying so
                // replaces five instructions apiece with one, three times for
                // every sample of every descriptor.
                let (r0i, c0i, o0i) = unsafe {
                    (
                        r0.to_int_unchecked::<i32>(),
                        c0.to_int_unchecked::<i32>(),
                        o0.to_int_unchecked::<i32>(),
                    )
                };
                // Folding the orientation bin back into 0..N. It is in
                // -N..=N, where masking off the low bits is the same two
                // adjustments the two branches made — and N is eight.
                debug_assert!((-(N as i32)..=N as i32).contains(&o0i));
                let o0i = o0i & (N as i32 - 1);
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
                // The eight corners of one sample's trilinear spread, written
                // without eight bounds checks. `rbin` and `cbin` are inside
                // (-1, D) — the test above says so — and `o0i` has just been
                // folded into 0..N, so the largest index touched is
                // (D * (D + 2) + D) * (N + 2) + (N - 1) + stride_r + stride_c
                // + 1, which is 358 of the 360 bins. This is the innermost
                // loop of the whole extractor: it runs some hundreds of times
                // for every descriptor of every image.
                debug_assert!(idx + stride_r + stride_c + 1 < hist.len());
                unsafe {
                    let h = hist.as_mut_ptr().add(idx);
                    *h += v_rco000;
                    *h.add(1) += v_rco001;
                    *h.add(stride_c) += v_rco010;
                    *h.add(stride_c + 1) += v_rco011;
                    *h.add(stride_r) += v_rco100;
                    *h.add(stride_r + 1) += v_rco101;
                    *h.add(stride_r + stride_c) += v_rco110;
                    *h.add(stride_r + stride_c + 1) += v_rco111;
                }
                }
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
// ---------------------------------------------------------------- kernel timings

/// Single-threaded timings of the extractor's inner loops, for comparing two
/// builds without the thermal and power-cap noise a whole-corpus run carries.
/// `cargo test --release -- --ignored --nocapture kernel_timings`
#[cfg(test)]
mod bench {
    use super::*;

    fn synthetic(w: usize, h: usize) -> Layer {
        let mut px = vec![0f32; w * h];
        let mut s = 0x1234_5678u32;
        for v in px.iter_mut() {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            *v = ((s >> 16) & 0xff) as f32 / 255.0;
        }
        // Some structure, so the extremum sweep and the descriptor behave.
        for y in 0..h {
            for x in 0..w {
                px[y * w + x] = px[y * w + x] * 0.3
                    + (((x / 17 + y / 13) % 5) as f32) * 0.15
                    + ((x as f32 * 0.05).sin() * (y as f32 * 0.03).cos()) * 0.2;
            }
        }
        Layer { w, h, px }
    }

    /// The fastest of several runs: the slow ones are the machine's, not the
    /// code's.
    fn ms(f: impl Fn()) -> f64 {
        let mut best = f64::MAX;
        for _ in 0..9 {
            let t = std::time::Instant::now();
            f();
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        best
    }

    #[test]
    #[ignore]
    fn kernel_timings() {
        let base = synthetic(640, 480);
        for sigma in [1.226f32, 1.545, 1.946, 2.452, 3.089] {
            let t = ms(|| {
                std::hint::black_box(blur_dog(&base, sigma));
            });
            println!("blur_dog sigma {sigma:.3} (r={}): {t:8.3} ms", gaussian_kernel(sigma).len() / 2);
        }
        let g = crate::decode::Gray { w: 640, h: 480, px: base.px.clone() };
        let p = Params::default();
        let t = ms(|| {
            std::hint::black_box(extract(&g, &p));
        });
        println!("extract 640x480: {t:8.3} ms ({} features)", extract(&g, &p).len());

        // Describe alone: one octave's gradients, every candidate described.
        let gauss = blur(&base, 1.0);
        let grad = Grad::of(&gauss);
        let t = ms(|| {
            let mut d = [0u8; DESC_LEN];
            let mut acc = 0f32;
            for i in 0..2000 {
                let x = 60.0 + ((i * 37) % 500) as f32;
                let y = 60.0 + ((i * 53) % 340) as f32;
                descriptor(&grad, 480, x, y, (i % 360) as f32, 1.6 + (i % 5) as f32 * 0.3, &mut d);
                acc += d[0] as f32;
            }
            std::hint::black_box(acc);
        });
        println!("2000 descriptors: {t:8.3} ms");
        let t = ms(|| {
            let mut hist = [0f32; ORI_BINS];
            let mut acc = 0f32;
            for i in 0..2000 {
                let x = 60.0 + ((i * 37) % 500) as f32;
                let y = 60.0 + ((i * 53) % 340) as f32;
                let scl = 1.6 + (i % 5) as f32 * 0.3;
                acc += orientation_hist(&grad, 480, x, y, (ORI_RADIUS * scl).round() as i32, ORI_SIG_FCTR * scl, &mut hist);
            }
            std::hint::black_box(acc);
        });
        println!("2000 orientation hists: {t:8.3} ms");
        let t = ms(|| {
            std::hint::black_box(Grad::of(&gauss));
        });
        println!("Grad::of 640x480: {t:8.3} ms");
    }
}
