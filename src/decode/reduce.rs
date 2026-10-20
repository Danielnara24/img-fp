//! A picture's rows to the working plane: grey, box-reduced and then
//! area-resampled to the working size as they arrive (`Reducer`, `Fit`), and
//! measured on the way (`Measure`); and the area resampling itself.

use super::measure::{Measure, Traits};
use super::picture::{orient, shown_size, Layout, Orientation, Rows, Sink};
use super::Gray;
use crate::timed;

/// The analysis's sink: a picture's rows reduced to the working plane as
/// they come, whatever decoded them, and measured on the way.
pub(super) struct Reduce {
    work: usize,
    rows: Option<(Rows, Box<dyn Reduction>)>,
}

/// A `Reducer` of one layout. The channel count is fixed for the whole
/// picture, but inside the loop it was a runtime stride and a runtime
/// divisor, which is enough to stop the loop vectorising over a hundred
/// megapixels a minute; so each layout gets its own copy of the arithmetic,
/// and only the call for each row is made through this.
trait Reduction {
    fn rows_read(&self) -> usize;
    fn push(&mut self, line: &[u8]);
    fn finish(self: Box<Self>) -> (Gray, Traits);
}

impl<const CH: usize, const ALPHA: bool> Reduction for Reducer<CH, ALPHA> {
    fn rows_read(&self) -> usize {
        Reducer::rows_read(self)
    }

    fn push(&mut self, line: &[u8]) {
        Reducer::push(self, line)
    }

    fn finish(self: Box<Self>) -> (Gray, Traits) {
        Reducer::finish(*self)
    }
}

impl Reduce {
    pub fn new(work: usize) -> Reduce {
        Reduce { work, rows: None }
    }

    /// The working plane, turned as the picture is shown; the picture's own
    /// size as shown; and what the rows measured.
    pub fn finish(self) -> anyhow::Result<(Gray, (u32, u32), Traits)> {
        let (rows, reduction) = self.rows.ok_or_else(|| anyhow::anyhow!("the decoder gave no picture"))?;
        let (g, traits) = reduction.finish();
        let size = shown_size(rows.w as u32, rows.h as u32, rows.orientation);
        Ok((orient(g, rows.orientation), size, traits))
    }
}

impl Sink for Reduce {
    fn streams(&self, _: usize, _: usize) -> bool {
        true
    }

    fn grey(&self) -> bool {
        true
    }

    fn holds(&self, w: usize, h: usize) -> u64 {
        working_bytes(w, h, self.work)
    }

    fn begin(&mut self, rows: &Rows) -> usize {
        let (w, h, work, grey) = (rows.w, rows.h, self.work, rows.grey_of_colour);
        let reduction: Box<dyn Reduction> = match rows.layout {
            Layout::Grey => Box::new(Reducer::<1, false>::new(w, h, work, grey)),
            Layout::GreyAlpha => Box::new(Reducer::<2, true>::new(w, h, work, grey)),
            Layout::Rgb => Box::new(Reducer::<3, false>::new(w, h, work, grey)),
            Layout::Rgba => Box::new(Reducer::<4, true>::new(w, h, work, grey)),
        };
        let n = reduction.rows_read();
        self.rows = Some((*rows, reduction));
        n
    }

    fn row(&mut self, line: &[u8]) {
        if let Some((_, reduction)) = &mut self.rows {
            reduction.push(line);
        }
    }

    fn whole(&mut self, img: image::DynamicImage, orientation: Orientation) {
        timed!(2, super::picture::rows_of(self, img, orientation))
    }
}

/// The float plane the box reduction writes while the decoder's buffer is
/// still alive, so that a claim covers what the decode path actually holds.
///
/// It is not a rounding error: a picture already near the working size is not
/// reduced at all, and its grey plane is then four bytes a pixel against the
/// decoder's three. Leaving it out, and the file's own bytes with it, made
/// every claim smaller than the memory it stood for, which is the one thing a
/// budget must not be.
pub(super) fn working_bytes(w: usize, h: usize, work: usize) -> u64 {
    let k = box_factor(w, h, work);
    let (ow, oh) = ((w / k).max(1), (h / k).max(1));
    // What `Fit` holds: the vertical pass's rows at the reduction's width and
    // the output, or the reduction's plane when there is nothing to resample.
    let f = Fit::size(ow, oh, work);
    match f {
        Some((tw, th)) => ((ow * th + tw * th) * 4) as u64,
        None => (ow * oh * 4) as u64,
    }
}

