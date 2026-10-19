//! What format a file's bytes are, and whether they are a picture at all:
//! asked of a file's head before the rest of it is read.

use anyhow::{bail, Context, Result};
use image::ImageFormat;
use std::path::Path;

/// A file's format, as its bytes (and, for a format with no signature, its
/// name) say: one `image` crate decoder's, or JPEG XL or HEIF, which have
/// their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image(ImageFormat),
    Jxl,
    Heif,
}

/// What a walk takes when `-x` is not given: every extension this can decode.
/// A file with none at all is left out by default, as in `vid-fp`, and
/// `-x '*'` is how to reach it; see `extensions.rs`.
///
/// Every name a decodable file goes by, not only the commonest: `pam` is the
/// PNM family's own (`P7`, read by the same decoder as `ppm`), `apng` is a PNG
/// whose first frame is the picture, and `dib` is a BMP. All three decoded
/// under `-x '*'` and were passed over by a default walk as "not searched".
/// So were `avifs` and `heics`, the sequence forms, whose first picture
/// libheif reads; the JPEG names `jif`, `jfi`, `pjpeg` and `pjp`; and
/// `jpg_large`, `jpg_orig` and `png_large`, which is how a picture saved from
/// Twitter is named.
pub const EXTENSIONS: [&str; 37] = [
    "jpg", "jpeg", "jpe", "jfif", "jif", "jfi", "pjpeg", "pjp", "jpg_large", "jpg_orig", "png",
    "apng", "png_large", "gif", "webp", "bmp", "dib", "tif", "tiff", "avif", "avifs", "heic",
    "heics", "heif", "hif", "jxl", "ico", "pnm", "pbm", "pgm", "ppm", "pam", "tga", "qoi", "exr",
    "hdr", "ff",
];

/// The error `decode` gives for a macOS metadata file (AppleDouble or
/// AppleSingle, by its first four bytes): the `._` twin a Mac leaves beside a
/// file it copies, which carries the picture's name and none of its pixels.
/// A skip whatever it is called; see `problems::Problems::mac_metadata`.
pub const MAC_METADATA: &str = "a macOS metadata file, not a picture";

/// Whether a file's head is AppleDouble's or AppleSingle's magic number.
fn mac_metadata(head: &[u8]) -> bool {
    matches!(head.get(..4), Some([0x00, 0x05, 0x16, 0x07] | [0x00, 0x05, 0x16, 0x00]))
}

/// The error `decode` gives for bytes that are no picture format at all, as
/// opposed to a picture that would not decode. Under a wildcard walk the first
/// is a file that never claimed to be an image, and is a skip, not a problem.
pub const NOT_AN_IMAGE: &str = "not an image";

/// The format of a file whose head is `head`: by its signature (`sniff`),
/// and failing that by `unmarked_format`. `None` is a file that is no
/// picture this can read, which `decode` refuses with `NOT_AN_IMAGE`.
pub(super) fn identify(head: &[u8], path: &Path) -> Option<Kind> {
    sniff(head).or_else(|| unmarked_format(head, path).map(Kind::Image))
}

/// Identify a format from the first bytes.
///
/// Four of these signatures are two to four bytes long — `BM`, `P1` to `P7`,
/// `00 00 01 00` and `GIF8` — and a text file can start with any of the first
/// two: "BMW service record", "P3 notes". Under `-x '*'` such a file was taken
/// for a broken picture, a problem rather than a skip, and two copies of it
/// were reported as a byte-identical *image*. So for those formats the bytes
/// after the signature have to pass the first check the format's own decoder
/// makes (`plausible`), which is a check every file that decoder can read
/// already passes — the decodable set is exactly what it was.
pub(super) fn sniff(b: &[u8]) -> Option<Kind> {
    match sniff_signature(b)? {
        Kind::Image(fmt) if !plausible(fmt, b) => None,
        kind => Some(kind),
    }
}

fn sniff_signature(b: &[u8]) -> Option<Kind> {
    if b.len() < 12 {
        return None;
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Kind::Image(ImageFormat::Jpeg));
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Kind::Image(ImageFormat::Png));
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Some(Kind::Image(ImageFormat::Gif));
    }
    if b.starts_with(b"RIFF") && &b[8..12] == b"WEBP" {
        return Some(Kind::Image(ImageFormat::WebP));
    }
    if b.starts_with(b"BM") {
        return Some(Kind::Image(ImageFormat::Bmp));
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        return Some(Kind::Image(ImageFormat::Tiff));
    }
    if b.starts_with(&[0xFF, 0x0A]) || b.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return Some(Kind::Jxl);
    }
    if &b[4..8] == b"ftyp" && heif_brands(b) {
        return Some(Kind::Heif);
    }
    if b.starts_with(b"qoif") {
        return Some(Kind::Image(ImageFormat::Qoi));
    }
    if b.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        return Some(Kind::Image(ImageFormat::OpenExr));
    }
    if b.starts_with(b"farbfeld") {
        return Some(Kind::Image(ImageFormat::Farbfeld));
    }
    if b[0] == b'P' && (b'1'..=b'7').contains(&b[1]) {
        return Some(Kind::Image(ImageFormat::Pnm));
    }
    if b.starts_with(&[0, 0, 1, 0]) {
        return Some(Kind::Image(ImageFormat::Ico));
    }
    None
}

