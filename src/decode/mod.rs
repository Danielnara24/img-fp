//! File discovery and decoding to a single-channel working image.
//!
//! Formats are identified by content, not extension: a file with no extension
//! (or a wrong one) still decodes if its bytes are an image. Every path ends in
//! the same place — a `Gray` of f32 in 0..=1 whose long side is at most the
//! requested working size — so the rest of the pipeline never sees a format.
//!
//! The channel used is BT.601 luma (see `reduce::luma`), and a JPEG's is its
//! own Y plane (`jpeg::luma`), so that a JPEG and a PNG of one picture give
//! the same grey.
//!
//! **One way in for every question.** A file is identified once
//! (`sniff::identify`) and opened once (`open`), by the one module that knows
//! its format: `jpeg`, `png`, `tiff`, `jxl`, `heif`, or `other` for what the
//! `image` crate reads whole. Opening says what the file is beyond its pixels
//! (`Facts`) and how to read them: into a `Sink`, a row at a time where the
//! format allows. The analysis reads into `reduce::Reduce`, a person's preview
//! into `shown::Shown`; neither knows a format, and no format knows which of
//! them it feeds. The file's header alone, for the progress bar, is `probe`.
//!
//! - `sniff`: what format a file's bytes are, and whether they are one.
//! - `picture`: `Picture`, `Sink`, `Rows`, `Layout`; turning a picture.
//! - `budget`: how much decoding may be held at once, and the allocation limit.
//! - `reduce`: rows to the working plane, and the area resampling.
//! - `measure`: `Traits`, measured on the way through the reduction.
//! - `shown`: rows or a whole picture to a picture to look at.

use anyhow::{bail, Context, Result};
use crate::timed;
use image::{DynamicImage, ImageFormat};
use std::path::Path;

mod budget;
mod heif;
mod jpeg;
mod jxl;
mod measure;
mod other;
mod picture;
mod png;
mod reduce;
mod shown;
mod sniff;
mod tiff;
#[cfg(test)]
mod tests;

pub use jpeg::same_jpeg_coding;
pub use measure::{chroma_distance, Chroma, Traits};
pub use reduce::{area_factor, resize_area};
pub use shown::fit_preview;
pub use sniff::{is_a_picture, may_be_image, Kind, EXTENSIONS, MAC_METADATA, NOT_AN_IMAGE};

use measure::CellColour;
use picture::{orient, Facts, Orientation, Picture};

/// Single-channel image, row-major, values in 0..=1.
#[derive(Clone, Debug)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Gray {
    pub fn new(w: usize, h: usize) -> Self {
        Gray { w, h, px: vec![0.0; w * h] }
    }
}

pub struct Decoded {
    /// Long side <= the working size passed to `decode`, times
    /// `area_factor` for a picture wider than 4:3.
    pub work: Gray,
    /// The picture's own width and height, as shown: an EXIF orientation that
    /// turns it on its side is applied, as it is to `work`.
    pub size: (u32, u32),
    /// What was wrong with a file that decoded only in part: a JPEG cut off
    /// in its image data, which the decoder fills out with grey rather than
    /// refuse. See `jpeg::cut_off`.
    pub damaged: Option<String>,
    /// What the keep/delete suggestion asks of the file beyond its analysis.
    pub traits: Traits,
}

/// The error a camera's raw file is refused with (`tiff::open`).
pub const CAMERA_RAW: &str = "a camera raw file, which img-fp does not read";

/// A file's bytes, opened by the module of its format: what the file says
/// of itself, and how to read its pixels. The one place a format is chosen.
fn open(kind: Kind, bytes: &[u8]) -> Result<Picture<'_>> {
    Ok(match kind {
        Kind::Image(ImageFormat::Jpeg) => jpeg::open(bytes),
        Kind::Image(ImageFormat::Png) => png::open(bytes),
        Kind::Image(ImageFormat::Tiff) => tiff::open(bytes)?,
        Kind::Image(ImageFormat::Ico) => other::open_ico(bytes),
        Kind::Image(fmt) => other::open(bytes, fmt),
        Kind::Jxl => jxl::open(bytes)?,
        Kind::Heif => heif::open(bytes)?,
    })
}

/// Decode a file to a working-resolution gray image.
#[cfg(test)]
pub fn decode(path: &Path, work_size: usize) -> Result<Decoded> {
    decode_with(path, work_size, probe(path).as_ref())
}

