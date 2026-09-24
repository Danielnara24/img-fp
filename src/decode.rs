//! File discovery and decoding to a single-channel working image.
//!
//! Formats are identified by content, not extension: a file with no extension
//! (or a wrong one) still decodes if its bytes are an image. Every path ends in
//! the same place — a `Gray` of f32 in 0..=1 whose long side is at most the
//! requested working size — so the rest of the pipeline never sees a format.
//!
//! The channel used is the mean of R, G and B rather than a luma weighting.
//! Mean is invariant to any permutation of the channels, which luma is not, so
//! a channel-swapped copy of an image yields the same working image.

use anyhow::{bail, Context, Result};
use crate::timed;
use image::{DynamicImage, ImageDecoder, ImageFormat};
use std::io::Cursor;
use std::path::Path;

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
    /// Long side <= the working size passed to `decode`.
    pub work: Gray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image(ImageFormat),
    Jxl,
    Heif,
    Unknown,
}

const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "heic",
    "heif", "hif", "jxl", "ico", "pnm", "pbm", "pgm", "ppm", "tga", "dds", "qoi", "exr", "ff",
];

/// Cheap pre-filter on the path: known image extension, or no extension at
/// all (which then gets sniffed). Files with an unrelated extension are
/// skipped without being opened.
pub fn looks_like_image(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext = ext.to_ascii_lowercase();
            EXTENSIONS.contains(&ext.as_str())
        }
        None => true,
    }
}

/// Identify a format from the first bytes.
pub fn sniff(b: &[u8]) -> Kind {
    if b.len() < 12 {
        return Kind::Unknown;
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Kind::Image(ImageFormat::Jpeg);
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Kind::Image(ImageFormat::Png);
    }
    if b.starts_with(b"GIF8") {
        return Kind::Image(ImageFormat::Gif);
    }
    if b.starts_with(b"RIFF") && &b[8..12] == b"WEBP" {
        return Kind::Image(ImageFormat::WebP);
    }
    if b.starts_with(b"BM") {
        return Kind::Image(ImageFormat::Bmp);
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        return Kind::Image(ImageFormat::Tiff);
    }
    if b.starts_with(&[0xFF, 0x0A]) || b.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return Kind::Jxl;
    }
    if &b[4..8] == b"ftyp" {
        // ISO BMFF: HEIC/HEIF/AVIF brands all go to libheif.
        let brand = &b[8..12];
        if matches!(
            brand,
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"mif1" | b"msf1"
                | b"avif" | b"avis"
        ) {
            return Kind::Heif;
        }
    }
    if b.starts_with(b"qoif") {
        return Kind::Image(ImageFormat::Qoi);
    }
    if b.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        return Kind::Image(ImageFormat::OpenExr);
    }
    if b.starts_with(b"farbfeld") {
        return Kind::Image(ImageFormat::Farbfeld);
    }
    if b[0] == b'P' && (b'1'..=b'7').contains(&b[1]) {
        return Kind::Image(ImageFormat::Pnm);
    }
    if b.starts_with(&[0, 0, 1, 0]) {
        return Kind::Image(ImageFormat::Ico);
    }
    Kind::Unknown
}

// ---------------------------------------------------------------- decode budget

