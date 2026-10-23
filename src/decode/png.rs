//! PNG: a row at a time where a sink takes rows (`rows`), the `image`
//! crate's whole decode otherwise; its size from IHDR; a palette and an
//! animation from its chunks.

use super::budget::max_alloc;
use super::picture::{claim, exif_turns, row16_to_rgba8, shown_size, Facts, Layout, Orientation, Picture, Rows, Sink};
use crate::timed;
use anyhow::Result;
use image::ImageFormat;
use std::cell::Cell;
use std::io::Cursor;
use std::rc::Rc;

/// What `Decoded::damaged` says of a PNG `rows` read only in part.
const CUT_OFF: &str = "the file ends before its image data does";

pub(super) fn open(bytes: &[u8]) -> Picture<'_> {
    // Whether the read found the file cut off, which the facts are asked
    // after.
    let cut = Rc::new(Cell::new(false));
    let read_cut = cut.clone();
    Picture::new(
        // A palette is not lossless, whatever the format: see `Traits::lossless`.
        move || Facts {
            lossless: !indexed(bytes),
            frames: apng_animated(bytes),
            exif: has_chunk(bytes, b"eXIf"),
            damaged: cut.get().then_some(CUT_OFF),
        },
        move |sink| {
            match rows(bytes, sink) {
                Some(Read::Whole) => {}
                Some(Read::CutOff) => read_cut.set(true),
                None => return super::other::whole(bytes, ImageFormat::Png, sink),
            }
            Ok(())
        },
    )
}

/// A PNG's rows into `sink`, or its whole decode where `rows` declines. A
/// cut-off one is read as `rows` reads it.
pub(super) fn read(bytes: &[u8], sink: &mut dyn Sink) -> Result<()> {
    match rows(bytes, sink) {
        Some(_) => Ok(()),
        None => super::other::whole(bytes, ImageFormat::Png, sink),
    }
}

/// What `rows` read: a whole file, or a cut-off one as far as it goes and
/// grey for the rest.
pub(super) enum Read {
    Whole,
    CutOff,
}

