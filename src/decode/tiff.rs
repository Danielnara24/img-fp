//! TIFF: refused when it is a camera's raw file; a strip or a row of tiles
//! at a time where a sink takes rows (`chunks`), the `image` crate's whole
//! decode otherwise; what its directories say (`Head`).

use super::budget::max_alloc;
use super::picture::{claim, row16_to_rgba8, shown_size, Facts, Layout, Orientation, Picture, Rows, Sink};
use crate::timed;
use anyhow::{bail, Result};
use image::ImageFormat;
use std::io::Cursor;

/// A TIFF, unless its directories say it is a camera's raw file: its first
/// picture is then the camera's preview, and read as the photograph it would
/// be judged at a few hundred pixels and suggested for deletion beside its
/// own JPEG.
pub(super) fn open(bytes: &[u8]) -> Result<Picture<'_>> {
    let head = Head::of(bytes);
    if head.raw {
        bail!(super::Refused::CameraRaw);
    }
    Ok(Picture::new(
        move || Facts {
            lossless: head.compression.is_none_or(|c| !LOSSY.contains(&c)),
            frames: head.pictures >= 2,
            ..Facts::default()
        },
        move |sink| {
            if chunks(bytes, sink).is_some() {
                return Ok(());
            }
            super::other::whole(bytes, ImageFormat::Tiff, sink)
        },
    ))
}

/// TIFF compression schemes that lose pixels: old-style and new-style JPEG,
/// and the lossy JPEG of DNG.
const LOSSY: [u16; 3] = [6, 7, 34892];

/// What the suggestion and the raw test ask of a TIFF's directories.
#[derive(Default)]
pub(super) struct Head {
    /// Directories holding a whole picture rather than a reduced copy of one
    /// (`NewSubfileType` without its first bit), up to two.
    pub pictures: usize,
    /// A camera's raw file: a DNG (`DNGVersion`), a Canon CR2 (`CR` after
    /// the header), or a first directory that is a reduced-resolution
    /// preview, or that names a camera (`Make`) and keeps its pictures in
    /// `SubIFDs` — the layout of NEF, ARW, PEF, SRW and their kin.
    pub raw: bool,
    /// The first directory's compression (tag 259), where it says.
    pub compression: Option<u16>,
}

impl Head {
    /// A TIFF's chain of directories, read off its bytes as far as they read
    /// as one. Classic TIFF and BigTIFF, either byte order.
    pub fn of(b: &[u8]) -> Head {
        let mut head = Head::default();
        let le = match b.get(..2) {
            Some(b"II") => true,
            Some(b"MM") => false,
            _ => return head,
        };
        let at = |o: usize, n: usize| -> Option<u64> {
            let s = b.get(o..o.checked_add(n)?)?;
            Some(if le { s.iter().rev().fold(0u64, |v, &x| v << 8 | x as u64) } else { s.iter().fold(0u64, |v, &x| v << 8 | x as u64) })
        };
        // Field sizes: classic, or BigTIFF's.
        let (big, first) = match at(2, 2) {
            Some(42) => (false, at(4, 4)),
            Some(43) => (true, at(8, 8)),
            _ => return head,
        };
        let (count_len, entry_len, value_len) = if big { (8, 20, 8) } else { (2, 12, 4) };
        head.raw = b.get(8..10) == Some(b"CR");
        let mut seen = std::collections::HashSet::new();
        let mut ifd = first.unwrap_or(0) as usize;
        for k in 0..64 {
            if ifd == 0 || !seen.insert(ifd) {
                break;
            }
            let Some(n) = at(ifd, count_len) else { break };
            let n = n as usize;
            let (mut reduced, mut make, mut subifds, mut dng) = (false, false, false, false);
            for e in 0..n.min(4096) {
                let o = ifd + count_len + e * entry_len;
                let Some(tag) = at(o, 2) else { break };
                match tag {
                    254 => {
                        // LONG, whose value fits the entry; its low bit says
                        // "reduced resolution".
                        let ty = at(o + 2, 2).unwrap_or(4);
                        let v = if ty == 3 { at(o + 4 + value_len, 2) } else { at(o + 4 + value_len, 4) };
                        reduced = v.is_some_and(|v| v & 1 == 1);
                    }
                    // SHORT, in the first two bytes of the value.
                    259 if k == 0 && head.compression.is_none() => head.compression = at(o + 4 + value_len, 2).map(|c| c as u16),
                    271 => make = true,
                    330 => subifds = true,
                    50706 => dng = true,
                    _ => {}
                }
            }
            if k == 0 {
                head.raw |= dng || reduced || (make && subifds);
            }
            head.pictures += !reduced as usize;
            if head.pictures >= 2 {
                break;
            }
            ifd = at(ifd + count_len + n * entry_len, value_len).unwrap_or(0) as usize;
        }
        head
    }
}