/// How much decoded image the workers may hold between them.
///
/// A decoder's output is much the largest thing this program allocates: a
/// 44-megapixel photograph is 133 MB of RGB against the 1.2 MB of working
/// image the analysis keeps of it, and the buffer lives only long enough to be
/// reduced. With one worker per thread all reaching for one at once, the peak
/// of a run was decided by how many large files happened to be adjacent in
/// directory order — an accident of the corpus's layout rather than anything
/// about the corpus.
///
/// So the decoders share a budget and a decode waits for room in it. This
/// bounds a transient; it does not limit the work. A file too large for the
/// whole budget still decodes, alone, and no file is skipped, resized or
/// treated differently for it — the output of a run cannot depend on this.
/// The figure is a fraction of what the machine reports free, because how much
/// scratch it is reasonable to hold is a fact about the machine and not about
/// the pictures.
fn decode_budget() -> usize {
    let available = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
        let line = s.lines().find(|l| l.starts_with("MemAvailable:"))?;
        line.split_whitespace().nth(1)?.parse::<usize>().ok()
    });
    match available {
        Some(kb) => (kb / 8 * 1024).max(64 << 20),
        // No /proc to ask: enough for several large photographs at once.
        None => 256 << 20,
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
fn working_bytes(w: usize, h: usize, work: usize) -> u64 {
    let k = box_factor(w, h, work);
    ((w / k).max(1) as u64) * ((h / k).max(1) as u64) * 4
}

/// Claims are served in the order they are made, so that a large one cannot be
/// starved by a stream of small ones slipping past it. Without that, a worker
/// holding a forty-megapixel photograph could wait indefinitely on a machine
/// whose budget is tight, while its seven neighbours took turns.
struct Budget {
    state: std::sync::Mutex<Queue>,
    room: std::sync::Condvar,
    limit: usize,
}

struct Queue {
    held: usize,
    issued: u64,
    serving: u64,
}

static BUDGET: std::sync::LazyLock<Budget> = std::sync::LazyLock::new(|| Budget {
    state: std::sync::Mutex::new(Queue { held: 0, issued: 0, serving: 0 }),
    room: std::sync::Condvar::new(),
    limit: decode_budget(),
});

thread_local! {
    /// How many claims this thread is already holding.
    ///
    /// It is not bookkeeping for its own sake: a worker can re-enter the
    /// budget while holding it, and a thread that waits for room only it can
    /// give back waits forever. See `reserve_inner`.
    static HELD_HERE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// A claim on the decode budget, given back when the buffers it covers die.
struct Permit(usize);

impl Drop for Permit {
    fn drop(&mut self) {
        let mut q = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
        q.held -= self.0;
        drop(q);
        HELD_HERE.with(|h| h.set(h.get() - 1));
        BUDGET.room.notify_all();
    }
}

/// Wait for room for `bytes` of decoded image. A request larger than the whole
/// budget waits for the workers to empty and then proceeds: the alternative is
/// refusing to read a file for being big.
fn reserve(bytes: u64) -> Permit {
    timed!(29, reserve_inner(bytes))
}

fn reserve_inner(bytes: u64) -> Permit {
    let want = bytes.min(isize::MAX as u64) as usize;
    // A thread already holding a claim takes what it asks for and does not
    // queue. It cannot safely wait: the room it would be waiting for is room
    // it is itself holding, and every later ticket queues behind it, so the
    // whole pool stops. That is not hypothetical — a decoder that runs its own
    // work on the global rayon pool leaves its thread free to steal another
    // image's task, and that task decodes, and so claims. `decode_jxl` was
    // exactly this until it was given a pool of its own, and the shape is
    // one library away from coming back.
    //
    // The overshoot is bounded by the nesting depth and costs nothing that
    // matters: this bounds a transient, and a budget that hangs is worse than
    // a budget briefly exceeded.
    if HELD_HERE.with(|h| h.get()) > 0 {
        let mut q = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
        q.held += want;
        drop(q);
        HELD_HERE.with(|h| h.set(h.get() + 1));
        return Permit(want);
    }
    let mut q = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
    let ticket = q.issued;
    q.issued += 1;
    loop {
        if q.serving == ticket && (q.held == 0 || q.held + want <= BUDGET.limit) {
            q.held += want;
            q.serving += 1;
            break;
        }
        q = BUDGET.room.wait(q).unwrap_or_else(|e| e.into_inner());
    }
    drop(q);
    HELD_HERE.with(|h| h.set(h.get() + 1));
    // The next in line may now fit in what is left.
    BUDGET.room.notify_all();
    Permit(want)
}

/// Decode a file to a working-resolution gray image.
pub fn decode(path: &Path, work_size: usize) -> Result<Decoded> {
    let bytes = timed!(0, std::fs::read(path).with_context(|| format!("read {}", path.display()))?);
    let kind = sniff(&bytes);
    let (w, h, gray) = match kind {
        // The two formats that are almost all of a real corpus get their own
        // line in the profile; everything else shares `decode:codec`.
        Kind::Image(ImageFormat::Jpeg) => timed!(22, decode_image_crate(&bytes, ImageFormat::Jpeg, work_size)?),
        Kind::Image(ImageFormat::Png) => timed!(23, decode_image_crate(&bytes, ImageFormat::Png, work_size)?),
        Kind::Image(ImageFormat::WebP) => timed!(24, decode_image_crate(&bytes, ImageFormat::WebP, work_size)?),
        Kind::Image(ImageFormat::Tiff) => timed!(25, decode_image_crate(&bytes, ImageFormat::Tiff, work_size)?),
        Kind::Image(fmt) => decode_image_crate(&bytes, fmt, work_size)?,
        Kind::Jxl => timed!(26, decode_jxl(&bytes, work_size)?),
        Kind::Heif => timed!(27, decode_heif(&bytes, work_size)?),
        Kind::Unknown => {
            // Last resort: let the image crate guess (covers TGA/DDS, which
            // have no magic), then give up.
            match image::guess_format(&bytes) {
                Ok(fmt) => decode_image_crate(&bytes, fmt, work_size)?,
                Err(_) => bail!("not an image"),
            }
        }
    };
    let _ = (w, h);
    Ok(Decoded { work: gray })
}

/// What a file's header says, read without decoding anything: its format and
/// the size of the picture. It exists for the progress bar, which weighs each
/// file by what analysing it will cost (see `progress::analysis_cost`), and
/// nothing about the result depends on it — a file this cannot read is
/// weighed from its size and then decoded, or refused, as usual.
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
pub fn probe(path: &Path) -> Option<Probe> {
    use std::io::{BufRead, BufReader};
    let mut r = BufReader::with_capacity(16 << 10, std::fs::File::open(path).ok()?);
    let head = r.fill_buf().ok()?;
    let kind = sniff(head);
    let (w, h) = match kind {
        Kind::Image(ImageFormat::Jpeg) => jpeg_size(&mut r)?,
        Kind::Image(ImageFormat::Png) => {
            // The signature, then IHDR's length and type, then its width and
            // height; the chunk is required to come first.
            let be = |i: usize| head.get(i..i + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()));
            (be(16)?, be(20)?)
        }
        Kind::Image(fmt) => image::ImageReader::with_format(r, fmt).into_dimensions().ok()?,
        Kind::Jxl => jxl_size(&mut r)?,
        Kind::Heif => {
            let ctx = libheif_rs::HeifContext::read_from_file(path.to_str()?).ok()?;
            let handle = ctx.primary_image_handle().ok()?;
            (handle.width(), handle.height())
        }
        Kind::Unknown => image::ImageReader::new(r).with_guessed_format().ok()?.into_dimensions().ok()?,
    };
    Some(Probe { kind, w, h })
}

/// The frame size from a JPEG's start-of-frame segment, skipping every
/// segment before it by its length.
fn jpeg_size(r: &mut std::io::BufReader<std::fs::File>) -> Option<(u32, u32)> {
    fn byte(r: &mut impl std::io::Read) -> Option<u8> {
        let mut b = [0u8];
        r.read_exact(&mut b).ok().map(|_| b[0])
    }
    let be16 = |r: &mut std::io::BufReader<std::fs::File>| Some(u16::from_be_bytes([byte(r)?, byte(r)?]));
    // SOI, which `sniff` has already seen.
    be16(r)?;
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
            let h = be16(r)?;
            let w = be16(r)?;
            return Some((w as u32, h as u32));
        }
        if len < 2 {
            return None;
        }
        r.seek_relative(len - 2).ok()?;
    }
}

