//! What opening a file gives (`Picture`), and where its pixels go (`Sink`).
//!
//! A format's module opens a file and says two things about it: what the
//! file says of itself beyond its pixels (`Facts`), and how to read the
//! pixels into a sink. A sink is a question asked of the pixels — the
//! analysis's working plane, a person's preview — and says what will do for
//! it: rows as they are decoded, or the picture as the `image` crate decodes
//! it whole (`Sink::streams`), and whether a picture's grey alone will do
//! where a decoder holds it apart (`Sink::grey`). A decoder that can give
//! either chooses by what the sink says; one that can give only rows gives
//! rows; one that can give only the whole picture gives that.

use super::Gray;
use anyhow::Result;
use image::DynamicImage;

pub(super) use image::metadata::Orientation;

/// What a file says of itself that its pixels do not: what the suggestion
/// of which copy to keep asks (`Traits`), and what a decode that finished
/// found wrong with it. Asked by the analysis alone, after the decode.
#[derive(Default)]
pub(super) struct Facts {
    /// Stored losslessly; see `Traits::lossless`.
    pub lossless: bool,
    /// Holds more than one picture; see `Traits::frames`.
    pub frames: bool,
    /// Carries an EXIF block where its format keeps one; see `Traits::exif`.
    pub exif: bool,
    /// More than eight bits a sample; see `Traits::deep`.
    pub deep: bool,
    /// Carries a colour profile; see `Traits::profile`.
    pub profile: bool,
    /// What was wrong with a file that decoded only in part; see
    /// `Decoded::damaged`.
    pub damaged: Option<&'static str>,
}

/// A file opened by its format's module: its facts, and its pixels to read.
pub(super) struct Picture<'a> {
    pub facts: Box<dyn FnOnce() -> Facts + 'a>,
    pub read: Read<'a>,
}

/// Decode a picture's pixels into a sink.
pub(super) type Read<'a> = Box<dyn FnOnce(&mut dyn Sink) -> Result<()> + 'a>;

impl<'a> Picture<'a> {
    pub fn new(facts: impl FnOnce() -> Facts + 'a, read: impl FnOnce(&mut dyn Sink) -> Result<()> + 'a) -> Picture<'a> {
        Picture { facts: Box::new(facts), read: Box::new(read) }
    }
}

/// The channels each pixel of a row carries, eight bits each: grey or RGB,
/// with or without alpha last. Every decoder's rows are one of these; a
/// picture in any other layout is made one before it is handed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Layout {
    Grey,
    GreyAlpha,
    Rgb,
    Rgba,
}

impl Layout {
    /// The layout of `colour` channels (1 or 3), and alpha.
    pub fn of(colour: usize, alpha: bool) -> Layout {
        match (colour, alpha) {
            (1, false) => Layout::Grey,
            (1, true) => Layout::GreyAlpha,
            (_, false) => Layout::Rgb,
            (_, true) => Layout::Rgba,
        }
    }

    pub fn ch(self) -> usize {
        match self {
            Layout::Grey => 1,
            Layout::GreyAlpha => 2,
            Layout::Rgb => 3,
            Layout::Rgba => 4,
        }
    }
}

/// What a decoder says of the rows it is about to hand a sink.
#[derive(Clone, Copy)]
pub(super) struct Rows {
    /// The picture as stored, before `orientation` turns it.
    pub w: usize,
    pub h: usize,
    pub layout: Layout,
    /// How the picture is shown; the sink applies it.
    pub orientation: Orientation,
    /// The rows are a colour picture's grey alone: its colour was there and
    /// was not decoded (`Sink::grey`).
    pub grey_of_colour: bool,
}

/// Where a decoder's pixels go.
pub(super) trait Sink {
    /// Whether rows at eight bits will do for a picture shown `w` x `h`,
    /// where a decoder could give either them or the picture as the `image`
    /// crate decodes it whole, at its own depth.
    fn streams(&self, w: usize, h: usize) -> bool;
    /// Whether a picture's grey alone will do, where a decoder holds it apart
    /// from the colour (a JPEG's Y plane).
    fn grey(&self) -> bool;
    /// What the sink holds for a `w` x `h` picture while it is read, which
    /// the decoder claims with its own (`claim`).
    fn holds(&self, w: usize, h: usize) -> u64;
    /// The picture's rows are coming, top first; how many it reads. Starts
    /// the sink afresh, so a decoder that gives up part way and hands the
    /// picture on another way leaves nothing of its rows behind.
    fn begin(&mut self, rows: &Rows) -> usize;
    /// The next row: `w` pixels in the layout `begin` was given.
    fn row(&mut self, line: &[u8]);
    /// The picture decoded whole by the `image` crate, and how it is shown;
    /// `rows_of` hands it on as rows.
    fn whole(&mut self, img: DynamicImage, orientation: Orientation);
}

