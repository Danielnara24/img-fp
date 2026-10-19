//! JPEG: its Y plane where grey will do (`luma`), its own colour planes for
//! its colour (`cells`), and the `image` crate's decode otherwise; its size
//! from its frame header; whether it is cut off; its coding without its
//! metadata.

use super::budget::{cover, max_alloc};
use super::measure::CellColour;
use super::picture::{claim, exif_turns, Facts, Layout, Orientation, Picture, Rows, Sink};
use crate::timed;
use image::ImageFormat;
use std::path::Path;
use zune_core::colorspace::ColorSpace;

/// What `Decoded::damaged` says of a JPEG `cut_off` finds.
const CUT_OFF: &str = "the file ends before its image data does";

pub(super) fn open(bytes: &[u8]) -> Picture<'_> {
    Picture::new(
        move || Facts { damaged: cut_off(bytes).then_some(CUT_OFF), ..Facts::default() },
        move |sink| {
            if sink.grey() && luma(bytes, sink).is_some() {
                return Ok(());
            }
            super::other::whole(bytes, ImageFormat::Jpeg, sink)
        },
    )
}

type Zune<'a> = zune_jpeg::JpegDecoder<zune_core::bytestream::ZCursor<&'a [u8]>>;

/// `zune-jpeg` over `bytes`, decoding to `colour`: as lenient as the `image`
/// crate's own use of it, and with no size limit of its own (`max_alloc` is
/// asked instead).
fn zune(bytes: &[u8], colour: ColorSpace) -> Zune<'_> {
    let opts = zune_core::options::DecoderOptions::default()
        .set_strict_mode(false)
        .set_max_width(usize::MAX)
        .set_max_height(usize::MAX)
        .jpeg_set_out_colorspace(colour);
    zune_jpeg::JpegDecoder::new_with_options(zune_core::bytestream::ZCursor::new(bytes), opts)
}

/// The EXIF orientation a decoder read from the file's first APP1.
fn orientation(dec: &Zune) -> Orientation {
    dec.exif().and_then(|e| Orientation::from_exif_chunk(e)).unwrap_or(Orientation::NoTransforms)
}

/// A JPEG's Y plane, which is the grey value the analysis wants: a JPEG
/// stores luma and chroma, and luma is what `grey_of` computes from the RGB
/// every other format hands it. Asking `zune-jpeg` for it directly skips the
/// chroma upsampling and the colour conversion, and the buffer is one byte a
/// pixel rather than three — measured on 400 of IMGS2's JPEGs, 2.6 s against
/// 3.8 for the same decoder producing RGB.
///
/// `None` means "take the general path": a colour space with no Y plane to
/// read (CMYK, YCCK, or RGB stored as such), a picture past `max_alloc`, and
/// any error at all, so that a broken file fails with the message the general
/// path gives it.
fn luma(bytes: &[u8], sink: &mut dyn Sink) -> Option<()> {
    let mut dec = zune(bytes, ColorSpace::Luma);
    dec.decode_headers().ok()?;
    if !matches!(dec.input_colorspace()?, ColorSpace::YCbCr | ColorSpace::Luma) {
        return None;
    }
    let (w, h) = dec.dimensions()?;
    let n = dec.output_buffer_size()?;
    if n != w * h || n as u64 > max_alloc() {
        return None;
    }
    let grey_of_colour = dec.input_colorspace()? == ColorSpace::YCbCr;
    let orientation = orientation(&dec);
    let _permit = claim(bytes, n as u64, sink, w, h);
    let mut buf = vec![0u8; n];
    timed!(1, dec.decode_into(&mut buf).ok()?);
    timed!(2, {
        let rows = sink.begin(&Rows { w, h, layout: Layout::Grey, orientation, grey_of_colour });
        for line in buf.chunks_exact(w).take(rows) {
            sink.row(line);
        }
    });
    Some(())
}

/// A JPEG's colour cells as `colour_of` measures them, from its own colour
/// planes: Y, Cb and Cr over every pixel of every fourth row, converted to
/// RGB as cell means (`CellColour`), which skips the decoder's own colour
/// conversion; a JPEG with no colour planes is grey in every cell, without a
/// pixel decoded. `None` for anything else, which is read through `preview`.
pub(super) fn cells(bytes: &[u8]) -> Option<(CellColour, Orientation)> {
    let mut dec = zune(bytes, ColorSpace::YCbCr);
    dec.decode_headers().ok()?;
    let orientation = orientation(&dec);
    match dec.input_colorspace()? {
        ColorSpace::Luma => Some((CellColour { n: [1.0; 16], ..Default::default() }, orientation)),
        ColorSpace::YCbCr => {
            let (w, h) = dec.dimensions()?;
            let n = dec.output_buffer_size()?;
            if n != w * h * 3 || (n as u64) > max_alloc() {
                return None;
            }
            let _permit = cover(bytes.len() as u64 + n as u64);
            let mut buf = vec![0u8; n];
            dec.decode_into(&mut buf).ok()?;
            let mut c = CellColour { ycbcr: true, ..Default::default() };
            for y in (0..h).step_by(4) {
                c.add_row(y, h, &buf[y * w * 3..(y + 1) * w * 3], 3, 1);
            }
            Some((c, orientation))
        }
        _ => None,
    }
}

