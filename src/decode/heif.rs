//! HEIF and AVIF, through libheif (`crate::heif`): the primary picture,
//! which libheif turns as the file says, a row at a time from its plane.

use super::budget::{max_alloc, too_large};
use super::picture::{claim, Facts, Layout, Orientation, Picture, Rows, Sink};
use anyhow::Result;
use std::path::Path;

/// A HEIF; more than one picture when its brands name an image sequence.
pub(super) fn open(bytes: &[u8]) -> Result<Picture<'_>> {
    let p = crate::heif::Primary::from_bytes(bytes)?;
    // libheif has limits of its own, but they are a fixed size and not this
    // machine's; the same refusal as every other decoder, from the header.
    too_large("heif", p.width(), p.height(), if p.has_alpha() { 8 } else { 6 }, max_alloc())?;
    Ok(Picture::new(move || Facts { frames: sequence(bytes), exif: super::exif_in(bytes), ..Facts::default() }, move |sink| read(p, bytes, sink)))
}

fn read(p: crate::heif::Primary<'_>, bytes: &[u8], sink: &mut dyn Sink) -> Result<()> {
    let alpha = p.has_alpha();
    let (pw, ph) = (p.width() as usize, p.height() as usize);
    // The decoded interleaved plane, and as much again. The second half used
    // to be a copy of the plane with its stride padding packed out; the rows
    // now go to the sink from where they lie, and the claim is left as it
    // was rather than guessed down to what libheif holds while it converts.
    let _permit = claim(bytes, (pw as u64) * (ph as u64) * if alpha { 8 } else { 6 }, sink, pw, ph);
    let img = p.decode()?;
    let (data, stride, w, h) = img.plane();
    let layout = Layout::of(3, alpha);
    let wanted = sink.begin(&Rows { w, h, layout, orientation: Orientation::NoTransforms, grey_of_colour: false });
    for y in 0..wanted {
        sink.row(&data[y * stride..y * stride + w * layout.ch()]);
    }
    Ok(())
}

/// A HEIF's size from its primary picture's header. Through a reader rather
/// than `read_from_file`, which takes the path as `&str`: a HEIC whose name
/// is not UTF-8 had no size in any report or on its card. libheif reads only
/// what it seeks to.
pub(super) fn size(path: &Path) -> Option<(u32, u32)> {
    let p = crate::heif::Primary::from_file(std::fs::File::open(path).ok()?).ok()?;
    Some((p.width(), p.height()))
}

/// Whether a HEIF's `ftyp` names an image sequence: `msf1`, or the HEVC
/// (`hevc`, `hevx`, `hevs`) or AV1 (`avis`) sequence brands.
pub(super) fn sequence(b: &[u8]) -> bool {
    let Some(len) = b.get(..4).map(|v| u32::from_be_bytes([v[0], v[1], v[2], v[3]]) as usize) else { return false };
    if b.get(4..8) != Some(b"ftyp") || len < 16 {
        return false;
    }
    let brands = b.get(8..len.min(b.len())).unwrap_or(&[]);
    // Major brand, minor version, then compatible brands.
    brands.chunks_exact(4).enumerate().filter(|(k, _)| *k != 1).any(|(_, x)| matches!(x, b"msf1" | b"hevc" | b"hevx" | b"hevs" | b"avis"))
}