/// A picture the `image` crate decoded whole, handed to `sink` as rows at
/// eight bits: its own where they are, made RGBA where they are not.
pub(super) fn rows_of<S: Sink + ?Sized>(sink: &mut S, img: DynamicImage, orientation: Orientation) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let rgba;
    let (layout, data): (Layout, &[u8]) = match &img {
        DynamicImage::ImageRgb8(b) => (Layout::Rgb, b.as_raw()),
        DynamicImage::ImageRgba8(b) => (Layout::Rgba, b.as_raw()),
        DynamicImage::ImageLuma8(b) => (Layout::Grey, b.as_raw()),
        DynamicImage::ImageLumaA8(b) => (Layout::GreyAlpha, b.as_raw()),
        _ => {
            rgba = img.to_rgba8();
            (Layout::Rgba, rgba.as_raw())
        }
    };
    let n = sink.begin(&Rows { w, h, layout, orientation, grey_of_colour: false });
    let stride = w * layout.ch();
    for y in 0..n {
        sink.row(&data[y * stride..(y + 1) * stride]);
    }
}

/// Claim what reading a `w` x `h` picture holds: the file's `bytes`, the
/// decoder's own buffers (`own`), and what the sink keeps.
pub(super) fn claim(bytes: &[u8], own: u64, sink: &dyn Sink, w: usize, h: usize) -> super::budget::Permit {
    super::budget::cover(bytes.len() as u64 + own + sink.holds(w, h))
}

/// Whether an orientation turns the picture on its side, so that its shown
/// width is its stored height.
pub(super) fn turns(o: Orientation) -> bool {
    use Orientation::*;
    matches!(o, Rotate90 | Rotate270 | Rotate90FlipH | Rotate270FlipH)
}

/// The size a picture stored at `w` x `h` is shown at.
pub(super) fn shown_size(w: u32, h: u32, o: Orientation) -> (u32, u32) {
    if turns(o) { (h, w) } else { (w, h) }
}

/// A working plane, turned the way an EXIF orientation says to show it.
///
/// **The picture is reduced first and turned after**, as a plane of a few
/// hundred pixels a side. It used to be the other way round: the decoded
/// picture was turned at full size, which held a second copy of it for the
/// turn and walked the first column-wise, a cache miss a pixel — and a phone
/// stores most of its portraits that way. Twenty-four 4032x3024 JPEGs with
/// orientation 6 decoded in 3 CPU-seconds against 2 for the same pictures
/// stored upright, and held 12 MB more a worker. The mapping is
/// `apply_orientation`'s, pixel for pixel; what differs is only which edge the
/// box reduction trims when the size does not divide.
pub(super) fn orient(g: Gray, o: Orientation) -> Gray {
    use Orientation::*;
    if o == NoTransforms {
        return g;
    }
    let (w, h) = (g.w, g.h);
    let (ow, oh) = if turns(o) { (h, w) } else { (w, h) };
    // Where output pixel (x, y) is read from.
    let from = |x: usize, y: usize| -> (usize, usize) {
        match o {
            Rotate90 => (y, h - 1 - x),
            Rotate180 => (w - 1 - x, h - 1 - y),
            Rotate270 => (w - 1 - y, x),
            FlipHorizontal => (w - 1 - x, y),
            FlipVertical => (x, h - 1 - y),
            Rotate90FlipH => (y, x),
            Rotate270FlipH => (w - 1 - y, h - 1 - x),
            NoTransforms => (x, y),
        }
    };
    let mut out = Gray::new(ow, oh);
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = from(x, y);
            out.px[y * ow + x] = g.px[sy * w + sx];
        }
    }
    out
}

/// The EXIF orientation tag (0x0112) from a TIFF structure: IFD0's entry, as
/// the `image` crate reads it.
fn exif_orientation(t: &[u8]) -> Option<u16> {
    let le = match t.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b: [u8; 2] = t.get(i..i.checked_add(2)?)?.try_into().ok()?;
        Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b: [u8; 4] = t.get(i..i.checked_add(4)?)?.try_into().ok()?;
        Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let ifd = u32_at(4)? as usize;
    for k in 0..u16_at(ifd)? as usize {
        let e = ifd.checked_add(2 + 12 * k)?;
        if u16_at(e)? == 0x0112 {
            return u16_at(e + 8);
        }
    }
    None
}

/// Whether an EXIF block turns the picture on its side: orientations 5 to 8.
pub(super) fn exif_turns(tiff: &[u8]) -> bool {
    exif_orientation(tiff).and_then(|o| Orientation::from_exif(o as u8)).is_some_and(turns)
}

/// A sixteen-bit sample as `image` makes it eight bits: `round(c / 257)`,
/// which is `c * 255 / 65535` rounded; no sample sits on a half.
#[inline(always)]
pub(super) fn sample8(c: u16) -> u8 {
    ((c as u32 + 128) / 257) as u8
}

/// One row of sixteen-bit samples, `ch` to a pixel, as RGBA8, the way the
/// general path's `to_rgba8` makes them: grey repeated to red, green and
/// blue, and an opaque alpha where there is none.
pub(super) fn row16_to_rgba8(samples: impl Iterator<Item = u16>, ch: usize, out: &mut [u8]) {
    let mut s = samples.map(sample8);
    for px in out.chunks_exact_mut(4) {
        let mut next = || s.next().unwrap_or(0);
        let p = match ch {
            1 => {
                let l = next();
                [l, l, l, 255]
            }
            2 => {
                let l = next();
                [l, l, l, next()]
            }
            3 => [next(), next(), next(), 255],
            _ => [next(), next(), next(), next()],
        };
        px.copy_from_slice(&p);
    }
}