/// The frame size from a JPEG's start-of-frame segment, skipping every
/// segment before it by its length — and turned, when an EXIF segment ahead
/// of the frame says the picture is shown on its side.
pub(super) fn size(r: &mut std::io::BufReader<std::fs::File>) -> Option<(u32, u32)> {
    fn byte(r: &mut impl std::io::Read) -> Option<u8> {
        let mut b = [0u8];
        r.read_exact(&mut b).ok().map(|_| b[0])
    }
    let be16 = |r: &mut std::io::BufReader<std::fs::File>| Some(u16::from_be_bytes([byte(r)?, byte(r)?]));
    // SOI, which `sniff` has already seen.
    be16(r)?;
    let mut turned = None;
    loop {
        if byte(r)? != 0xFF {
            return None;
        }
        let mut m = byte(r)?;
        // Any number of 0xFF fill bytes may precede a marker.
        while m == 0xFF {
            m = byte(r)?;
        }
        // Markers that stand alone, with no length after them.
        if matches!(m, 0x01 | 0xD0..=0xD8) {
            continue;
        }
        let len = be16(r)? as i64;
        // Every SOFn: C0-CF but for DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
            let _precision = byte(r)?;
            let (h, w) = (be16(r)? as u32, be16(r)? as u32);
            return Some(if turned == Some(true) { (h, w) } else { (w, h) });
        }
        if len < 2 {
            return None;
        }
        // The first EXIF segment is the one the decoder reads its orientation
        // from; a later one is ignored there, and so here.
        if m == 0xE1 && turned.is_none() {
            let mut seg = vec![0u8; len as usize - 2];
            std::io::Read::read_exact(r, &mut seg).ok()?;
            if let Some(tiff) = seg.strip_prefix(b"Exif\0\0") {
                turned = Some(exif_turns(tiff));
            }
            continue;
        }
        r.seek_relative(len - 2).ok()?;
    }
}

/// Whether a JPEG stops before its picture does: its segments, read by their
/// lengths, and then its image data, run off the end of the file before the
/// marker that ends the image.
///
/// **This is how a cut-off JPEG is told from a whole one**, and it is asked of
/// the file rather than of the decoder. Decoded leniently, as browsers do, a
/// JPEG cut off at a third of its length is a picture whose last two thirds
/// are grey, and it was analysed and matched as if nothing were wrong —
/// chosen, once, as its group's representative over the intact file it was
/// cut from. PNG, WebP and the rest refuse such a file, so it was a problem in
/// every format but the commonest. The decoder's strict mode says so too, and
/// says it as well of whole files it merely dislikes: one of IMGS's
/// ffmpeg-written JPEGs, which libjpeg and PIL read, was "Bad Huffman Code".
/// The structure is what a truncation breaks, and the structure is unambiguous:
/// inside image data a 0xFF is followed by a stuffed zero, a restart marker or
/// fill, so the first other marker is the next segment, and a file that ends
/// first was cut off. What follows the end of the image — the trailers some
/// cameras append — is not read. Corruption inside data that does reach its
/// end is not caught, and is not what this is for.
///
/// A header this cannot read is the decoder's to judge, and is not called cut
/// off here.
pub(super) fn cut_off(b: &[u8]) -> bool {
    let mut i = 2;
    loop {
        // A marker, after any fill.
        if i >= b.len() {
            return true;
        }
        if b[i] != 0xFF {
            return false;
        }
        while i < b.len() && b[i] == 0xFF {
            i += 1;
        }
        let Some(&m) = b.get(i) else { return true };
        i += 1;
        match m {
            0xD9 => return false,
            0x01 | 0xD0..=0xD8 => continue,
            _ => {}
        }
        let Some(len) = b.get(i..i + 2).map(|l| u16::from_be_bytes([l[0], l[1]]) as usize) else { return true };
        if len < 2 {
            return false;
        }
        i += len;
        if i > b.len() {
            return true;
        }
        if m != 0xDA {
            continue;
        }
        // Image data, to the next marker that is not part of it.
        loop {
            match memchr::memchr(0xFF, &b[i..]) {
                None => return true,
                Some(k) => i += k,
            }
            match b.get(i + 1) {
                None => return true,
                Some(0x00) | Some(0xD0..=0xD7) => i += 2,
                Some(0xFF) => i += 1,
                Some(_) => break,
            }
        }
    }
}

/// Whether two JPEGs hold the same coded picture and differ only in their
/// metadata: the same bytes once every APPn and COM segment is left out of
/// each. Such files decode alike in every channel, which is the common way
/// two JPEGs come to share their grey: a copy with its EXIF stripped or
/// edited. `None` when either is not a JPEG this can walk.
pub fn same_jpeg_coding(a: &Path, b: &Path) -> Option<bool> {
    let (x, y) = (std::fs::read(a).ok()?, std::fs::read(b).ok()?);
    Some(jpeg_coding(&x)? == jpeg_coding(&y)?)
}

/// A JPEG's bytes without its APPn and COM segments, up to and including its
/// first scan's data, which runs to the end of the file.
pub(super) fn jpeg_coding(b: &[u8]) -> Option<Vec<u8>> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut out = Vec::with_capacity(b.len());
    let mut i = 2;
    loop {
        // Fill bytes before a marker are allowed.
        while b.get(i) == Some(&0xFF) && b.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if b.get(i) != Some(&0xFF) {
            return None;
        }
        let marker = *b.get(i + 1)?;
        let len = u16::from_be_bytes([*b.get(i + 2)?, *b.get(i + 3)?]) as usize;
        let end = i.checked_add(2 + len)?;
        if end > b.len() {
            return None;
        }
        let metadata = matches!(marker, 0xE0..=0xEF | 0xFE);
        if marker == 0xDA {
            // The scan header and everything after it: entropy-coded data,
            // and any further scans and segments a progressive file has.
            out.extend_from_slice(&b[i..]);
            return Some(out);
        }
        if !metadata {
            out.extend_from_slice(&b[i..end]);
        }
        i = end;
    }
}