/// `decode`, given what `probe` read of the file's header, which decides how
/// much of the decode budget to claim before the file is read; `None` claims
/// the file's bytes alone.
pub fn decode_with(path: &Path, work_size: usize, header: Option<&Probe>) -> Result<Decoded> {
    // The head first, so a file that is no picture is turned away without
    // queueing for room it would never use: under `-x '*'` that is every
    // video in the folder, and each would wait for the budget to empty.
    let (f, head, len) = timed!(0, sniff::open_if_image(path)?);
    let _claim = budget::Claim::new(len + header.map_or(0, |p| budget::decode_estimate(p, work_size)));
    let bytes = timed!(0, sniff::read_rest(f, head, len, path)?);
    let kind = sniff::identify(&bytes, path).context(NOT_AN_IMAGE)?;
    let picture = open(kind, &bytes)?;
    let mut sink = reduce::Reduce::new(work_size);
    let read = picture.read;
    match prof_slot(kind) {
        Some(_slot) => timed!(_slot, read(&mut sink)),
        None => read(&mut sink),
    }?;
    let (work, size, mut traits) = sink.finish()?;
    let Facts { lossless, frames, damaged } = (picture.facts)();
    traits.lossless = lossless;
    traits.frames = frames;
    traits.exif = has_exif(&bytes);
    Ok(Decoded { work, size, damaged: damaged.map(String::from), traits })
}

/// The profile's line for a format's decode: the formats that are almost all
/// of a real corpus get their own; everything else shares `decode:codec`.
fn prof_slot(kind: Kind) -> Option<usize> {
    match kind {
        Kind::Image(ImageFormat::Jpeg) => Some(22),
        Kind::Image(ImageFormat::Png) => Some(23),
        Kind::Image(ImageFormat::WebP) => Some(24),
        Kind::Image(ImageFormat::Tiff) => Some(25),
        Kind::Jxl => Some(26),
        Kind::Heif => Some(27),
        Kind::Image(_) => None,
    }
}

/// Whether a file carries an EXIF block where formats put one: within its
/// first 128 KB, as JPEG's APP1, WebP's and HEIF's `Exif\0\0`, or PNG's
/// `eXIf` chunk.
fn has_exif(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1 << 17)];
    memchr::memmem::find(head, b"Exif\0\0").is_some() || memchr::memmem::find(head, b"eXIf").is_some()
}

/// What a file's header says, read without decoding anything: its format and
/// the size of the picture. It exists for the progress bar, which weighs each
/// file by what analysing it will cost (see `progress::analysis_cost`), and
/// nothing about the result depends on it — a file this cannot read is
/// weighed from its size and then decoded, or refused, as usual.
#[derive(Clone, Copy)]
pub struct Probe {
    pub kind: Kind,
    pub w: u32,
    pub h: u32,
}

/// Read a file's header. JPEG and PNG, which are nearly every file in a real
/// corpus, are parsed here by hand, because the `image` crate's JPEG decoder
/// reads the whole file into memory before it will say how large the picture
/// is — and a probe that reads every byte twice is not a probe. Walking the
/// markers to the frame header touches a few kilobytes.
///
/// **The size is the picture's as it is shown**, with an EXIF orientation
/// that turns it on its side already applied — which is what `decode` does to
/// the pixels, and what a person looking at the file sees. It used to be the
/// stored size, so a portrait photograph from a phone, stored landscape with
/// an orientation of 6, was reported 4032x3024 beside an upright copy of
/// itself at 3024x4032, in every report and on the window's cards.
pub fn probe(path: &Path) -> Option<Probe> {
    // A header parser is a decoder too; see `decode_guarded`.
    std::panic::catch_unwind(|| probe_header(path)).ok().flatten()
}

/// The size a file's header gives, by its format's own reader of it: each
/// reads what it must and no more.
fn probe_header(path: &Path) -> Option<Probe> {
    use std::io::{BufRead, BufReader};
    let mut r = BufReader::with_capacity(16 << 10, std::fs::File::open(path).ok()?);
    let head = r.fill_buf().ok()?;
    let kind = sniff::identify(head, path)?;
    let (w, h) = match kind {
        Kind::Image(ImageFormat::Jpeg) => jpeg::size(&mut r)?,
        Kind::Image(ImageFormat::Png) => png::size(&mut r)?,
        Kind::Image(fmt) => other::size(r, fmt)?,
        Kind::Jxl => jxl::size(&mut r)?,
        Kind::Heif => heif::size(path)?,
    };
    Some(Probe { kind, w, h })
}

/// `decode`, with a panic inside it made an error for this file.
///
/// The decoders are other people's code reading bytes nobody vouches for,
/// and a panic in one, unwinding through the parallel analysis, ended the
/// whole run — and every rescan, at the same file. Caught here it is that
/// file's problem, as any refusal is. Nothing the decode holds outlives it
/// in a broken state: its budget claim is given back as it unwinds.
pub fn decode_guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let why = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned());
            bail!("the decoder failed: {}", why.unwrap_or_else(|| "panic".into()))
        }
    }
}