/// A PNG decoded a row at a time straight into the box reduction, so that the
/// picture never exists whole.
///
/// It is the largest transient the benchmark corpus has. A forty-megapixel PNG
/// is 133 MB of RGB, or 177 of RGBA, held only long enough to be reduced to a
/// working plane of a megabyte; there are a hundred PNGs past four megapixels
/// in that corpus and nothing in the analysis needs any row but the one it is
/// reading. The `png` crate unfilters a row at a time anyway — `next_frame` is
/// a loop over the same rows into one big buffer — so asking for them one by
/// one yields the same bytes, and the reduction sums them exactly as it sums a
/// whole buffer's rows (see `Reducer`).
///
/// `None` means "take the general path", and it is what this says about
/// anything it is not sure it would read identically: an interlaced or
/// animated file, a palette — and any error at all, so that a broken file
/// fails with the message the general path gives it. Sixteen bits a channel
/// are made eight as the general path's `to_rgba8` makes them, and handed
/// on as RGBA8.
///
/// **A frame larger than `max_alloc` is read here too.** It used to be sent
/// to the general path, to be refused there as it would have been — but the
/// limit exists to turn an allocation larger than memory into an error, and
/// this path makes no such allocation. So the pictures it was written for
/// were exactly the ones it turned away: a 32000x32000 PNG, 3 GB of RGB, went
/// to the general path, held it all and peaked at 3.0 GB of RSS in swap on a
/// 6 GB machine, where a 12000x12000 one through here peaked at 18 MB. On a
/// machine with less free it would have been "Memory limit exceeded". That costs a truncated file a second decode, which is a
/// price only broken files pay.
///
/// A sink that would rather have the picture whole at its own depth
/// (`Sink::streams`) is declined too, unless the file is cut off.
///
/// **A file that ends before its image data does (`cut_off`) is read as far
/// as it goes**, and the rows it does not hold are mid-grey, as the JPEG
/// decoder fills a cut-off JPEG. It was an error ("unexpected end of file"),
/// a problem and nothing more, where a JPEG cut at the same place was
/// analysed, matched, and said to be damaged: a download or a copy that
/// stopped part-way is the same accident whatever the format. Only the end
/// of the file is forgiven; data that is there and wrong still fails as the
/// general path fails it.
pub(super) fn rows(bytes: &[u8], sink: &mut dyn Sink) -> Option<Read> {
    // What `image` asks of the `png` crate, so that the rows are the rows it
    // would have got: the allocation limit the general path is given (see
    // `max_alloc`), text chunks read, and EXPAND, which widens palettes and
    // low bit depths to eight bits and a tRNS chunk to an alpha channel.
    let mut dec = png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: max_alloc() as usize });
    dec.set_ignore_text_chunk(false);
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().ok()?;
    let info = reader.info();
    if info.interlaced || info.animation_control.is_some() {
        return None;
    }
    // The orientation the general path reads, from an eXIf chunk ahead of
    // the pixels.
    let orientation = info
        .exif_metadata
        .as_deref()
        .and_then(Orientation::from_exif_chunk)
        .unwrap_or(Orientation::NoTransforms);
    let (w, h) = (info.width as usize, info.height as usize);
    let (sw, sh) = shown_size(w as u32, h as u32, orientation);
    let cut = cut_off(bytes);
    if !sink.streams(sw as usize, sh as usize) && !cut {
        return None;
    }
    let line = reader.output_line_size(info.width)? as u64;
    let (color, depth) = reader.output_color_type();
    let layout = match color {
        png::ColorType::Grayscale => Layout::Grey,
        png::ColorType::GrayscaleAlpha => Layout::GreyAlpha,
        png::ColorType::Rgb => Layout::Rgb,
        png::ColorType::Rgba => Layout::Rgba,
        png::ColorType::Indexed => return None,
    };
    let ch = layout.ch();
    // The file, the working plane, and the decoder's current and previous
    // rows (it unfilters against the one before); sixteen bits a channel
    // also hold the row made RGBA8.
    let sixteen = match depth {
        png::BitDepth::Eight => false,
        png::BitDepth::Sixteen => true,
        _ => return None,
    };
    let _permit = claim(bytes, 2 * line + if sixteen { w as u64 * 4 } else { 0 }, sink, w, h);
    let layout = if sixteen { Layout::Rgba } else { layout };
    let wanted = sink.begin(&Rows { w, h, layout, orientation, grey_of_colour: false });
    let mut rgba = vec![0u8; if sixteen { w * 4 } else { 0 }];
    let mut filled = false;
    timed!(1, {
        // Every row is read, the ones the sink does not want included, so
        // that a file broken in its last rows fails here as it fails there.
        for y in 0..h {
            let row = match reader.next_row() {
                Ok(Some(row)) => row,
                _ if cut => {
                    let ch = layout.ch();
                    let grey: Vec<u8> = (0..w * ch).map(|k| if ch % 2 == 0 && k % ch == ch - 1 { 255 } else { 128 }).collect();
                    for _ in y..wanted {
                        sink.row(&grey);
                    }
                    filled = true;
                    break;
                }
                _ => return None,
            };
            let data = row.data();
            if data.len() != w * ch * if sixteen { 2 } else { 1 } {
                return None;
            }
            if y >= wanted {
                continue;
            }
            if sixteen {
                row16_to_rgba8(data.chunks_exact(2).map(|b| u16::from_be_bytes([b[0], b[1]])), ch, &mut rgba);
                sink.row(&rgba);
            } else {
                sink.row(data);
            }
        }
    });
    // Every row may be there with the file still cut off after them (the
    // end of the data stream, `IEND`): damaged, and nothing to fill.
    if filled || cut {
        return Some(Read::CutOff);
    }
    if reader.next_row().ok()?.is_some() {
        return None;
    }
    Some(Read::Whole)
}