/// How much further than `--work-size` a picture's long side may go: as far
/// as gives it the pixels a 4:3 picture of the working size gets,
/// `sqrt(3/4 * long/short)`, never less than 1. The limit is on the long
/// side, so without this a full-page screenshot or a panorama is shrunk by
/// its aspect ratio as well as its size: a 1080x11520 page at 512 is 48x512,
/// and each photograph in it 48x64. A 4:3 or squarer picture is untouched,
/// and no picture costs more than a 4:3 one does. The pixel check's
/// thumbnail is scaled by the same factor (`analysis::analyse`).
pub fn area_factor(w: usize, h: usize) -> f64 {
    if w == 0 || h == 0 {
        return 1.0;
    }
    let (long, short) = (w.max(h) as f64, w.min(h) as f64);
    (0.75 * long / short).sqrt().max(1.0)
}

/// The long-side limit for a `w` by `h` picture at `--work-size work`.
fn long_limit(w: usize, h: usize, work: usize) -> usize {
    if work == 0 { 0 } else { (work as f64 * area_factor(w, h)).round() as usize }
}

/// Integer box factor that keeps the long side at or above 2x the working
/// size (or 1), so the final area resample still has enough to average.
pub(super) fn box_factor(w: usize, h: usize, work: usize) -> usize {
    let work = long_limit(w, h, work);
    let long = w.max(h);
    if work == 0 {
        return 1;
    }
    (long / (2 * work)).max(1)
}

/// BT.601 luma of one RGB pixel: the Y a JPEG stores, which is what
/// `jpeg::luma` reads straight out of the file, so a JPEG and a PNG of
/// one picture give the same grey. The weighted sum is taken in integers,
/// which is exact, and divided once.
#[inline(always)]
pub(super) fn luma(r: u8, g: u8, b: u8) -> f32 {
    (LUMA_R * r as u32 + LUMA_G * g as u32 + LUMA_B * b as u32) as f32 / 1000.0
}
const LUMA_R: u32 = 299;
const LUMA_G: u32 = 587;
const LUMA_B: u32 = 114;

/// One grey sample from one source pixel: the luma of RGB, the mean of any
/// other set of colour channels,
/// alpha flattened onto mid-grey.
#[inline(always)]
fn grey_of<const CH: usize, const ALPHA: bool>(p: &[u8]) -> f32 {
    let color_ch = if ALPHA { CH - 1 } else { CH };
    if color_ch == 3 {
        let mut g = luma(p[0], p[1], p[2]);
        if ALPHA {
            let a = p[CH - 1] as f32 / 255.0;
            g = g * a + 128.0 * (1.0 - a);
        }
        return g;
    }
    let mut v = 0u32;
    for c in 0..color_ch {
        v += p[c] as u32;
    }
    // Dividing by one is the identity and is skipped; three and anything else
    // divide.
    //
    // The division used to be a table of the 766 quotients three bytes can
    // make, on the grounds that a float divide is an order of magnitude dearer
    // than a load. That is true of *one* divide. It is the wrong trade here,
    // because the load is a gather: a table lookup per pixel is the one thing
    // in this loop the compiler cannot vectorise around, and it was holding the
    // whole reduction to a pixel at a time. Eight lanes dividing at once beat
    // eight lanes waiting on eight scattered loads, and the quotient is the
    // same float either way — the table held nothing but this division, taken
    // at compile time.
    let mut g = if color_ch == 1 { v as f32 } else { v as f32 / color_ch as f32 };
    if ALPHA {
        // The blend is taken for every pixel, opaque or not.
        //
        // An opaque pixel is the overwhelmingly common case and its blend is
        // the identity — `g * 1.0 + 128.0 * 0.0` — so it used to be skipped.
        // But skipping it means a branch per pixel, and a branch per pixel is
        // what keeps the compiler from running a whole row of them at once;
        // the eight-lane blend costs less than the eight tests it replaces
        // even when all eight are opaque. Adding a positive zero to a
        // non-negative grey is that grey, so the pixels that used to skip it
        // get the value they always got.
        let a = p[CH - 1] as f32 / 255.0;
        g = g * a + 128.0 * (1.0 - a);
    }
    g
}