/// A TIFF read a strip, or a row of tiles, at a time into the reduction,
/// rather than decoded whole: the format of scans and of sixteen-bit masters,
/// and so of the largest pictures a library holds. Decoded whole, a
/// 12000x12000 picture peaked at 850 MB as an 8-bit TIFF and 1.69 GB as a
/// 16-bit one — the frame, and for sixteen bits a second, RGBA8 copy of it —
/// against 19 MB as a PNG, which was already read a row at a time.
///
/// **It gives the general path's pixels exactly**: the rows `image`'s TIFF
/// decoder would have produced from the same `tiff` decoder, sixteen bits
/// made eight as `to_rgba8` makes them (`row16_to_rgba8`), into the same
/// reduction. `None`, which takes the general path, for anything else: a
/// layout of planes, a sample that is not an unsigned 8- or 16-bit integer,
/// one bit a pixel, a palette, CMYK, YCbCr — and any error, so that a broken
/// file fails with the general path's message. Every chunk is read, the
/// rows the reduction drops included, so a file broken in its last rows
/// fails here as it fails there.
///
/// A sink that would rather have the picture whole at its own depth
/// (`Sink::streams`) is declined too.
pub(super) fn chunks(bytes: &[u8], sink: &mut dyn Sink) -> Option<()> {
    use tiff::decoder::{Decoder, DecodingResult};
    use tiff::tags::Tag;
    use tiff::ColorType as T;
    let mut limits = tiff::decoder::Limits::default();
    let cap = usize::try_from(max_alloc()).unwrap_or(usize::MAX);
    limits.decoding_buffer_size = cap;
    limits.intermediate_buffer_size = cap;
    limits.ifd_value_size = cap;
    let mut dec = Decoder::new(Cursor::new(bytes)).ok()?.with_limits(limits);
    let (w, h) = dec.dimensions().ok()?;
    let (ch, sixteen) = match dec.colortype().ok()? {
        T::Gray(8) => (1, false),
        T::Gray(16) => (1, true),
        T::GrayA(8) => (2, false),
        T::GrayA(16) => (2, true),
        T::RGB(8) => (3, false),
        T::RGB(16) => (3, true),
        T::RGBA(8) => (4, false),
        T::RGBA(16) => (4, true),
        _ => return None,
    };
    if dec.find_tag_unsigned_vec::<u16>(Tag::SampleFormat).ok()?.is_some_and(|f| f.iter().any(|&s| s != 1)) {
        return None;
    }
    if dec.find_tag_unsigned::<u16>(Tag::PlanarConfiguration).ok()?.is_some_and(|p| p != 1) {
        return None;
    }
    let orientation = dec
        .find_tag(Tag::Orientation)
        .ok()
        .flatten()
        .and_then(|v| Orientation::from_exif(v.into_u16().ok()?.min(255) as u8))
        .unwrap_or(Orientation::NoTransforms);
    let (cw, chh) = dec.chunk_dimensions();
    if cw == 0 || chh == 0 {
        return None;
    }
    let (wu, hu) = (w as usize, h as usize);
    let (sw, sh) = shown_size(w, h, orientation);
    if !sink.streams(sw as usize, sh as usize) {
        return None;
    }
    let across = w.div_ceil(cw);
    let down = h.div_ceil(chh);
    // The pixels the sink is handed: the file's own channels at eight bits,
    // or RGBA8 made from sixteen.
    let layout = if sixteen { Layout::Rgba } else { [Layout::Grey, Layout::GreyAlpha, Layout::Rgb, Layout::Rgba][ch - 1] };
    let out_ch = layout.ch();
    let band_bytes = wu * chh as usize * out_ch;
    // The file, the sink's, one band of rows and one chunk of the decoder's
    // (sixteen bits a sample at most).
    let _permit = claim(bytes, band_bytes as u64 + (cw as u64 * chh as u64 * ch as u64 * 2), sink, wu, hu);
    let mut band = vec![0u8; band_bytes];
    let read_band = |dec: &mut Decoder<Cursor<&[u8]>>, band: &mut [u8], r: u32| -> Option<usize> {
        let mut rows = 0;
        for c in 0..across {
            let i = r * across + c;
            let (dw, dh) = dec.chunk_data_dimensions(i);
            let (dw, dh) = (dw as usize, dh as usize);
            rows = dh;
            let x0 = (c * cw) as usize;
            if x0 + dw > wu || dh > chh as usize {
                return None;
            }
            match dec.read_chunk(i).ok()? {
                DecodingResult::U8(v) if !sixteen => {
                    if v.len() < dw * dh * ch {
                        return None;
                    }
                    for y in 0..dh {
                        band[(y * wu + x0) * ch..(y * wu + x0 + dw) * ch].copy_from_slice(&v[y * dw * ch..(y + 1) * dw * ch]);
                    }
                }
                DecodingResult::U16(v) if sixteen => {
                    if v.len() < dw * dh * ch {
                        return None;
                    }
                    for y in 0..dh {
                        let src = v[y * dw * ch..(y + 1) * dw * ch].iter().copied();
                        row16_to_rgba8(src, ch, &mut band[(y * wu + x0) * 4..(y * wu + x0 + dw) * 4]);
                    }
                }
                _ => return None,
            }
        }
        Some(rows)
    };
    let wanted = sink.begin(&Rows { w: wu, h: hu, layout, orientation, grey_of_colour: false });
    let mut next_band = 0u32;
    timed!(1, {
        let (mut band_rows, mut at) = (0usize, 0usize);
        for _ in 0..wanted {
            if at == band_rows {
                if next_band >= down {
                    return None;
                }
                band_rows = read_band(&mut dec, &mut band, next_band)?;
                at = 0;
                next_band += 1;
            }
            sink.row(&band[at * wu * out_ch..(at + 1) * wu * out_ch]);
            at += 1;
        }
    });
    // The rest of the file, read and checked as the general path reads it.
    while next_band < down {
        read_band(&mut dec, &mut band, next_band)?;
        next_band += 1;
    }
    Some(())
}
