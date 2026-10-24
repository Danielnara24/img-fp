//! What the suggestion asks of a picture beyond its analysis (`Traits`),
//! measured on the rows the reduction reads (`Measure`), and a picture's
//! colour as a grid of cells (`CellColour`).

/// What the suggestion of which copy to keep needs to know about a file that
/// its working plane cannot say, because the working plane is the picture
/// shrunk to `--work-size` with its contrast left as it was and its colour
/// gone: whether its pixels hold real detail at its own size, whether it has
/// colour, and whether its format loses anything. Measured while the decoded
/// rows go through the reduction (`Reducer::push`), so a picture is read once
/// for both; see `suggest.rs` for how each is used.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Traits {
    /// The mean absolute difference in grey (0..=255) between pixels 1, 2, 4
    /// and 8 apart along a row, then 1 and 2 apart down a column, at the
    /// file's own size, over every eighth row. An upscale's pixels differ
    /// little from their neighbours a pixel away and much from those eight
    /// away; the picture it was made from does not.
    pub detail: [f32; 6],
    /// How much colour the picture has: `CellColour`, over the rows sampled
    /// for `detail`. Near nothing for a picture with no colour, however noisy.
    /// Negative where it was not measured, which is a JPEG decoded to its
    /// luma plane alone.
    pub colour: f32,
    /// The mean grey (0..=255) of each cell of a 4x4 grid over the working
    /// plane, row by row.
    pub grid: [f32; 16],
    /// A JPEG's `colour` as the suggestion measured it on an earlier run
    /// (`colour_of`), and kept in its cache record (`cache::Store::keep_colour`);
    /// `colour` itself stays unmeasured, since the decode did not measure it.
    pub colour_kept: Option<f32>,
    /// Stored losslessly: PNG, TIFF, lossless WebP and the like.
    pub lossless: bool,
    /// The file carries an EXIF block.
    pub exif: bool,
    /// The file holds more than one picture — an animation's frames, a
    /// TIFF's pages, an image sequence — of which the analysis read the
    /// first (each format's `Facts::frames`). The rest are pictures no match was
    /// made on, so the suggestion deletes such a file only for a copy of its
    /// bytes.
    pub frames: bool,
    /// More than eight bits a sample: a 16-bit PNG or TIFF, a 12-bit JPEG,
    /// a JPEG XL of more bits, a float picture (EXR, HDR), farbfeld, a PNM
    /// whose largest value is past 255. The analysis reads eight, so an
    /// 8-bit export of such a master is the same pixels to it, and the
    /// master holds what the export lost. Not said of HEIF.
    pub deep: bool,
    /// Carries a colour profile (an ICC profile; for JPEG XL, colours that
    /// are not sRGB): the same values are other colours under it, and a copy
    /// stripped of it is the same pixels shown wrong.
    pub profile: bool,
}

/// `Traits::detail` and `colour` as they are summed over one picture's rows:
/// a `Reducer`'s, made with it and fed by it, so that a decoder that starts
/// over starts this over too.
#[derive(Default)]
pub(super) struct Measure {
    sums: [f64; 6],
    rows_h: f64,
    rows_v: f64,
    colour: CellColour,
    /// The colour channels were not decoded: `colour` means nothing.
    colourless: bool,
    prev: Vec<f32>,
}

impl Measure {
    /// A measure of rows that carry the picture's colour, or, `colourless`,
    /// its grey alone.
    pub fn new(colourless: bool) -> Measure {
        Measure { colourless, ..Default::default() }
    }

    /// One source row's share of the measure: `grey` is its grey values,
    /// 0..=255, and `line` its bytes. Every eighth row is measured along
    /// itself and kept; the one and the two after it are measured against it.
    #[inline]
    pub fn row<const CH: usize, const ALPHA: bool>(&mut self, sy: usize, h: usize, line: &[u8], grey: &[f32]) {
        let phase = sy % MEASURE_STRIDE;
        if phase > 2 {
            return;
        }
        let w = grey.len();
        if phase == 0 {
            if w > 8 {
                let n = w - 8;
                let base = &grey[..n];
                for (k, d) in [1usize, 2, 4, 8].into_iter().enumerate() {
                    self.sums[k] += abs_diff_sum(&grey[d..d + n], base) as f64 / n as f64;
                }
                self.rows_h += 1.0;
            }
            self.prev.clear();
            self.prev.extend_from_slice(grey);
            let colour_ch = if ALPHA { CH - 1 } else { CH };
            if colour_ch == 3 {
                self.colour.add_row(sy, h, line, CH, 1);
            }
        } else if self.prev.len() == w && w > 0 {
            self.sums[3 + phase] += abs_diff_sum(grey, &self.prev) as f64 / w as f64;
            if phase == 1 {
                self.rows_v += 1.0;
            }
        }
    }

    /// What the rows measured, as a file's traits.
    pub fn traits(&self) -> Traits {
        let mut t = Traits::default();
        for k in 0..4 {
            t.detail[k] = if self.rows_h > 0.0 { (self.sums[k] / self.rows_h) as f32 } else { 0.0 };
        }
        for k in 4..6 {
            t.detail[k] = if self.rows_v > 0.0 { (self.sums[k] / self.rows_v) as f32 } else { 0.0 };
        }
        t.colour = if self.colourless { -1.0 } else { self.colour.value() };
        t
    }
}