/// The box reduction, fed one source row at a time and in order.
///
/// The whole-buffer path hands it the rows of a decoded picture and the
/// streaming PNG path hands it rows as the decoder unfilters them, so the two
/// are one piece of arithmetic rather than two that must be kept in step: the
/// same grey values, summed into the same boxes in the same order.
struct Reducer<const CH: usize, const ALPHA: bool> {
    w: usize,
    h: usize,
    k: usize,
    ow: usize,
    oh: usize,
    inv: f32,
    /// Where the reduced rows go: the area resample to the working size.
    fit: Fit,
    /// One source row's grey values, so that the interleaved bytes are undone
    /// once per row rather than once per box. See `grey_row`.
    grey: Vec<f32>,
    /// The output row being summed into, when there is a box to sum.
    row: Vec<f32>,
    /// The next source row `push` expects.
    sy: usize,
    /// What the rows measure on the way (`Traits`).
    measure: Measure,
}

impl<const CH: usize, const ALPHA: bool> Reducer<CH, ALPHA> {
    fn new(w: usize, h: usize, work: usize, grey_of_colour: bool) -> Self {
        let k = box_factor(w, h, work);
        let ow = (w / k).max(1);
        let oh = (h / k).max(1);
        Reducer {
            w,
            h,
            k,
            ow,
            oh,
            inv: 1.0 / (255.0 * (k * k) as f32),
            fit: Fit::new(ow, oh, work),
            grey: vec![0.0; w],
            row: if k == 1 { Vec::new() } else { vec![0.0f32; ow] },
            sy: 0,
            measure: Measure::new(grey_of_colour),
        }
    }

    /// How many source rows the reduction reads: the rest are the edge the
    /// floor division drops.
    fn rows_read(&self) -> usize {
        (self.oh * self.k).min(self.h)
    }

    fn push(&mut self, line: &[u8]) {
        let (w, k, sy) = (self.w, self.k, self.sy);
        self.sy += 1;
        if sy >= self.rows_read() {
            return;
        }
        if k == 1 {
            // No box reduction: every output pixel is one source pixel, so
            // there is nothing to accumulate and nothing to zero first. The
            // row goes straight on to the area resample, which takes it while
            // it is still in the first-level cache; the full-size plane it
            // used to be written into first is gone.
            grey_row::<CH, ALPHA>(&line[..w * CH], &mut self.grey[..w]);
            self.measure.row::<CH, ALPHA>(sy, self.h, &line[..w * CH], &self.grey[..w]);
            let inv = self.inv;
            let out = &mut self.grey[..self.ow];
            for g in out.iter_mut() {
                *g *= inv;
            }
            self.fit.push(out);
            return;
        }
        if sy % k == 0 {
            self.row.fill(0.0);
        }
        grey_row::<CH, ALPHA>(&line[..w * CH], &mut self.grey[..w]);
        self.measure.row::<CH, ALPHA>(sy, self.h, &line[..w * CH], &self.grey[..w]);
        box_row(k, &self.grey[..w], &mut self.row);
        // The last row of a box: `k` rows in, or the picture's last row when
        // the picture is shorter than one box.
        if sy % k == k - 1 || sy + 1 == self.h {
            let inv = self.inv;
            for v in self.row.iter_mut() {
                *v *= inv;
            }
            self.fit.push(&self.row);
        }
    }

    fn finish(self) -> (Gray, Traits) {
        let traits = self.measure.traits();
        (timed!(3, self.fit.finish()), traits)
    }
}

/// `fit_to`, fed the rows of the plane it resamples one at a time and in
/// order, so that the plane never has to exist: each row is added into the
/// one or two output rows its area covers as it arrives. The output is the
/// one `fit_to` gives the whole plane — the same weights, applied to the same
/// rows in the same order, the first of them written rather than added to a
/// zero — and `streamed_fit_is_fit_to` holds the two together.
pub(super) struct Fit {
    w: usize,
    h: usize,
    /// The resampled size; `fit` is false when the plane is already within
    /// the working size and is kept as it comes.
    tw: usize,
    th: usize,
    fit: bool,
    /// Every (source row, output row, weight) the vertical pass applies, in
    /// source-row order; `first` says the weight is the output row's first.
    taps: Vec<(u32, u32, f32, bool)>,
    next: usize,
    sy: usize,
    out: Vec<f32>,
}