/// A JPEG XL image's size, fed to the decoder a block at a time until the
/// header is complete, which is a few hundred bytes into the file.
fn jxl_size(r: &mut impl std::io::Read) -> Option<(u32, u32)> {
    let mut uninit = jxl_oxide::JxlImage::builder().pool(jxl_oxide::JxlThreadPool::none()).build_uninit();
    let mut buf = vec![0u8; 4096];
    let mut valid = 0usize;
    // An ICC profile can sit ahead of the frame; a megabyte of header is a
    // file this is not going to learn anything cheap about.
    for _ in 0..256 {
        let n = r.read(&mut buf[valid..]).ok()?;
        if n == 0 {
            return None;
        }
        valid += n;
        let used = uninit.feed_bytes(&buf[..valid]).ok()?;
        buf.copy_within(used..valid, 0);
        valid -= used;
        match uninit.try_init().ok()? {
            jxl_oxide::InitializeResult::NeedMoreData(u) => uninit = u,
            jxl_oxide::InitializeResult::Initialized(img) => return Some((img.width(), img.height())),
        }
    }
    None
}

fn decode_image_crate(bytes: &[u8], fmt: ImageFormat, work: usize) -> Result<(u32, u32, Gray)> {
    let reader = image::ImageReader::with_format(Cursor::new(bytes), fmt);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    // The header already says how large the pixels will be. A rotation holds
    // two of them for a moment, since it cannot be done in place, and a
    // channel layout the reduction has no specialisation for is converted to
    // RGBA8 first, which holds another.
    let rotates = !matches!(
        orientation,
        image::metadata::Orientation::NoTransforms
            | image::metadata::Orientation::FlipHorizontal
            | image::metadata::Orientation::FlipVertical
            | image::metadata::Orientation::Rotate180
    );
    let converts = !matches!(
        decoder.color_type(),
        image::ColorType::Rgb8 | image::ColorType::Rgba8 | image::ColorType::L8 | image::ColorType::La8
    );
    let (dw, dh) = decoder.dimensions();
    let _permit = reserve(
        bytes.len() as u64
            + decoder.total_bytes().saturating_mul(1 + rotates as u64)
            + if converts { dw as u64 * dh as u64 * 4 } else { 0 }
            + working_bytes(dw as usize, dh as usize, work),
    );
    let mut img = timed!(1, DynamicImage::from_decoder(decoder)?);
    if orientation != image::metadata::Orientation::NoTransforms {
        img.apply_orientation(orientation);
    }
    let (w, h) = (img.width(), img.height());
    let gray = timed!(2, dynamic_to_gray(&img, work));
    Ok((w, h, gray))
}