/// How much colour a picture has: the mean, over a 4x4 grid of cells, of
/// |r - g| + |g - b| between each cell's mean channels. 0 for a grey
/// picture, and a uniform tint's own figure for a tinted one.
///
/// **Of the cells' means, not of the pixels.** Measured pixel by pixel, the
/// noise of a grey photograph counted as colour: grey seeds of the benchmark
/// corpus with noise added read 0.6 to 5.9, and photographs with a faint real
/// tint read 1.4 to 3.0, so no bar could tell a noisy grey copy from a colour
/// one, and the noisy copy was kept as "colour" over a clean lossless grey
/// one. Noise averages away in a cell's mean and a tint does not: over the
/// grid the same noisy grey copies read at most 0.26, and lossy codecs give a
/// grey picture 0.10 at most, while the faintest tints still read 0.62 and
/// 1.16. The cells are `Traits::grid`'s.
#[derive(Default)]
pub(super) struct CellColour {
    pub sum: [[f64; 3]; 16],
    pub n: [f64; 16],
    /// The rows are a JPEG's Y, Cb and Cr rather than R, G and B.
    pub ycbcr: bool,
}

impl CellColour {
    /// Source row `sy` of `h`, `ch` bytes a pixel with the colour channels
    /// first, every `step`th pixel.
    pub fn add_row(&mut self, sy: usize, h: usize, line: &[u8], ch: usize, step: usize) {
        let w = line.len() / ch;
        let cy = (sy * 4 / h.max(1)).min(3);
        for cx in 0..4 {
            let (a, b) = (cx * w / 4, (cx + 1) * w / 4);
            // From the first sampled pixel in the cell, so the sample is the
            // one a single stride over the row takes.
            let first = a.div_ceil(step) * step;
            let (mut r, mut g, mut bl, mut n) = (0u32, 0u32, 0u32, 0u32);
            for p in line[(first * ch).min(b * ch)..b * ch].chunks_exact(ch).step_by(step) {
                r += p[0] as u32;
                g += p[1] as u32;
                bl += p[2] as u32;
                n += 1;
            }
            let c = cy * 4 + cx;
            self.sum[c][0] += r as f64;
            self.sum[c][1] += g as f64;
            self.sum[c][2] += bl as f64;
            self.n[c] += n as f64;
        }
    }

    /// Each cell's mean colour as RGB, `None` for a cell no pixel reached.
    fn rgb(&self) -> [Option<(f64, f64, f64)>; 16] {
        std::array::from_fn(|c| {
            let (s, n) = (&self.sum[c], self.n[c]);
            (n > 0.0).then(|| {
                if self.ycbcr {
                    // JFIF's conversion, which is linear, so the mean colour
                    // of a cell is the conversion of its mean Y, Cb and Cr.
                    // Converted pixel by pixel, by zune-jpeg's integer path,
                    // a grey photograph with noise added read 1.32 where the
                    // same pixels decoded by libjpeg read 0.25.
                    let (y, cb, cr) = (s[0] / n, s[1] / n - 128.0, s[2] / n - 128.0);
                    (y + 1.402 * cr, y - 0.344136 * cb - 0.714136 * cr, y + 1.772 * cb)
                } else {
                    (s[0] / n, s[1] / n, s[2] / n)
                }
            })
        })
    }

    pub fn value(&self) -> f32 {
        let (mut total, mut cells) = (0f64, 0f64);
        for (r, g, b) in self.rgb().into_iter().flatten() {
            total += (r - g).abs() + (g - b).abs();
            cells += 1.0;
        }
        if cells > 0.0 { (total / cells) as f32 } else { 0.0 }
    }

    /// Each cell's colour with its brightness taken out: `r - g` and `g - b`
    /// of its mean, the two differences `value` sums. Zero for a cell no
    /// pixel reached.
    pub fn chroma(&self) -> Chroma {
        self.rgb().map(|c| c.map_or([0.0; 2], |(r, g, b)| [(r - g) as f32, (g - b) as f32]))
    }
}

/// The colour of a 4x4 grid's cells, as `CellColour::chroma` gives it, over
/// the picture as it is shown: what two pictures of the same grey can still
/// differ in. See `chroma_of`.
pub type Chroma = [[f32; 2]; 16];

/// How far apart two pictures' colours are: the mean over the cells of
/// |Δ(r - g)| + |Δ(g - b)|, which is `Traits::colour`'s measure taken of the
/// difference. A grey picture against itself in colour reads its own colour.
pub fn chroma_distance(a: &Chroma, b: &Chroma) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x[0] - y[0]).abs() + (x[1] - y[1]).abs()).sum::<f32>() / 16.0
}

/// How far apart the rows `detail` is measured on are.
const MEASURE_STRIDE: usize = 8;

/// The sum of |a - b| over two rows, in eight lanes of their own: one float
/// accumulator is a chain of dependent adds the compiler may not reorder, and
/// it held the measure to a pixel at a time, which tripled the reduction's
/// cost.
#[inline]
fn abs_diff_sum(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0f32; 8];
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for k in 0..8 {
            acc[k] += (x[k] - y[k]).abs();
        }
    }
    let mut tail = 0f32;
    for (x, y) in ra.iter().zip(rb) {
        tail += (x - y).abs();
    }
    acc.iter().sum::<f32>() + tail
}