impl Fit {
    /// The size `fit_to` would resample a `w` by `h` plane to, or `None` when
    /// it would leave it as it is.
    fn size(w: usize, h: usize, work: usize) -> Option<(usize, usize)> {
        let work = long_limit(w, h, work);
        let long = w.max(h);
        if work == 0 || long <= work {
            return None;
        }
        let s = work as f32 / long as f32;
        let (tw, th) = (((w as f32 * s).round() as usize).max(1), ((h as f32 * s).round() as usize).max(1));
        ((tw, th) != (w, h)).then_some((tw, th))
    }

    pub fn new(w: usize, h: usize, work: usize) -> Fit {
        let (fit, (tw, th)) = match Fit::size(w, h, work) {
            Some(t) => (true, t),
            None => (false, (w, h)),
        };
        let mut taps = Vec::new();
        if fit {
            let yw = weights(h, th);
            for oy in 0..th {
                let start = yw.start[oy] as usize;
                let (a, b) = (yw.at[oy] as usize, yw.at[oy + 1] as usize);
                for (i, &wgt) in yw.w[a..b].iter().enumerate() {
                    taps.push(((start + i) as u32, oy as u32, wgt, i == 0));
                }
            }
            // Stable, so an output row's taps keep their order.
            taps.sort_by_key(|t| t.0);
        }
        Fit { w, h, tw, th, fit, taps, next: 0, sy: 0, out: Vec::with_capacity(if fit { w * th } else { w * h }) }
    }

    pub fn push(&mut self, row: &[f32]) {
        let w = self.w;
        debug_assert_eq!(row.len(), w);
        let sy = self.sy as u32;
        self.sy += 1;
        if !self.fit {
            self.out.extend_from_slice(row);
            return;
        }
        while let Some(&(r, oy, wgt, first)) = self.taps.get(self.next) {
            if r != sy {
                break;
            }
            self.next += 1;
            let at = oy as usize * w;
            if first {
                debug_assert_eq!(self.out.len(), at);
                self.out.extend(row.iter().map(|v| v * wgt));
            } else {
                let dst = &mut self.out[at..at + w];
                for x in 0..w {
                    dst[x] += row[x] * wgt;
                }
            }
        }
    }

    pub fn finish(self) -> Gray {
        debug_assert_eq!(self.sy, self.h);
        if !self.fit {
            return Gray { w: self.w, h: self.h, px: self.out };
        }
        debug_assert_eq!(self.out.len(), self.w * self.th);
        horizontal_pass(&self.out, self.w, self.tw, self.th)
    }
}

/// Add one source row's boxes into the output row.
///
/// `ow * k` samples in, `ow` sums out, each over `k` neighbours taken left to
/// right. The box factor is a property of the picture, so it was a runtime
/// bound on the innermost loop of the reduction — which means the compiler
/// could neither unroll it nor see that `ox * k + k` never leaves the row, and
/// the whole thing ran one float at a time. Handing it the factor as a
/// constant lets the sums run eight output pixels at a time: for `k` of two,
/// one deinterleave and one add.
///
/// The sums are the same sums in the same order. `acc` started at zero and a
/// grey value is never negative, so `0.0 + g` is `g` and what is left is the
/// same left-to-right chain; the loop over output pixels is what widens, and
/// those are independent of each other. Only 2, 3 and 4 are spelled out —
/// above that the loop has enough work per output to unroll on its own, and
/// `box_factor` reaches 4 only past ten thousand pixels on the long side.
#[inline]
fn box_row(k: usize, grey: &[f32], row: &mut [f32]) {
    // A picture narrower than one box — a tall strip reduced by its own long
    // side — has an output pixel that runs off the end of the row. The general
    // form below clamps it; the widened ones need whole boxes.
    if row.len() * k <= grey.len() {
        match k {
            2 => return box_k::<2>(&grey[..row.len() * 2], row),
            3 => return box_k::<3>(&grey[..row.len() * 3], row),
            4 => return box_k::<4>(&grey[..row.len() * 4], row),
            _ => {}
        }
    }
    let w = grey.len();
    for (ox, r) in row.iter_mut().enumerate() {
        let mut acc = 0.0f32;
        for &g in grey[ox * k..(ox * k + k).min(w)].iter() {
            acc += g;
        }
        *r += acc;
    }
}

#[inline(always)]
fn box_k<const K: usize>(grey: &[f32], row: &mut [f32]) {
    debug_assert_eq!(grey.len(), row.len() * K);
    for (r, g) in row.iter_mut().zip(grey.chunks_exact(K)) {
        let mut acc = 0.0f32;
        for t in 0..K {
            acc += g[t];
        }
        *r += acc;
    }
}