/// `Traits::colour` of a file whose decode did not measure it: a JPEG, whose
/// colour channels the analysis never decodes. The suggestion asks it of the
/// few JPEGs a group could keep, and only until one has colour. A JPEG with
/// colour planes is decoded to its Y, Cb and Cr, which skips the colour
/// conversion, and measured as a decode measures any picture (`CellColour`),
/// over every pixel of every fourth row, its cells' means converted to RGB.
/// It used to be |Cb - 128| + |Cr - 128| pixel by pixel, which reads a
/// picture at about 0.6 of the RGB figure, against the one bar for both.
/// Anything else is read through `preview`.
pub fn colour_of(path: &Path) -> Option<f32> {
    Some(cells_of(path)?.0.value())
}

/// The colour of `path`'s cells as it is shown, the picture turned as its
/// EXIF orientation says: what `same::same_analysis` asks of two files whose
/// grey is the same, since the analysis sees nothing else. Decoded as
/// `colour_of` decodes, over every fourth row of a JPEG.
pub fn chroma_of(path: &Path) -> Option<Chroma> {
    let (cells, orientation) = cells_of(path)?;
    let chroma = cells.chroma();
    // Where each cell of the grid as shown is read from: the grid turned by
    // the same mapping a working plane is turned by.
    let mut index = Gray::new(4, 4);
    for (k, v) in index.px.iter_mut().enumerate() {
        *v = k as f32;
    }
    let from = orient(index, orientation);
    Some(std::array::from_fn(|k| chroma[from.px[k] as usize]))
}

/// The cells' colour sums for `colour_of` and `chroma_of`, and the turn the
/// picture is shown with, which a JPEG's own decode here does not apply: a
/// JPEG's from its own colour planes (`jpeg::cells`), anything else's from
/// its `preview`.
fn cells_of(path: &Path) -> Option<(CellColour, Orientation)> {
    let bytes = std::fs::read(path).ok()?;
    if sniff::identify(&bytes, path) == Some(Kind::Image(ImageFormat::Jpeg))
        && let Some(cells) = jpeg::cells(&bytes)
    {
        return Some(cells);
    }
    // `preview` turns the picture itself.
    let (w, h, rgba) = shrunk(decode_guarded(|| shown(&bytes, path, None)).ok()?, 256);
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut c = CellColour::default();
    for y in 0..h {
        c.add_row(y, h, &rgba[y * w * 4..(y + 1) * w * 4], 4, 1);
    }
    Some((c, Orientation::NoTransforms))
}

/// A picture for a person to look at rather than for the analysis: colour,
/// the right way up, and no larger than `long` on its long side, as RGBA.
///
/// Only `img-fp-gui` asks for one to show; a scan asks for one only to
/// measure a picture's colour (`colour_of`). It reads every format a scan
/// reads, through the same decoders, and claims the decode budget as a scan
/// does.
pub fn preview(path: &Path, long: u32) -> Result<(u32, u32, Vec<u8>)> {
    // The whole picture shrunk, as it always was: a scan reads this
    // (`colour_of`), so it does not take `preview_fit`'s reductions.
    let img = decode_guarded(|| shown(&read(path)?, path, None))?;
    Ok(shrunk(img, long))
}

/// `img` no larger than `long` on its long side, as RGBA.
fn shrunk(img: DynamicImage, long: u32) -> (u32, u32, Vec<u8>) {
    let long = long.max(1);
    let img = if img.width().max(img.height()) > long { img.thumbnail(long, long) } else { img };
    let rgba = img.into_rgba8();
    (rgba.width(), rgba.height(), rgba.into_raw())
}

/// A preview made for a place `w` x `h` pixels: shrunk to fit inside it, or
/// with `cover`, shrunk until it fills it and then cut to it, the middle
/// kept. Never enlarged, so a picture smaller than the place comes back at
/// its own size (and, with `cover`, cut only where it overhangs). RGBA.
///
/// A window draws a picture fastest at exactly the size it is shown, and a
/// texture twice that size costs four times its memory; this is that size.
/// A picture large enough to be reduced by an integer factor first is read
/// a row at a time into the reduction where its format allows
/// (`shown::Shown`), so a 44-megapixel PNG never exists whole.
pub fn preview_fit(path: &Path, w: u32, h: u32, cover: bool) -> Result<(u32, u32, Vec<u8>)> {
    let (w, h) = (w.max(1), h.max(1));
    let img = decode_guarded(|| shown(&read(path)?, path, Some((w, h, cover))))?;
    Ok(fit_preview(img, w, h, cover))
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("read {}", path.display()))
}

/// The picture in `bytes`, the right way up: whole, or, made for `place`, as
/// reduced while it is read as the place allows.
fn shown(bytes: &[u8], path: &Path, place: Option<(u32, u32, bool)>) -> Result<DynamicImage> {
    let kind = sniff::identify(bytes, path).context(NOT_AN_IMAGE)?;
    let mut sink = shown::Shown::new(place);
    (open(kind, bytes)?.read)(&mut sink)?;
    sink.finish()
}