/// Whether a PNG's file ends before its chunks do: a chunk that runs past
/// the end, or no `IEND`. Read from the chunks' lengths alone, not their
/// contents.
pub(super) fn cut_off(b: &[u8]) -> bool {
    let mut i = 8;
    loop {
        let Some(head) = b.get(i..i + 8) else { return true };
        let len = u32::from_be_bytes(head[..4].try_into().unwrap()) as usize;
        if &head[4..] == b"IEND" {
            return false;
        }
        match i.checked_add(12 + len) {
            Some(j) if j <= b.len() => i = j,
            _ => return true,
        }
    }
}

/// The shown size from a PNG's IHDR, turned by an eXIf chunk ahead of the
/// pixels.
pub(super) fn size(r: &mut std::io::BufReader<std::fs::File>) -> Option<(u32, u32)> {
    use std::io::BufRead;
    // The signature, then IHDR's length and type, then its width and
    // height; the chunk is required to come first.
    let head = r.fill_buf().ok()?;
    let be = |i: usize| head.get(i..i + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()));
    let (w, h) = (be(16)?, be(20)?);
    Some(if png_turned(r) { (h, w) } else { (w, h) })
}

/// Whether a PNG's `eXIf` chunk turns it, read from the chunks between IHDR
/// and the first IDAT — the ones the decoder reads before the pixels, and so
/// the only ones `decode` applies.
fn png_turned(r: &mut std::io::BufReader<std::fs::File>) -> bool {
    use std::io::Read;
    // The signature and IHDR, which `probe` has already read.
    if r.seek_relative(8 + 8 + 13 + 4).is_err() {
        return false;
    }
    loop {
        let mut head = [0u8; 8];
        if r.read_exact(&mut head).is_err() {
            return false;
        }
        let len = u32::from_be_bytes(head[..4].try_into().unwrap()) as usize;
        match &head[4..] {
            b"IDAT" | b"IEND" => return false,
            b"eXIf" => {
                // A chunk is at most 2^31 bytes by the format; an EXIF block
                // past a megabyte is not one a probe needs to read.
                if len > 1 << 20 {
                    return false;
                }
                let mut data = vec![0u8; len];
                return r.read_exact(&mut data).is_ok() && exif_turns(&data);
            }
            _ => {
                if r.seek_relative(len as i64 + 4).is_err() {
                    return false;
                }
            }
        }
    }
}

/// Whether a PNG's pixels are indices into a palette: colour type 3, in
/// IHDR, which is the first chunk.
fn indexed(b: &[u8]) -> bool {
    b.get(12..16) == Some(b"IHDR") && b.get(25) == Some(&3)
}

/// Whether a PNG has a chunk of type `kind`, before its end or wherever it
/// lies: `eXIf` may follow the image data.
fn has_chunk(b: &[u8], kind: &[u8; 4]) -> bool {
    let mut i = 8;
    while i + 8 <= b.len() {
        let n = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        match &b[i + 4..i + 8] {
            t if t == kind => return true,
            b"IEND" => return false,
            _ => {}
        }
        i = match i.checked_add(12 + n) {
            Some(j) => j,
            None => return false,
        };
    }
    false
}

/// Whether a PNG is an animation: an `acTL` chunk, with more than one frame
/// or with the one frame not the picture `IDAT` holds.
pub(super) fn apng_animated(b: &[u8]) -> bool {
    let mut i = 8;
    let mut frames = None;
    let mut control_before_data = false;
    while i + 8 <= b.len() {
        let n = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        match &b[i + 4..i + 8] {
            b"acTL" => frames = b.get(i + 8..i + 12).map(|v| u32::from_be_bytes([v[0], v[1], v[2], v[3]])),
            b"fcTL" => control_before_data = true,
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        i = match i.checked_add(12 + n) {
            Some(j) => j,
            None => break,
        };
    }
    frames.is_some_and(|f| f > 1 || !control_before_data)
}
