//! Every format the `image` crate decodes whole (`whole`), which is also the
//! general path the streaming readers fall back to; what each of these
//! formats' bytes say of it (`facts`); an icon holding an RGB PNG.

use super::budget::{fits_in_memory, limits};
use super::picture::{claim, turns, Facts, Orientation, Picture, Sink};
use crate::timed;
use anyhow::Result;
use image::{DynamicImage, ImageDecoder, ImageFormat};
use std::io::Cursor;

pub(super) fn open(bytes: &[u8], fmt: ImageFormat) -> Picture<'_> {
    Picture::new(move || facts(fmt, bytes), move |sink| whole(bytes, fmt, sink))
}

/// An icon: the crate's choice of its entries, or, where the crate refuses
/// it, that entry read as the PNG it is (`ico_png`).
pub(super) fn open_ico(bytes: &[u8]) -> Picture<'_> {
    Picture::new(
        move || facts(ImageFormat::Ico, bytes),
        move |sink| match whole(bytes, ImageFormat::Ico, sink) {
            Err(e) => match ico_png(bytes) {
                Some(png) => super::png::read(png, sink),
                None => Err(e),
            },
            done => done,
        },
    )
}

/// What a file of one of these formats says of itself. Lossless: the formats
/// that keep every pixel they are given, unless the pixels are indices into
/// a palette (a BMP of eight bits a pixel or fewer, a colour-mapped TGA): a
/// photograph saved as a 256-colour picture is dithered to fit, and counted
/// lossless it tied with the photograph's own lossless copy on everything
/// but size, and was kept over it. A WebP is lossless when its picture is a
/// `VP8L` chunk (`webp_lossless`). More than one picture: an animated GIF or
/// WebP.
fn facts(fmt: ImageFormat, b: &[u8]) -> Facts {
    let u16_at = |at: usize| b.get(at..at + 2).map(|v| u16::from_le_bytes([v[0], v[1]]));
    let u32_at = |at: usize| b.get(at..at + 4).map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]));
    // A WebP's EXIF is a chunk of its own, after the picture.
    let exif = fmt == ImageFormat::WebP && riff_chunks(b, b"EXIF") >= 1;
    let (lossless, frames) = match fmt {
        ImageFormat::WebP => (webp_lossless(b) == Some(true), riff_chunks(b, b"ANMF") >= 2),
        ImageFormat::Gif => (false, gif_frames(b) >= 2),
        // The bit count follows the header's size: at 24 in the old 12-byte
        // OS/2 header, at 28 in every later one.
        ImageFormat::Bmp => {
            let at = if u32_at(14) == Some(12) { 24 } else { 28 };
            (!u16_at(at).is_some_and(|bits| (1..=8).contains(&bits)), false)
        }
        // Image type 1 or 9: colour-mapped, plain or run-length.
        ImageFormat::Tga => (!matches!(b.get(2), Some(1 | 9)), false),
        ImageFormat::Pnm | ImageFormat::Qoi | ImageFormat::Ico | ImageFormat::OpenExr | ImageFormat::Hdr | ImageFormat::Farbfeld => (true, false),
        _ => (false, false),
    };
    Facts { lossless, frames, exif, ..Facts::default() }
}

/// Any format `image` reads, decoded whole, into `sink`.
pub(super) fn whole(bytes: &[u8], fmt: ImageFormat, sink: &mut dyn Sink) -> Result<()> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), fmt);
    reader.limits(limits());
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    // The header already says how large the pixels will be. A channel layout
    // the sink has no rows for is converted to RGBA8 first, which holds
    // another copy (`picture::rows_of`). An orientation is the sink's to
    // apply.
    let converts = !matches!(
        decoder.color_type(),
        image::ColorType::Rgb8 | image::ColorType::Rgba8 | image::ColorType::L8 | image::ColorType::La8
    );
    let (w, h) = decoder.dimensions();
    fits_in_memory(&decoder)?;
    let own = decoder.total_bytes() + if converts { w as u64 * h as u64 * 4 } else { 0 };
    let _permit = claim(bytes, own, sink, w as usize, h as usize);
    let img = timed!(1, DynamicImage::from_decoder(decoder)?);
    sink.whole(img, orientation);
    Ok(())
}

/// A picture's shown size from the `image` crate's own decoder, which reads
/// the header and whatever metadata comes before the pixels, and nothing else.
pub(super) fn size<R: std::io::BufRead + std::io::Seek>(r: R, fmt: ImageFormat) -> Option<(u32, u32)> {
    let mut rd = image::ImageReader::with_format(r, fmt);
    // The decode's own limits, or a picture the decode reads has no size here.
    rd.limits(limits());
    let mut d = rd.into_decoder().ok()?;
    let (w, h) = d.dimensions();
    Some(if d.orientation().is_ok_and(turns) { (h, w) } else { (w, h) })
}