/// Mean of RGB, alpha flattened onto mid-grey, box-reduced towards the
/// working size in the same pass so a 50-megapixel file never exists as f32.
fn dynamic_to_gray(img: &DynamicImage, work: usize) -> Gray {
    let (w, h) = (img.width() as usize, img.height() as usize);
    match img {
        DynamicImage::ImageRgb8(b) => reduce_to_gray(w, h, b.as_raw(), 3, false, work),
        DynamicImage::ImageRgba8(b) => reduce_to_gray(w, h, b.as_raw(), 4, true, work),
        DynamicImage::ImageLuma8(b) => reduce_to_gray(w, h, b.as_raw(), 1, false, work),
        DynamicImage::ImageLumaA8(b) => reduce_to_gray(w, h, b.as_raw(), 2, true, work),
        _ => {
            let b = img.to_rgba8();
            reduce_to_gray(w, h, b.as_raw(), 4, true, work)
        }
    }
}

/// Integer box factor that keeps the long side at or above 2x the working
/// size (or 1), so the final area resample still has enough to average.
fn box_factor(w: usize, h: usize, work: usize) -> usize {
    let long = w.max(h);
    if work == 0 {
        return 1;
    }
    (long / (2 * work)).max(1)
}

pub fn reduce_to_gray(w: usize, h: usize, data: &[u8], ch: usize, alpha: bool, work: usize) -> Gray {
    // The channel count is known to the caller and fixed for the whole image,
    // but inside the loop it was a runtime stride and a runtime divisor, which
    // is enough to stop the loop vectorising over a hundred megapixels a
    // minute. Each shape gets its own copy; the arithmetic is unchanged.
    let g = match (ch, alpha) {
        (3, false) => reduce::<3, false>(w, h, data, work),
        (4, true) => reduce::<4, true>(w, h, data, work),
        (1, false) => reduce::<1, false>(w, h, data, work),
        (2, true) => reduce::<2, true>(w, h, data, work),
        (4, false) => reduce::<4, false>(w, h, data, work),
        _ => return reduce_dyn(w, h, data, ch, alpha, work),
    };
    // Edge pixels lost to the floor division are ignored on purpose: at most
    // k-1 rows/cols of a picture already 2x the working size.
    timed!(3, fit_to(g, work))
}