/// One row of source pixels as grey values.
///
/// This exists because of the three-byte stride. `grey_of` is a handful of
/// integer operations and one divide, and a whole row of them is nothing a
/// compiler cannot run eight at a time — except that a pixel's three bytes sit
/// at `3x`, and a loop whose loads are that shape neither unrolls nor
/// vectorises. Measured, it held the reduction to 6.2 cycles a pixel where the
/// greyscale layout — the same loop with one load and no divide — ran at 3.6.
///
/// So the de-interleaving is done explicitly, eight pixels at a time, and what
/// comes out is bit for bit what `grey_of` gives: the channels are summed as
/// integers, which is exact, and the one division is the same IEEE division in
/// a vector lane as in a register. `specialised_reduction_matches_the_general_one`
/// holds this against the scalar reference over every layout.
#[inline]
fn grey_row<const CH: usize, const ALPHA: bool>(line: &[u8], out: &mut [f32]) {
    let n = out.len();
    debug_assert!(line.len() >= n * CH);
    // Mutated by the vector loop below, which another architecture does not
    // have.
    #[allow(unused_mut)]
    let mut x = 0usize;
    #[cfg(target_arch = "x86_64")]
    if (CH == 3 || CH == 4) && crate::simd::v3() {
        // SAFETY: the CPU has AVX2.
        x = unsafe { grey_row_avx2::<CH, ALPHA>(line, out) };
    }
    for x in x..n {
        out[x] = grey_of::<CH, ALPHA>(&line[x * CH..x * CH + CH]);
    }
}

/// The part of `grey_row` taken eight pixels at a time; the first pixel it
/// left for the scalar tail.
///
/// Eight pixels a step, and the load takes thirty-two bytes where three
/// channels need twenty-four, so the last few pixels of the row go down the
/// scalar path rather than read past the end of it. Its own function so that
/// it is compiled for AVX2 on a build that cannot assume it (see `simd.rs`),
/// and called once a row rather than once per eight pixels.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn grey_row_avx2<const CH: usize, const ALPHA: bool>(line: &[u8], out: &mut [f32]) -> usize {
    const STEP: usize = 8;
    let (n, mut x) = (out.len(), 0usize);
    while x + STEP <= n && x * CH + 32 <= line.len() {
        unsafe { grey_eight::<CH, ALPHA>(&line[x * CH..], &mut out[x..x + STEP]) };
        x += STEP;
    }
    x
}

/// Eight pixels' grey values, de-interleaved with shuffles.
///
/// `src` must carry 32 readable bytes. Three- and four-channel layouts have
/// their own path; anything else falls back to the scalar form, which is what
/// `grey_row` would have done anyway.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn grey_eight<const CH: usize, const ALPHA: bool>(src: &[u8], out: &mut [f32]) {
    use std::arch::x86_64::*;
    if CH != 3 && CH != 4 {
        for (i, o) in out.iter_mut().enumerate() {
            *o = grey_of::<CH, ALPHA>(&src[i * CH..i * CH + CH]);
        }
        return;
    }
    unsafe {
        let v = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
        // Each 32-bit lane ends up holding one pixel's channels in its low
        // bytes: for four channels that is already the layout, and for three
        // the dwords are first moved so that each 128-bit half starts on a
        // pixel boundary — bytes 0..12 in the low half, 12..24 in the high.
        let p = if CH == 3 {
            let moved = _mm256_permutevar8x32_epi32(v, _mm256_setr_epi32(0, 1, 2, 3, 3, 4, 5, 6));
            #[rustfmt::skip]
            let shuf = _mm256_setr_epi8(
                0, 1, 2, -1, 3, 4, 5, -1, 6, 7, 8, -1, 9, 10, 11, -1,
                0, 1, 2, -1, 3, 4, 5, -1, 6, 7, 8, -1, 9, 10, 11, -1,
            );
            _mm256_shuffle_epi8(moved, shuf)
        } else {
            v
        };
        let mask = _mm256_set1_epi32(0xff);
        let c0 = _mm256_and_si256(p, mask);
        let c1 = _mm256_and_si256(_mm256_srli_epi32(p, 8), mask);
        let c2 = _mm256_and_si256(_mm256_srli_epi32(p, 16), mask);
        // The colour channels, weighted and summed as integers as `grey_of`
        // does it. A fourth colour channel only exists when there is no alpha,
        // and four are averaged.
        let colour_ch = if ALPHA { CH - 1 } else { CH };
        let mut g = if colour_ch == 3 {
            let sum = _mm256_add_epi32(
                _mm256_add_epi32(
                    _mm256_mullo_epi32(c0, _mm256_set1_epi32(LUMA_R as i32)),
                    _mm256_mullo_epi32(c1, _mm256_set1_epi32(LUMA_G as i32)),
                ),
                _mm256_mullo_epi32(c2, _mm256_set1_epi32(LUMA_B as i32)),
            );
            _mm256_div_ps(_mm256_cvtepi32_ps(sum), _mm256_set1_ps(1000.0))
        } else {
            let sum = _mm256_add_epi32(_mm256_add_epi32(_mm256_add_epi32(c0, c1), c2), _mm256_srli_epi32(p, 24));
            _mm256_div_ps(_mm256_cvtepi32_ps(sum), _mm256_set1_ps(colour_ch as f32))
        };
        if ALPHA {
            let a = _mm256_div_ps(_mm256_cvtepi32_ps(_mm256_srli_epi32(p, 24)), _mm256_set1_ps(255.0));
            // `g * a + 128 * (1 - a)`, in that order and with no contraction:
            // the scalar form is two multiplications and an addition and this
            // has to be the same three roundings.
            let t = _mm256_sub_ps(_mm256_set1_ps(1.0), a);
            g = _mm256_add_ps(_mm256_mul_ps(g, a), _mm256_mul_ps(_mm256_set1_ps(128.0), t));
        }
        _mm256_storeu_ps(out.as_mut_ptr(), g);
    }
}