/// The PNG inside an icon, for an icon the image crate refuses.
///
/// An icon's entries may be PNGs, and the crate accepts only one in RGBA,
/// which is what the format asks for. Pillow does not do that: saving a
/// picture with no alpha channel as `.ico` — `Image.open("logo.jpg")
/// .save("favicon.ico")`, which is how a great many favicons are made — writes
/// an RGB PNG under a directory entry claiming 32 bits a pixel, and every one
/// of them was "not in RGBA format". Browsers show them, so they are read:
/// the entry the crate would have chosen — most bits a pixel, then most
/// pixels, the last of equals — and, when it is a PNG, as a PNG. An entry
/// that is a bitmap was the crate's to read and is not second-guessed.
pub(super) fn ico_png(bytes: &[u8]) -> Option<&[u8]> {
    let u16_at = |i: usize| Some(u16::from_le_bytes(bytes.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?));
    if u16_at(0)? != 0 || !matches!(u16_at(2)?, 1 | 2) {
        return None;
    }
    let side = |b: u8| if b == 0 { 256u32 } else { b as u32 };
    let mut entries: Vec<((u16, u32), usize, usize)> = Vec::new();
    for k in 0..u16_at(4)? as usize {
        let e = 6 + 16 * k;
        let (w, h) = (side(*bytes.get(e)?), side(*bytes.get(e + 1)?));
        let score = (u16_at(e + 6)?, w * h);
        entries.push((score, u32_at(e + 12)? as usize, u32_at(e + 8)? as usize));
    }
    // The crate's own choice, step for step: the last entry first, then each
    // other one in order if it scores strictly higher.
    let mut best = entries.pop()?;
    for e in entries {
        if e.0 > best.0 {
            best = e;
        }
    }
    let (_, at, len) = best;
    let entry = bytes.get(at..at.checked_add(len)?)?;
    entry.starts_with(b"\x89PNG\r\n\x1a\n").then_some(entry)
}

/// Whether a WebP's picture is lossless (`VP8L`) or lossy (`VP8 `), from the
/// first picture chunk in `b`; for an animation, its first frame's. `None`
/// when `b` ends first.
pub(super) fn webp_lossless(b: &[u8]) -> Option<bool> {
    fn walk(b: &[u8], mut at: usize) -> Option<bool> {
        loop {
            let id = b.get(at..at + 4)?;
            let len = u32::from_le_bytes(b.get(at + 4..at + 8)?.try_into().ok()?) as usize;
            match id {
                b"VP8L" => return Some(true),
                b"VP8 " => return Some(false),
                // A frame: 16 bytes of placement, then its own chunks.
                b"ANMF" => return walk(b, at + 8 + 16),
                _ => at = at.checked_add(8)?.checked_add(len + (len & 1))?,
            }
        }
    }
    walk(b, 12)
}

/// How many top-level chunks of a RIFF file (a WebP) carry `fourcc`, counted
/// to two.
pub(super) fn riff_chunks(b: &[u8], fourcc: &[u8; 4]) -> usize {
    let mut i = 12;
    let mut count = 0;
    while i + 8 <= b.len() && count < 2 {
        let n = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        if &b[i..i + 4] == fourcc {
            count += 1;
        }
        // Chunks are padded to an even length.
        i = match i.checked_add(8 + n + (n & 1)) {
            Some(j) => j,
            None => break,
        };
    }
    count
}

/// How many images a GIF holds: its image descriptors, walked block by block.
pub(super) fn gif_frames(b: &[u8]) -> usize {
    if b.len() < 13 {
        return 0;
    }
    let table = |flags: u8| if flags & 0x80 != 0 { 3 << ((flags & 7) + 1) } else { 0 };
    let mut i = 13 + table(b[10]);
    let mut frames = 0;
    // Data sub-blocks: a length byte and that many bytes, to a zero length.
    let skip_blocks = |mut i: usize| -> Option<usize> {
        loop {
            let n = *b.get(i)? as usize;
            i += 1 + n;
            if n == 0 {
                return Some(i);
            }
        }
    };
    while let Some(&tag) = b.get(i) {
        match tag {
            0x2C => {
                frames += 1;
                if frames >= 2 {
                    break;
                }
                let Some(&flags) = b.get(i + 9) else { break };
                // Descriptor, local table, LZW code size, then data.
                match skip_blocks(i + 10 + table(flags) + 1) {
                    Some(n) => i = n,
                    None => break,
                }
            }
            0x21 => match skip_blocks(i + 2) {
                Some(n) => i = n,
                None => break,
            },
            _ => break,
        }
    }
    frames
}