/// One grey sample from one source pixel: the mean of the colour channels,
/// alpha flattened onto mid-grey.
#[inline(always)]
fn grey_of<const CH: usize, const ALPHA: bool>(p: &[u8]) -> f32 {
    let color_ch = if ALPHA { CH - 1 } else { CH };
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

fn reduce<const CH: usize, const ALPHA: bool>(w: usize, h: usize, data: &[u8], work: usize) -> Gray {
    let k = box_factor(w, h, work);
    let ow = (w / k).max(1);
    let oh = (h / k).max(1);
    let inv = 1.0 / (255.0 * (k * k) as f32);
    let mut px: Vec<f32> = Vec::with_capacity(ow * oh);
    // One source row's grey values, so that the interleaved bytes are undone
    // once per row rather than once per box. See `grey_row`.
    let mut grey: Vec<f32> = vec![0.0; w];
    if k == 1 {
        // No box reduction: every output pixel is one source pixel, so there
        // is nothing to accumulate and nothing to zero first.
        //
        // The plane this writes is read straight back by the area resample
        // below, and handing that resample a row at a time instead — so the
        // grey values never leave the first-level cache — is slower, not
        // faster: the resample reads the plane sequentially, which the
        // prefetcher serves for nothing, and a row at a time costs a loop
        // boundary per row of the picture. Measured at +25% on RGB.
        for y in 0..oh {
            let line = &data[y * w * CH..(y + 1) * w * CH];
            grey_row::<CH, ALPHA>(line, &mut grey[..w]);
            px.extend(grey[..ow].iter().map(|g| g * inv));
        }
        return Gray { w: ow, h: oh, px };
    }
    let mut row = vec![0.0f32; ow];
    for oy in 0..oh {
        row.fill(0.0);
        for sy in oy * k..(oy * k + k).min(h) {
            let line = &data[sy * w * CH..(sy + 1) * w * CH];
            grey_row::<CH, ALPHA>(line, &mut grey[..w]);
            box_row(k, &grey[..w], &mut row);
        }
        px.extend(row.iter().map(|v| v * inv));
    }
    Gray { w: ow, h: oh, px }
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
    // Mutated by the vector loop below, which a target without `avx2` does not
    // have.
    #[allow(unused_mut)]
    let mut x = 0usize;
    #[cfg(target_feature = "avx2")]
    if CH == 3 || CH == 4 {
        // Eight pixels a step, and the load takes thirty-two bytes where three
        // channels need twenty-four, so the last few pixels of the row go down
        // the scalar path rather than read past the end of it.
        const STEP: usize = 8;
        while x + STEP <= n && x * CH + 32 <= line.len() {
            unsafe { grey_eight::<CH, ALPHA>(&line[x * CH..], &mut out[x..x + STEP]) };
            x += STEP;
        }
    }
    for x in x..n {
        out[x] = grey_of::<CH, ALPHA>(&line[x * CH..x * CH + CH]);
    }
}

/// Eight pixels' grey values, de-interleaved with shuffles.
///
/// `src` must carry 32 readable bytes. Three- and four-channel layouts have
/// their own path; anything else falls back to the scalar form, which is what
/// `grey_row` would have done anyway.
#[cfg(target_feature = "avx2")]
#[inline]
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
        // The colour channels, summed as integers in the order `grey_of` sums
        // them. A fourth colour channel only exists when there is no alpha.
        let mut sum = _mm256_add_epi32(_mm256_add_epi32(c0, c1), c2);
        let colour_ch = if ALPHA { CH - 1 } else { CH };
        if colour_ch == 4 {
            sum = _mm256_add_epi32(sum, _mm256_srli_epi32(p, 24));
        }
        let mut g = _mm256_cvtepi32_ps(sum);
        if colour_ch != 1 {
            g = _mm256_div_ps(g, _mm256_set1_ps(colour_ch as f32));
        }
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

/// Any other channel layout, with the shape carried at run time. This is the
/// reference: `reduce` is the same arithmetic with the shape known at compile
/// time, and `specialised_reduction_matches_the_general_one` holds the two
/// together.
fn reduce_dyn(w: usize, h: usize, data: &[u8], ch: usize, alpha: bool, work: usize) -> Gray {
    let k = box_factor(w, h, work);
    let ow = (w / k).max(1);
    let oh = (h / k).max(1);
    let mut out = Gray::new(ow, oh);
    let color_ch = if alpha { ch - 1 } else { ch };
    let inv = 1.0 / (255.0 * (k * k) as f32);
    for oy in 0..oh {
        let row = &mut out.px[oy * ow..(oy + 1) * ow];
        for sy in oy * k..(oy * k + k).min(h) {
            let line = &data[sy * w * ch..(sy + 1) * w * ch];
            for ox in 0..ow {
                let mut acc = 0.0f32;
                for sx in ox * k..(ox * k + k).min(w) {
                    let p = &line[sx * ch..sx * ch + ch];
                    let mut v = 0u32;
                    for c in 0..color_ch {
                        v += p[c] as u32;
                    }
                    let mut g = v as f32 / color_ch as f32;
                    if alpha {
                        let a = p[ch - 1] as f32 / 255.0;
                        g = g * a + 128.0 * (1.0 - a);
                    }
                    acc += g;
                }
                row[ox] += acc;
            }
        }
        for v in row.iter_mut() {
            *v *= inv;
        }
    }
    fit_to(out, work)
}

/// Area-resample so the long side is exactly `work` (never upscales).
pub fn fit_to(g: Gray, work: usize) -> Gray {
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
pub fn resize_area(g: &Gray, tw: usize, th: usize) -> Gray {
    if tw == g.w && th == g.h {
        return g.clone();
    }
    let xw = weights(g.w, tw);
    let yw = weights(g.h, th);
    // Horizontal pass. Written once, never zeroed first: every element of it
    // is produced below before anything reads it.
    //
    // An output pixel averages two or three source pixels, so the work per
    // output is a couple of multiply-adds — and around them stood four index
    // bounds to prove, one of them per tap. Taking the taps and the source
    // pixels they read as two slices of equal length proves the lot once. The
    // taps are the same taps, read in the same order.
    let mut tmp: Vec<f32> = Vec::with_capacity(tw * g.h);
    for y in 0..g.h {
        let src = &g.px[y * g.w..(y + 1) * g.w];
        tmp.extend((0..tw).map(|ox| {
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
    // Vertical pass. The first tap writes the row instead of adding to a row
    // of zeros, so the output plane is never zeroed — a megabyte an image that
    // was overwritten immediately. Adding a non-negative product to zero is
    // the product, so the rows that come out are the rows that came out.
    let mut px: Vec<f32> = Vec::with_capacity(tw * th);
    for oy in 0..th {
        let start = yw.start[oy] as usize;
        let (a, b) = (yw.at[oy] as usize, yw.at[oy + 1] as usize);
        let base = px.len();
        let w0 = yw.w[a];
        let s0 = &tmp[start * tw..(start + 1) * tw];
        px.extend(s0.iter().map(|v| v * w0));
        let dst = &mut px[base..base + tw];
        for (i, wgt) in yw.w[a + 1..b].iter().enumerate() {
            let src = &tmp[(start + i + 1) * tw..(start + i + 2) * tw];
            for x in 0..tw {
                dst[x] += src[x] * wgt;
            }
        }
    }
    Gray { w: tw, h: th, px }
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

fn decode_jxl(bytes: &[u8], work: usize) -> Result<(u32, u32, Gray)> {
    let image = jxl_oxide::JxlImage::builder()
        // jxl-oxide enables rayon by default and its default pool is the
        // *global* one — the same pool the per-image walk runs on. A decode
        // that dispatches there leaves its own thread waiting, and a waiting
        // rayon worker steals: the stolen task is another image, which decodes
        // and claims the budget while this thread's claim is still held.
        // Nothing here needs a second level of parallelism anyway; the images
        // are already one per thread.
        .pool(jxl_oxide::JxlThreadPool::none())
        .read(Cursor::new(bytes))
        .map_err(|e| anyhow::anyhow!("jxl: {e}"))?;
    // The rendered frame, the float buffer it is streamed into and the bytes
    // packed out of that: four bytes a sample twice over, then one. Claimed
    // from the header, before the render that allocates the first of them.
    let header = image.image_header();
    let nch = if header.metadata.grayscale() { 1 } else { 3 } + header.metadata.alpha().is_some() as u64;
    let _permit = reserve(
        bytes.len() as u64
            + (image.width() as u64) * (image.height() as u64) * nch * 9
            + working_bytes(image.width() as usize, image.height() as usize, work),
    );
    let render = image
        .render_frame(0)
        .map_err(|e| anyhow::anyhow!("jxl render: {e}"))?;
    let mut stream = render.stream();
    let (w, h, ch) = (stream.width() as usize, stream.height() as usize, stream.channels() as usize);
    let mut buf = vec![0f32; w * h * ch];
    stream.write_to_buffer(&mut buf);
    // Colour channels come first; a trailing channel is alpha if the header
    // says there is one. Anything else (black, spot) is ignored.
    let color = if image.image_header().metadata.grayscale() { 1 } else { 3 };
    let has_alpha = image.image_header().metadata.alpha().is_some();
    let mut u8buf = Vec::with_capacity(w * h * (color + has_alpha as usize));
    for px in buf.chunks_exact(ch) {
        for c in 0..color {
            u8buf.push((px[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
        if has_alpha {
            u8buf.push((px[color].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    let g = reduce_to_gray(w, h, &u8buf, color + has_alpha as usize, has_alpha, work);
    Ok((w as u32, h as u32, g))
}

fn decode_heif(bytes: &[u8], work: usize) -> Result<(u32, u32, Gray)> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_bytes(bytes).map_err(|e| anyhow::anyhow!("heif: {e}"))?;
    let handle = ctx.primary_image_handle().map_err(|e| anyhow::anyhow!("heif: {e}"))?;
    let has_alpha = handle.has_alpha_channel();
    let chroma = if has_alpha { RgbChroma::Rgba } else { RgbChroma::Rgb };
    // The decoded interleaved plane, and the copy packed out of it.
    let _permit = reserve(
        bytes.len() as u64
            + (handle.width() as u64) * (handle.height() as u64) * if has_alpha { 8 } else { 6 }
            + working_bytes(handle.width() as usize, handle.height() as usize, work),
    );
    let img = lib
        .decode(&handle, ColorSpace::Rgb(chroma), None)
        .map_err(|e| anyhow::anyhow!("heif decode: {e}"))?;
    let planes = img.planes();
    let plane = planes.interleaved.context("heif: no interleaved plane")?;
    let (w, h) = (plane.width as usize, plane.height as usize);
    let ch = if has_alpha { 4 } else { 3 };
    // Re-pack rows to remove the stride padding.
    let mut packed = Vec::with_capacity(w * h * ch);
    for y in 0..h {
        let row = &plane.data[y * plane.stride..y * plane.stride + w * ch];
        packed.extend_from_slice(row);
    }
    let g = reduce_to_gray(w, h, &packed, ch, has_alpha, work);
    Ok((w as u32, h as u32, g))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_resize_preserves_mean() {
        let mut g = Gray::new(10, 7);
        for (i, v) in g.px.iter_mut().enumerate() {
            *v = (i % 13) as f32 / 12.0;
        }
        let mean: f32 = g.px.iter().sum::<f32>() / g.px.len() as f32;
        let r = resize_area(&g, 3, 2);
        let m2: f32 = r.px.iter().sum::<f32>() / r.px.len() as f32;
        assert!((mean - m2).abs() < 0.05, "{mean} vs {m2}");
    }

    /// The per-shape copies of the grey reduction exist only to let the
    /// compiler see the stride and the divisor. They must agree with the
    /// general version exactly — not nearly — for every shape a decoder hands
    /// over, at a box factor of one and above.
    #[test]
    fn specialised_reduction_matches_the_general_one() {
        for &(ch, alpha) in &[(1, false), (2, true), (3, false), (4, true), (4, false)] {
            // Widths that matter: each of these leaves a tail of pixels too
            // short for `grey_row`'s eight-at-a-time path, so both halves of it
            // are compared against the reference — and the shapes at and below
            // twice the working size take the `k == 1` branch, which scales the
            // grey values where the other accumulates them.
            for &(w, h, work) in
                &[(37usize, 23usize, 64usize), (200, 150, 32), (64, 64, 0), (100, 80, 64), (121, 97, 64)]
            {
                let data: Vec<u8> = (0..w * h * ch)
                    .map(|i| ((i * 37 + i / 17 * 11) % 251) as u8)
                    .collect();
                let fast = reduce_to_gray(w, h, &data, ch, alpha, work);
                let slow = reduce_dyn(w, h, &data, ch, alpha, work);
                assert_eq!((fast.w, fast.h), (slow.w, slow.h), "{ch} {alpha} {w}x{h}@{work}");
                assert_eq!(fast.px, slow.px, "{ch} {alpha} {w}x{h}@{work}");
            }
        }
    }

    #[test]
    fn sniff_basics() {
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0\0\x10JFIF\0\x01\x01"), Kind::Image(ImageFormat::Jpeg));
        assert_eq!(sniff(b"\0\0\0\x18ftypavif\0\0\0\0"), Kind::Heif);
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Kind::Image(ImageFormat::WebP));
    }
}

// ---------------------------------------------------------------- kernel timings

/// Single-threaded timings of the decode-side inner loops.
/// `cargo test --release -- --ignored --nocapture reduce_timings`
#[cfg(test)]
mod bench {
    use super::*;

    /// The fastest of several runs: the slow ones are the machine's, not the
    /// code's.
    fn ms(f: impl Fn()) -> f64 {
        let mut best = f64::MAX;
        for _ in 0..9 {
            let t = std::time::Instant::now();
            f();
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        best
    }

    #[test]
    #[ignore]
    fn reduce_timings() {
        for &(w, h) in &[(1200usize, 900usize), (4000, 3000)] {
            for &(ch, alpha, name) in &[(3usize, false, "rgb8"), (4, true, "rgba8"), (1, false, "l8")] {
                let data: Vec<u8> = (0..w * h * ch).map(|i| ((i * 37 + i / 101 * 7) % 251) as u8).collect();
                let t = ms(|| {
                    std::hint::black_box(reduce_to_gray(w, h, &data, ch, alpha, 640));
                });
                let k = box_factor(w, h, 640);
                println!("reduce {name} {w}x{h} (k={k}): {t:8.3} ms  -> {:5.1} Mpx/s", (w * h) as f64 / t / 1000.0);
            }
        }
        let mut g = Gray::new(1280, 960);
        for (i, v) in g.px.iter_mut().enumerate() {
            *v = ((i * 37) % 251) as f32 / 251.0;
        }
        let t = ms(|| {
            std::hint::black_box(resize_area(&g, 640, 480));
        });
        println!("resize_area 1280x960 -> 640x480: {t:8.3} ms");
    }
}