/// Area-resample so the long side is exactly `work` (never upscales).
#[cfg(test)]
pub(super) fn fit_to(g: Gray, work: usize) -> Gray {
    let work = long_limit(g.w, g.h, work);
    let long = g.w.max(g.h);
    if work == 0 || long <= work {
        return g;
    }
    let s = work as f32 / long as f32;
    let tw = ((g.w as f32 * s).round() as usize).max(1);
    let th = ((g.h as f32 * s).round() as usize).max(1);
    resize_area(&g, tw, th)
}

/// Separable area (box with fractional edges) resampling. Exact averaging of
/// source pixel coverage, which is what a good downscaler does and what keeps
/// a 25-pixel thumbnail comparable to a 25-pixel thumbnail made by Pillow.
///
/// The vertical pass runs first. It is the one that vectorises — an output
/// row is a weighted sum of two or three whole source rows — and running it
/// first means the horizontal pass, which cannot, sees `th` rows rather than
/// all `g.h` of them. The horizontal pass then takes every output pixel over
/// the same number of taps (`Taps::fixed`), padded with zero weights, so that
/// its inner loop has a trip count the compiler can see.
pub fn resize_area(g: &Gray, tw: usize, th: usize) -> Gray {
    if tw == g.w && th == g.h {
        return g.clone();
    }
    let yw = weights(g.h, th);
    let w = g.w;
    let mut tmp: Vec<f32> = Vec::with_capacity(w * th);
    for oy in 0..th {
        let start = yw.start[oy] as usize;
        let (a, b) = (yw.at[oy] as usize, yw.at[oy + 1] as usize);
        let base = tmp.len();
        let w0 = yw.w[a];
        let s0 = &g.px[start * w..(start + 1) * w];
        tmp.extend(s0.iter().map(|v| v * w0));
        let dst = &mut tmp[base..base + w];
        for (i, wgt) in yw.w[a + 1..b].iter().enumerate() {
            let src = &g.px[(start + i + 1) * w..(start + i + 2) * w];
            for x in 0..w {
                dst[x] += src[x] * wgt;
            }
        }
    }
    horizontal_pass(&tmp, w, tw, th)
}