/// Whether an ISO BMFF `ftyp` box names a brand libheif reads: HEIC, HEIF or
/// AVIF, still or sequence.
///
/// **Any of its brands, not only the first.** The major brand is the one the
/// writer liked best, and the compatible list is the one a reader is meant to
/// go by: a HEIF file whose major brand is `mif2`, from the format's second
/// edition, lists `mif1` and `heic` after it, and libheif — through
/// ImageMagick and gdk-pixbuf alike — reads it. Judged by the major brand
/// alone it was "not an image" here, and a problem. The list is read only as
/// far as the box and the head both go; a brand past the head is not looked
/// for, and a video's brands (`isom`, `mp41`, `qt  `) name none of these.
fn heif_brands(b: &[u8]) -> bool {
    const HEIF: [&[u8; 4]; 10] = [b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"mif1", b"msf1", b"avif", b"avis"];
    let size = u32::from_be_bytes(b[0..4].try_into().unwrap()) as usize;
    // The major brand, then the minor version (four bytes, not a brand), then
    // the compatible brands to the end of the box.
    let end = size.min(b.len());
    let major = std::iter::once(&b[8..12]);
    let compatible = b.get(16..end.max(16)).into_iter().flat_map(|l| l.chunks_exact(4));
    major.chain(compatible).any(|brand| HEIF.iter().any(|h| &h[..] == brand))
}

/// Whether the bytes after a short signature pass the first test the format's
/// decoder applies to them. Each test is the decoder's own, in `image` 0.25,
/// so nothing the decoder could read is turned away; a head too short to say
/// passes. Formats with a long signature are not asked.
fn plausible(fmt: ImageFormat, b: &[u8]) -> bool {
    match fmt {
        // The DIB header's size, which the BMP decoder reads first and refuses
        // unless it is one of the six it knows.
        ImageFormat::Bmp => match b.get(14..18) {
            Some(v) => matches!(u32::from_le_bytes(v.try_into().unwrap()), 12 | 40 | 52 | 56 | 108 | 124),
            None => true,
        },
        ImageFormat::Pnm => pnm_header_starts(b),
        // At least one directory entry, and the entry's planes and bit depth
        // no more than 256: the ICO decoder's own refusals.
        ImageFormat::Ico => {
            let u16_at = |i: usize| b.get(i..i + 2).map(|v| u16::from_le_bytes(v.try_into().unwrap()));
            u16_at(4).is_none_or(|n| n > 0)
                && u16_at(10).is_none_or(|p| p <= 256)
                && u16_at(12).is_none_or(|d| d <= 256)
        }
        _ => true,
    }
}

/// What a PNM decoder reads after `P1` to `P6`: whitespace and `#` comments,
/// then a digit. After `P7`, a newline and then header lines, the first that
/// is not a comment naming a PAM field.
fn pnm_header_starts(b: &[u8]) -> bool {
    let rest = &b[2..];
    if b[1] == b'7' {
        let Some((&first, mut rest)) = rest.split_first() else { return true };
        if first != b'\n' {
            return false;
        }
        loop {
            let (line, more) = match rest.iter().position(|&c| c == b'\n') {
                Some(k) => (&rest[..k], Some(&rest[k + 1..])),
                // The head ends inside this line: judge what there is of it.
                None => (rest, None),
            };
            if line.first() == Some(&b'#') {
                match more {
                    Some(m) => rest = m,
                    None => return true,
                }
                continue;
            }
            let word = line.trim_ascii_start();
            let word = &word[..word.iter().position(|c| c.is_ascii_whitespace()).unwrap_or(word.len())];
            const FIELDS: [&[u8]; 6] = [b"ENDHDR", b"HEIGHT", b"WIDTH", b"DEPTH", b"MAXVAL", b"TUPLTYPE"];
            return match more {
                Some(_) => FIELDS.contains(&word),
                None => FIELDS.iter().any(|f| f.starts_with(word)),
            };
        }
    }
    let mut comment = false;
    for &c in rest {
        if comment {
            comment = c != b'\n' && c != b'\r';
            continue;
        }
        match c {
            b'#' => comment = true,
            b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r' | b' ' => {}
            c => return c.is_ascii_digit(),
        }
    }
    true
}

/// How much of a file's start decides whether it is a picture at all. `sniff`
/// reads twelve bytes and `image::guess_format` no more than that.
const HEAD: usize = 64;

/// A file's bytes, if its first few say it is a picture; `NOT_AN_IMAGE`
/// without reading the rest if they say it is not.
///
/// Whole files used to be read first and asked second. Under `-x '*'` a walk
/// hands this every file in a folder, and a 1.5 GB video took 1.5 GB of memory
/// to be found not to be a picture — eight at a time, and before the decode
/// budget, which is claimed once a decoder knows the picture's size, could
/// say anything about it. The answer is the same either way: both tests look
/// only at the head of the file.
#[cfg(test)]
pub(super) fn read_if_image(path: &Path) -> Result<Vec<u8>> {
    let (f, head, len) = open_if_image(path)?;
    read_rest(f, head, len, path)
}

/// The open file, its head, and its length, if the head says it is a picture.
pub(super) fn open_if_image(path: &Path) -> Result<(std::fs::File, Vec<u8>, u64)> {
    use std::io::Read;
    let ctx = || format!("read {}", path.display());
    let mut f = std::fs::File::open(path).with_context(ctx)?;
    let mut bytes = Vec::with_capacity(HEAD);
    (&mut f).take(HEAD as u64).read_to_end(&mut bytes).with_context(ctx)?;
    if mac_metadata(&bytes) {
        bail!(MAC_METADATA);
    }
    if !head_is_image(&bytes, path) {
        bail!(NOT_AN_IMAGE);
    }
    let len = f.metadata().map_or(bytes.len() as u64, |m| m.len());
    Ok((f, bytes, len))
}

/// Whether a file's head says it may be a picture: the test `decode` makes
/// before it reads any more of the file, and refuses with `NOT_AN_IMAGE`.
fn head_is_image(head: &[u8], path: &Path) -> bool {
    identify(head, path).is_some()
}

/// `false` when `decode` would turn this file away as `NOT_AN_IMAGE` on its
/// head alone; `true` when it may be a picture, and when the file cannot be
/// read, which is the decode's to report.
///
/// The exact pass asks it of a wildcard walk's same-size files before reading
/// them whole: those are what the pass compares byte for byte, and under
/// `-x '*'` two copies of a 4 GB video are one comparison of 8 GB, for a pair
/// the run then drops as no picture at all.
pub fn may_be_image(path: &Path) -> bool {
    use std::io::Read;
    let mut head = Vec::with_capacity(HEAD);
    match std::fs::File::open(path).and_then(|f| f.take(HEAD as u64).read_to_end(&mut head)) {
        Ok(_) => head_is_image(&head, path),
        Err(_) => true,
    }
}

/// Whether `path` is an existing file whose head says it is a picture, by the
/// same test `decode` turns a file away on. `false` for anything that cannot
/// be opened. The outputs a run writes ask it before they write: see
/// `lib::outputs_are_not_pictures`.
pub fn is_a_picture(path: &Path) -> bool {
    use std::io::Read;
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
        return false;
    }
    let mut head = Vec::with_capacity(HEAD);
    std::fs::File::open(path).and_then(|f| f.take(HEAD as u64).read_to_end(&mut head)).is_ok() && head_is_image(&head, path)
}

/// The rest of a file `open_if_image` has begun.
pub(super) fn read_rest(mut f: std::fs::File, mut bytes: Vec<u8>, len: u64, path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    bytes.reserve_exact((len as usize).saturating_sub(bytes.len()));
    f.read_to_end(&mut bytes).with_context(|| format!("read {}", path.display()))?;
    Ok(bytes)
}

/// The format of a file `sniff` could not place: whatever the image crate
/// recognises in its first bytes, and failing that, a format that has no
/// signature to recognise, taken from the file's name.
///
/// TGA is the one that matters. It carries no magic at all, so
/// `guess_format` can never name it, and every `.tga` — a default extension —
/// was refused as "not an image" without its decoder being asked. The name is
/// consulted only for formats like that, so a `.png` holding something else
/// is still judged by its bytes.
pub(super) fn unmarked_format(bytes: &[u8], path: &Path) -> Option<ImageFormat> {
    // Not a format `sniff` judges: it has refused those bytes already, and
    // `guess_format` would take them back on the same two bytes.
    //
    // Nor one this build has no decoder for. `guess_format` knows DDS, PCX and
    // others whose features are not compiled in, and naming one here made a
    // texture under `-x '*'` a picture that would not decode — a problem and
    // exit 2 — where it is a file that is not an image this build can read.
    if let Ok(fmt) = image::guess_format(bytes) {
        if fmt.reading_enabled() && !matches!(fmt, ImageFormat::Bmp | ImageFormat::Pnm | ImageFormat::Ico | ImageFormat::Gif) {
            return Some(fmt);
        }
    }
    ImageFormat::from_path(path).ok().filter(|f| *f == ImageFormat::Tga)
}