/// The horizontal half of `resize_area`, over `th` rows of width `w`.
fn horizontal_pass(tmp: &[f32], w: usize, tw: usize, th: usize) -> Gray {
    let xw = weights(w, tw);
    let mut px: Vec<f32> = Vec::with_capacity(tw * th);
    match xw.max_taps() {
        1 => horizontal::<1>(tmp, w, &xw, tw, th, &mut px),
        2 => horizontal::<2>(tmp, w, &xw, tw, th, &mut px),
        3 => horizontal::<3>(tmp, w, &xw, tw, th, &mut px),
        4 => horizontal::<4>(tmp, w, &xw, tw, th, &mut px),
        5 => horizontal::<5>(tmp, w, &xw, tw, th, &mut px),
        6 => horizontal::<6>(tmp, w, &xw, tw, th, &mut px),
        _ => {
            for y in 0..th {
                let src = &tmp[y * w..(y + 1) * w];
                px.extend((0..tw).map(|ox| {
                    let start = xw.start[ox] as usize;
                    let (a, b) = (xw.at[ox] as usize, xw.at[ox + 1] as usize);
                    let ws = &xw.w[a..b];
                    let ss = &src[start..start + ws.len()];
                    let mut acc = 0.0;
                    for (sv, wgt) in ss.iter().zip(ws) {
                        acc += sv * wgt;
                    }
                    acc
                }));
            }
        }
    }
    Gray { w: tw, h: th, px }
}

/// The horizontal pass with every output pixel over `T` taps: the source
/// pixels from `start` on, the weights past an output's own padded with
/// zeros. A tap that would read past the row's end is moved back so that it
/// reads inside it; its weight is zero, so what it reads adds nothing.
#[inline]
fn horizontal<const T: usize>(tmp: &[f32], w: usize, xw: &Taps, tw: usize, th: usize, px: &mut Vec<f32>) {
    let mut starts: Vec<u32> = Vec::with_capacity(tw);
    let mut ws: Vec<[f32; T]> = Vec::with_capacity(tw);
    for ox in 0..tw {
        let (a, b) = (xw.at[ox] as usize, xw.at[ox + 1] as usize);
        let mut start = xw.start[ox] as usize;
        let mut k = [0f32; T];
        k[..b - a].copy_from_slice(&xw.w[a..b]);
        // Shift a window that runs off the end back inside the row, and the
        // weights with it.
        while start + T > w && start > 0 {
            start -= 1;
            k.rotate_right(1);
        }
        starts.push(start as u32);
        ws.push(k);
    }
    for y in 0..th {
        let src = &tmp[y * w..(y + 1) * w];
        let at = px.len();
        px.reserve(tw);
        for (slot, (&st, k)) in px.spare_capacity_mut()[..tw].iter_mut().zip(starts.iter().zip(&ws)) {
            let st = st as usize;
            let ss: &[f32; T] = src[st..st + T].try_into().unwrap();
            let mut acc = 0.0;
            for t in 0..T {
                acc += ss[t] * k[t];
            }
            slot.write(acc);
        }
        // SAFETY: the `tw` slots past `at` were written just above.
        unsafe { px.set_len(at + tw) };
    }
}

/// The source pixels each output pixel averages, flat: output `o` covers
/// `w[at[o]..at[o + 1]]` starting at source index `start[o]`. One allocation
/// for the lot rather than one per output pixel — a 640-wide resample was
/// 640 of them, twice per image.
struct Taps {
    start: Vec<u32>,
    at: Vec<u32>,
    w: Vec<f32>,
}

impl Taps {
    /// The most taps any one output pixel takes.
    fn max_taps(&self) -> usize {
        self.at.windows(2).map(|p| (p[1] - p[0]) as usize).max().unwrap_or(0)
    }
}

/// For each output index: the first source index and the normalised weights
/// covering the source interval [o*s, (o+1)*s) where s = src/dst. Also correct
/// when upscaling (weights become mostly a single 1.0, i.e. nearest-ish box).
fn weights(src: usize, dst: usize) -> Taps {
    let s = src as f64 / dst as f64;
    let mut t = Taps { start: Vec::with_capacity(dst), at: Vec::with_capacity(dst + 1), w: Vec::new() };
    for o in 0..dst {
        let a = o as f64 * s;
        let b = ((o + 1) as f64 * s).min(src as f64);
        let i0 = a.floor() as usize;
        let i1 = (b.ceil() as usize).min(src).max(i0 + 1);
        t.start.push(i0 as u32);
        t.at.push(t.w.len() as u32);
        let mut total = 0.0;
        for i in i0..i1 {
            let lo = (i as f64).max(a);
            let hi = ((i + 1) as f64).min(b);
            let wgt = (hi - lo).max(0.0);
            t.w.push(wgt as f32);
            total += wgt;
        }
        let inv = if total > 0.0 { (1.0 / total) as f32 } else { 0.0 };
        let from = t.at[o] as usize;
        for w in t.w[from..].iter_mut() {
            *w *= inv;
        }
    }
    t.at.push(t.w.len() as u32);
    t
}
