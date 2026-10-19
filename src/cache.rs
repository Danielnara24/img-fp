//! On-disk cache of the per-image analysis.
//!
//! Describing an image is by far the most expensive stage, and it depends only
//! on the file and the extraction settings. Caching it makes a rescan of a
//! mostly-unchanged directory nearly free, and makes tuning the matching
//! stages practical.
//!
//! The format is a header naming the format, then one record per file. A
//! record is keyed by canonical path, size, modification time and change
//! time, so an edited file is re-described rather than trusted, and it says
//! which extraction settings made it.
//!
//! **The settings are a record's, not the file's.** They used to be in the
//! header, and a run at any other `--work-size` found the whole file stale and
//! replaced it with an empty one: one scan at 640 cost the analysis of every
//! folder the machine had ever scanned at 512, silently, and going back to 512
//! cost it again. Now a run reads the records made at its own settings and
//! carries the rest over untouched, as it does a record for a file it did not
//! walk, so the two never mix and neither costs the other anything.
//!
//! **What a record costs, because it is more than people expect.** An analysis
//! is 128 bytes of descriptor and 20 bytes of keypoint for each of up to 600
//! keypoints, plus a thumbnail of up to 128x128. That is ~148 bytes per
//! keypoint, and a *small* photograph is the worst case rather than the best:
//! `upsample_below` enlarges a small picture before describing it, so a
//! 224x224 file yields some 530 keypoints — 95 KB of analysis for a 25 KB
//! JPEG. Nothing about the encoding causes that; it is how much analysis
//! the picture produces, and the knob connected to it is the enlargement, not
//! this file.
//!
//! **So what was left for the encoding was the quarter the old one wasted.**
//! Measured on 300 files of the found corpus, where a record averaged 93.1 KB
//! of which the descriptors were 71.5%, the thumbnail 17.2% and the keypoints
//! 11.2%:
//!
//! - **The descriptors are near-incompressible, and that is a measurement
//!   rather than a guess.** Their bytes carry 5.81 bits of order-0 entropy
//!   each, so deflate reaches 0.758 of raw and there is nowhere much to go:
//!   `zstd -1` gets 0.728, `zstd -19` 0.675 at 12 s for 20 MB, and the best
//!   context model tried — previous bin, orientation index — only lowers the
//!   entropy to 5.48 bits, which is 0.685. Not one descriptor in 159,701 was
//!   a duplicate of another. A range coder would buy 7% of the file for 150
//!   lines that have to be exactly reversible; deflate buys 18% for none.
//! - **The keypoints are five f32 fields and compress as bytes, not as
//!   floats.** Interleaved they are 0.823; split so that all the exponent
//!   bytes are adjacent, all the high mantissa bytes, and so on, 0.763.
//! - **The thumbnail is a picture and wants a picture's predictor.** Raw it
//!   deflates to 0.852 — 128x128 of detail is not repetitive — and Paeth
//!   filtered first, 0.681. (Plain "up" is 0.700 and "left" 0.728.)
//!
//! Together that is **0.755** of the old format on the found corpus and
//! **0.720** on the benchmark one, which is 70 KB and 35 KB an image. The
//! whole of the difference from 1.00 is those three lines; there is no fourth
//! one worth writing.
//!
//! Records unpack in parallel, in batches, and pack on the worker that made
//! them, so neither the clock nor the peak notices — and a run whose records
//! all came from the cache does not write to it at all, which is what keeps a
//! threshold sweep from paying for a deflate of the whole corpus a dozen times
//! over.
//!
//! **A record is appended the moment it exists**, not saved with the rest at
//! the end, so that Ctrl-C costs nothing: whatever the analysis had finished
//! is already in the file, and the interrupt only waits for the one write in
//! flight. See `Store`, and `seal` for what the handler does. The price is
//! that the file can hold records nothing will read — one superseded by a
//! later record for the same path, one for a file that has gone — and the run
//! compacts it when it does, by copying bytes rather than packing them again.

use crate::decode::Traits;
use crate::problems::Problems;
use crate::sift::{Features, Keypoint, DESC_LEN};
use crate::verify::Thumb;
use anyhow::{anyhow, bail, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The last two bytes are the format's version. A file carrying the prefix and
/// another version is a cache this build cannot read, which is a cache of
/// *another version* and not a damaged one: it was written by img-fp, older or
/// newer, and another copy of img-fp on the machine is using it.
///
/// **It is left as it is.** It used to be replaced by an empty file, which
/// is right for an old cache nobody will read again and wrong for the common
/// case: a window from the release and a command line from `cargo install`,
/// one version apart, sharing the default cache, each emptying the other's
/// on every run — a 30 MB cache came out of one run at 145 KB, with exit 0
/// and nothing said. Now the run keeps nothing, says why, and `--clear-cache`
/// or another `--cache` is how to start a new one. Since the default file's
/// name carries the format (`file_name`), that is met only through a
/// `--cache` naming another version's file: two versions never share the
/// default.
///
/// 04: thumbnails are stretched to the full byte range (`Thumb::build`).
/// 05: a small picture is enlarged no further than the working size
/// (`lib::enlarge_below`), which changes every record below 512.
/// 06: a keypoint is four fields, not five: the extractor's `response`, which
/// nothing after extraction reads, is no longer kept on it.
/// 07: a record's key carries the file's change time (see `Key`), and its path
/// is the file's canonical one, so that a folder named two ways is one set of
/// records (see `lib::cache_names`).
/// 08: grey is BT.601 luma rather than the mean of RGB, and a JPEG's is read
/// from its Y plane (`decode::jpeg::luma`).
/// 09: the settings moved from the header into each record, so that one file
/// holds the analyses of several working sizes; and a record carries the
/// picture's own size, which the report's `scale` is stated in.
/// 10: an EXIF orientation is applied to the working plane after the
/// reduction rather than to the decoded picture before it (`decode::picture::orient`),
/// which moves the edge the reduction trims on every turned picture.
/// 11: a record carries the file's `decode::Traits`, which the keep/delete
/// suggestion reads: measured while the file is decoded, and not otherwise
/// to be had from a cached record.
/// 12: a picture wider than 4:3 is analysed with a longer long side, and its
/// thumbnail with one to match (`decode::area_factor`).
/// 13: `Traits::colour` is the colour of a 4x4 grid's cell means, which noise
/// does not reach (`decode::measure::CellColour`); and `Traits::lossless` is said of a
/// WebP with metadata, a JPEG-compressed TIFF and a JPEG XL by what their
/// pictures are (each format's `decode::picture::Facts`). A record of 12 says neither.
/// 14: a record says whether its file holds more than one picture
/// (`decode::Traits::frames`), which a record of 13 cannot, and a 16-bit
/// TIFF or PNG is read as `image` read it but a row at a time.
/// 15: `Traits::lossless` is said of a TGA, an HDR and a farbfeld file as
/// of the other lossless formats; a record of 14 called them lossy.
const MAGIC: &[u8; 8] = b"IMGFPC15";
const MAGIC_PREFIX: &[u8; 6] = b"IMGFPC";

/// The format this build reads and writes, for messages.
pub const FORMAT: &str = match std::str::from_utf8(MAGIC) {
    Ok(s) => s,
    Err(_) => panic!(),
};

/// Records packed or unpacked in one parallel batch. Large enough that the
/// threads are not synchronising over nothing, small enough that a batch of
/// held records is megabytes rather than the whole corpus — the cache of a
/// large corpus is most of a gigabyte and assembling it in memory would double
/// the run's peak for the length of one write.
const BATCH: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
/// What the cached analysis depends on. Only the settings a run can actually
/// change belong here; the detector's own constants are compiled in, so a
/// binary that changes them changes the magic instead.
pub struct Settings {
    pub work_size: u32,
    pub features: u32,
    pub thumb: u32,
}

pub struct Record {
    pub feats: Features,
    pub thumb: Thumb,
    /// The picture's own size, as shown (EXIF orientation applied); `feats`
    /// holds the working image's.
    pub dims: (u32, u32),
    pub traits: Traits,
}

/// What the loaded cache says about one path: its key, its analysis — only
/// for a path this run walks, see `open` — and where its record sits in the
/// file.
pub type Entry = (Key, Option<Record>, Span);

/// Which paths this run walks, asked of every record as it is read.
pub type Walked<'a> = &'a (dyn Fn(&Path) -> bool + Sync);

/// What says a file is still the file a record describes: its size, its
/// modification time and its change time, in nanoseconds.
///
/// **The change time is the one that cannot be put back.** Size and mtime
/// are both under the user's control — `cp -p`, `rsync -a`, `touch -r` and
/// `exiftool -P` all restore a modification time — so a file rewritten with a
/// different picture of the same size kept its record, and two files that
/// were no longer anything alike were reported as a duplicate at correlation
/// 1.00. The kernel sets ctime on every write, rename and chmod, and nothing
/// in userspace sets it back. What it costs is a re-analysis after a chmod, a
/// rename or a new hard link, and every record after a backup restore: a few
/// milliseconds an image, against reporting a pair that is not there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub len: u64,
    pub mtime: i64,
    pub ctime: i64,
}

/// The default cache file's name, which carries the format: `analysis-IMGFPC15.bin`.
///
/// **Each format has a default file of its own**, so that a format change
/// starts a new file beside the old one instead of meeting it. With one name
/// for every format, an upgrade found the old version's file there, left it
/// alone (see the note on `MAGIC`), and so cached nothing on any run until
/// someone thought of `--clear-cache`, which the window's users would not.
/// Now two versions installed side by side each keep their own default, and
/// an upgraded one starts afresh. The price is that an old format's file stays
/// in the directory after nothing reads it; it is not removed, by the user's
/// decision, since telling an unused one from another installed copy's can
/// only be guessed. (Up to `IMGFPC11` the file was `analysis.bin`.)
///
/// A file named with `--cache` is used as named, whatever format it holds.
pub fn file_name() -> String {
    format!("analysis-{FORMAT}.bin")
}

/// Where the cache lives when `--cache` does not say.
///
/// `$XDG_CACHE_HOME/img-fp`, or `~/.cache/img-fp`, and `/tmp/img-fp` for a
/// process with neither — the same order `vid-fp` follows, and the same
/// reason: a cache is regenerable data and this is where the system keeps
/// regenerable data.
fn default_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("img-fp")
}

/// Which file this run's analysis is kept in, and `None` if it cannot be kept
/// at all.
///
/// `--cache` names the FILE, because that is what it is, and because someone
/// pointing at a scratch disk wants to know exactly what appears there. A path
/// that is already a directory, or that is written with a trailing slash, is
/// treated as one and gets the default name inside it: `--cache /mnt/scratch`
/// obviously means "a cache in here", and the alternative is a cache file
/// named `scratch`. Missing parents are created either way — the default
/// location is made for the user, so a named one is too.
///
/// Nothing here is fatal, for the reason `load` gives: a run with no cache
/// finds exactly the same pairs, only slower. A directory that cannot be made
/// is a problem, which is to say it is said out loud and sets the exit code,
/// and then the run goes on without one.
pub fn resolve_path(explicit: Option<&Path>, problems: &mut Problems) -> Option<PathBuf> {
    let path = locate(explicit);
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        if let Err(e) = std::fs::create_dir_all(dir) {
            problems.cache(format!("could not create {}: {e}", dir.display()));
            return None;
        }
    }
    Some(path)
}

/// The file `resolve_path` names, without creating anything.
pub fn locate(explicit: Option<&Path>) -> PathBuf {
    match explicit {
        Some(given) => {
            let names_a_dir = given.is_dir() || given.to_string_lossy().ends_with('/');
            if names_a_dir { given.join(file_name()) } else { given.to_path_buf() }
        }
        None => default_dir().join(file_name()),
    }
}

/// The key of a file whose metadata is in hand: the walk's own, which it has
/// already read for every file it lists. Each later stage used to ask the
/// filesystem again — the exact pass for the size, the cache for the key, the
/// header probe and the report for the size once more — and three of those on
/// one thread, which a network share charges a round trip apiece.
pub fn key_from(md: &std::fs::Metadata) -> Option<Key> {
    use std::os::unix::fs::MetadataExt;
    let mtime = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as i64;
    let ctime = md.ctime().checked_mul(1_000_000_000)?.checked_add(md.ctime_nsec())?;
    Some(Key { len: md.len(), mtime, ctime })
}

// ------------------------------------------------------------ packing

/// The four `Keypoint` fields, in the order a record stores them.
fn field_of(kp: &Keypoint, i: usize) -> f32 {
    match i {
        0 => kp.x,
        1 => kp.y,
        2 => kp.sigma,
        _ => kp.angle,
    }
}
const FIELDS: usize = 4;

/// PNG's Paeth predictor: of the pixel to the left, the one above and the one
/// above-left, whichever is nearest to `a + b - c`.
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i16 + b as i16 - c as i16;
    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// One record's variable-length half: three deflate streams, each carrying
/// data laid out the way its own redundancy runs.
///
/// The three are separate because a deflate stream has one Huffman table and
/// these have three distributions — keypoint bytes, descriptor bytes, and a
/// picture's residuals. Measured on 64 records of the found corpus, one mixed
/// stream is 0.765 of raw where three are 0.753. Grouping the three across a
/// whole batch of records rather than per record is 0.7521 against 0.7526,
/// which is nothing: the streams are long enough already.
fn pack(f: &Features, t: &Thumb) -> Result<Vec<u8>> {
    let n = f.kps.len();
    // The keypoint floats by byte position, so that all the exponent bytes are
    // adjacent, all the high mantissa bytes, and so on. Interleaved they
    // deflate to 0.823 and split to 0.763.
    let mut planes = Vec::with_capacity(n * FIELDS * 4);
    let mut bits: Vec<[u8; 4]> = Vec::with_capacity(n);
    for field in 0..FIELDS {
        bits.clear();
        bits.extend(f.kps.iter().map(|kp| field_of(kp, field).to_le_bytes()));
        for byte in 0..4 {
            planes.extend(bits.iter().map(|v| v[byte]));
        }
    }
    // The thumbnail is a picture, so it gets a picture's predictor: raw it
    // deflates to 0.852, Paeth-filtered to 0.681.
    let (w, h) = (t.w as usize, t.h as usize);
    let mut resid = Vec::with_capacity(w * h);
    let mut prev = vec![0u8; w];
    for y in 0..h {
        let row = &t.px[y * w..(y + 1) * w];
        let (mut left, mut upleft) = (0u8, 0u8);
        for x in 0..w {
            let up = prev[x];
            resid.push(row[x].wrapping_sub(paeth(left, up, upleft)));
            left = row[x];
            upleft = up;
        }
        prev.copy_from_slice(row);
    }
    // The descriptors go as they are. Nothing helps them: see the head of the
    // file for the entropy that says so.
    //
    // At the fastest level. The descriptors are nine tenths of a record and
    // hold nothing for an LZ to find (not one descriptor in 159,701 repeats
    // another), so the longer match search of the default level only spends
    // time: measured on 60 records of IMGS2, level 6 packed to 3,095,465 bytes
    // in 222 ms and level 1 to 3,078,257 bytes in 42 ms — smaller as well as
    // five times faster, since level 1 also gives the Huffman stage longer
    // blocks of literals. Every record is packed by the worker that made it,
    // so on a cold run with the cache on this was some 3 ms an image.
    let mut out = Vec::with_capacity(n * DESC_LEN);
    for stream in [&planes[..], &f.desc[..], &resid[..]] {
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        z.write_all(stream)?;
        let z = z.finish()?;
        out.extend_from_slice(&(z.len() as u64).to_le_bytes());
        out.extend_from_slice(&z);
    }
    Ok(out)
}

/// The inverse, given the shape the record's header already carried.
fn unpack(blob: &[u8], n: usize, tw: u16, th: u16) -> Result<(Vec<Keypoint>, Vec<u8>, Vec<u8>)> {
    let (w, h) = (tw as usize, th as usize);
    let mut at = 0;
    let mut stream = |want: usize| -> Result<Vec<u8>> {
        if at + 8 > blob.len() {
            bail!("record truncated");
        }
        let len = u64::from_le_bytes(blob[at..at + 8].try_into().unwrap()) as usize;
        at += 8;
        if at + len > blob.len() {
            bail!("record truncated");
        }
        let mut raw = Vec::with_capacity(want);
        flate2::read::DeflateDecoder::new(&blob[at..at + len])
            .take(want as u64 + 1)
            .read_to_end(&mut raw)?;
        at += len;
        if raw.len() != want {
            bail!("a record's stream is {} bytes unpacked, not {want}", raw.len());
        }
        Ok(raw)
    };
    let planes = stream(n * FIELDS * 4)?;
    let desc = stream(n * DESC_LEN)?;
    let resid = stream(w * h)?;

    let mut kps = vec![Keypoint { x: 0.0, y: 0.0, sigma: 0.0, angle: 0.0 }; n];
    let mut bits = vec![[0u8; 4]; n];
    for field in 0..FIELDS {
        for byte in 0..4 {
            let plane = &planes[(field * 4 + byte) * n..(field * 4 + byte + 1) * n];
            for (v, &b) in bits.iter_mut().zip(plane) {
                v[byte] = b;
            }
        }
        for (kp, v) in kps.iter_mut().zip(bits.iter()) {
            let f = f32::from_le_bytes(*v);
            match field {
                0 => kp.x = f,
                1 => kp.y = f,
                2 => kp.sigma = f,
                _ => kp.angle = f,
            }
        }
    }
    let mut px = vec![0u8; w * h];
    let mut prev = vec![0u8; w];
    for y in 0..h {
        let (mut left, mut upleft) = (0u8, 0u8);
        let (src, dst) = (&resid[y * w..(y + 1) * w], &mut px[y * w..(y + 1) * w]);
        for x in 0..w {
            let up = prev[x];
            let v = src[x].wrapping_add(paeth(left, up, upleft));
            dst[x] = v;
            left = v;
            upleft = up;
        }
        prev.copy_from_slice(dst);
    }
    Ok((kps, desc, px))
}

/// Writes the flat format a field at a time: into a record's bytes, and into
/// a file's header.
struct Buf<W: Write>(W);
impl<W: Write> Buf<W> {
    fn u32(&mut self, v: u32) -> Result<()> {
        self.0.write_all(&v.to_le_bytes())?;
        Ok(())
    }
    fn u64(&mut self, v: u64) -> Result<()> {
        self.0.write_all(&v.to_le_bytes())?;
        Ok(())
    }
    fn i64(&mut self, v: i64) -> Result<()> {
        self.0.write_all(&v.to_le_bytes())?;
        Ok(())
    }
    fn f32(&mut self, v: f32) -> Result<()> {
        self.0.write_all(&v.to_le_bytes())?;
        Ok(())
    }
    fn bytes(&mut self, v: &[u8]) -> Result<()> {
        self.u64(v.len() as u64)?;
        self.0.write_all(v)?;
        Ok(())
    }
    fn header(&mut self) -> Result<()> {
        self.0.write_all(MAGIC)?;
        Ok(())
    }
}

/// One whole record, as the file holds it.
///
/// The path is written as its own bytes, not as text. A name that is not
/// UTF-8 is still a name, and two of them that read the same once made
/// printable — `x\xfe.jpg` and `x\xff.jpg` — are two files: keyed on the
/// printable form, one was handed the other's analysis.
fn record(path: &Path, s: Settings, k: Key, dims: (u32, u32), tr: &Traits, f: &Features, t: &Thumb) -> Result<Vec<u8>> {
    let blob = pack(f, t)?;
    let path = path.as_os_str().as_bytes();
    let mut b = Buf(Vec::with_capacity(path.len() + 84 + blob.len()));
    b.bytes(path)?;
    b.u32(s.work_size)?;
    b.u32(s.features)?;
    b.u32(s.thumb)?;
    b.u64(k.len)?;
    b.i64(k.mtime)?;
    b.i64(k.ctime)?;
    b.u32(dims.0)?;
    b.u32(dims.1)?;
    for v in tr.detail.iter().chain([&tr.colour]).chain(tr.grid.iter()) {
        b.f32(*v)?;
    }
    b.u32(tr.lossless as u32 | (tr.exif as u32) << 1 | (tr.frames as u32) << 2)?;
    b.u32(f.w)?;
    b.u32(f.h)?;
    b.u32(f.kps.len() as u32)?;
    b.u32(t.w as u32)?;
    b.u32(t.h as u32)?;
    b.f32(t.scale)?;
    b.bytes(&blob)?;
    Ok(b.0)
}

/// `Traits::colour` of a JPEG, which its decode to grey does not measure.
const UNMEASURED: f32 = -1.0;
/// A JPEG's colour as the suggestion measured it later is kept in the same
/// field as `UNKEPT - colour`: below `UNMEASURED`, so that a build of this
/// format that knows nothing of it reads "not measured", as it always did.
const UNKEPT: f32 = -2.0;

/// A measured colour as a record keeps it and gives it back. Not quite the
/// colour, since `UNKEPT - c` rounds; but the same value again through any
/// number of rounds, so a run deciding by it decides as a later run reading
/// it from the record will.
pub fn kept_colour(c: f32) -> f32 {
    UNKEPT - (UNKEPT - c)
}

/// Where one record sits in the cache file, from its path's length to the end
/// of its packed half. What a compaction copies, without unpacking it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    at: u64,
    len: u64,
}

// ------------------------------------------------------------ interruption

/// Held for the length of every append, and by the interrupt handler from the
/// moment it runs until the process is gone. A record is therefore in the
/// file whole or not at all, and nothing is written after the handler has
/// said what the file holds.
static WRITING: Mutex<()> = Mutex::new(());

/// The temporary files a compaction, a fresh cache or a report is being
/// written into, while there are any: the handler removes them, since the
/// files they would have replaced are still there and still whole.
///
/// A list although a run writes one cache at a time. As a single slot, two
/// stores written at once — which the tests do, one per test thread — took
/// each other's entry, and whichever found the slot empty concluded it had
/// been interrupted.
static PARTIAL: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Records appended by this run, for the handler to report.
static APPENDED: AtomicUsize = AtomicUsize::new(0);

fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
    lock_any(m)
}

fn lock_any<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Make the cache file final, for a process about to exit on a signal, and
/// say how many records this run put in it.
///
/// There is nothing to write here, which is the point. Every record went into
/// the file the moment its worker finished it, so an interrupt waits for at
/// most the one append in flight — microseconds into the page cache — and
/// removes a compaction's temporary file if one is half written. The returned
/// guard is held until the process exits, so no worker appends after this.
pub fn seal() -> (MutexGuard<'static, ()>, usize) {
    let held = lock(&WRITING);
    for tmp in lock(&PARTIAL).drain(..) {
        std::fs::remove_file(tmp).ok();
    }
    (held, APPENDED.load(Ordering::SeqCst))
}

/// A temporary file that will replace another once it is written whole:
/// the interrupt handler removes it if it gets there first (`seal`).
pub(crate) fn begin_partial(tmp: &Path) {
    lock(&PARTIAL).push(tmp.to_path_buf());
}

/// Put `tmp`, begun with `begin_partial`, in `dest`'s place. Under the list's
/// lock, so an interrupt either removed it first — and this fails, in a
/// process that is exiting — or finds nothing to remove.
pub(crate) fn finish_partial(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    let mut partial = lock(&PARTIAL);
    let Some(k) = partial.iter().position(|p| p == tmp) else {
        return Err(std::io::Error::other("interrupted"));
    };
    partial.swap_remove(k);
    std::fs::rename(tmp, dest)
}

/// Give up on `tmp`, begun with `begin_partial`, and remove it.
pub(crate) fn abandon_partial(tmp: &Path) {
    let mut partial = lock(&PARTIAL);
    if let Some(k) = partial.iter().position(|p| p == tmp) {
        partial.swap_remove(k);
    }
    std::fs::remove_file(tmp).ok();
}

/// Write a new file at `path` through a temporary one, and hand back the new
/// file open for appending.
///
/// The rename happens under `PARTIAL`'s lock, so the interrupt handler either
/// removes the temporary file before it can be renamed — and the rename then
/// fails, in a process that is exiting — or finds nothing to remove.
fn replace_with(path: &Path, fill: impl FnOnce(&File) -> Result<()>) -> Result<File> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::remove_file(&tmp).ok();
    lock(&PARTIAL).push(tmp.clone());
    // Takes this call's entry out of the list, and says whether it was still
    // there to take — the handler having removed it is how an interrupt shows.
    let unregister = |partial: &mut Vec<PathBuf>| match partial.iter().position(|p| *p == tmp) {
        Some(k) => {
            partial.swap_remove(k);
            true
        }
        None => false,
    };
    let made = (|| -> Result<File> {
        let f = OpenOptions::new().read(true).append(true).create(true).open(&tmp)?;
        // Held shared before it has the cache's name, as every run holds it.
        flock(&f, LOCK_SH);
        fill(&f)?;
        let mut partial = lock(&PARTIAL);
        if !unregister(&mut partial) {
            bail!("interrupted");
        }
        std::fs::rename(&tmp, path)?;
        Ok(f)
    })();
    if made.is_err() {
        // A half-written temporary file is this run's litter, and the next
        // run will not know to clear it: the name carries a process id
        // precisely so that no other run touches it.
        unregister(&mut lock(&PARTIAL));
        std::fs::remove_file(&tmp).ok();
    }
    made
}

// ------------------------------------------------------------ the store

/// Why a cache file produced nothing.
///
/// The two are told apart because they are handled apart. A cache written by
/// a build with another format belongs to that build, and is left alone — see
/// the note on `MAGIC`. A file that is not a cache at all, or that is corrupt
/// in the middle, is a file the user pointed at and will keep pointing at, and
/// it costs a full re-analysis every run until it is noticed.
enum Reject {
    /// Written by a build with another format: its magic, as text.
    OtherVersion(String),
    /// Not a cache, or corrupt: something to say out loud.
    Damaged(anyhow::Error),
}

impl From<anyhow::Error> for Reject {
    fn from(e: anyhow::Error) -> Self {
        Reject::Damaged(e)
    }
}

/// What reading a cache file found at its end.
struct Tail {
    /// Where the last whole record ends.
    end: u64,
    /// Records read, including any that a later record for the same path
    /// superseded.
    count: usize,
    /// The file stops part-way through a record. That is what a process
    /// killed mid-append leaves, and it costs one record, not the cache.
    truncated: bool,
    /// Records that were framed whole but will not unpack, or claim a shape no
    /// record of these settings can have. Each costs itself: its image is
    /// analysed again and the next compaction leaves it out.
    damaged: usize,
    /// Why the file stopped making sense part-way through, when it did for a
    /// reason other than running out: a record's framing is damaged, so no
    /// record after it can be found. The file is cut there, as a torn tail
    /// is, and the records before it are kept.
    broken_at: Option<String>,
}

/// The run's cache file, open for appending for as long as the run has
/// anything to put in it.
///
/// **A record goes into the file the moment its worker has made it**, rather
/// than all of them at the end, and that is what makes an interrupt cheap: a
/// Ctrl-C half an hour into describing a corpus finds the half hour already on
/// disk, and exits without writing anything. The record is packed by the
/// worker that made it, so this costs the run no deflate it did not already
/// pay — it used to be paid in one pass after the analysis — and the file
/// holds every record from that point on, so a rewrite is a copy.
///
/// The file is rewritten only when it holds something the run would not keep:
/// a record superseded by a newer one for the same path, a file that has
/// gone, a `--prune-cache`. Then `compact` copies the records worth keeping
/// into a new file, sorted, byte for byte and without unpacking any of them.
pub struct Store {
    path: PathBuf,
    /// The settings this run's records are made at.
    settings: Settings,
    /// Records made at other settings, the latest for each path and settings:
    /// never unpacked, and carried into a compaction while their file exists.
    others: Vec<(PathBuf, Span)>,
    /// `None` when the cache could not be opened for writing, and the run
    /// keeps nothing.
    file: Option<File>,
    records: AtomicUsize,
    /// Set by the first append that fails; the rest are not attempted.
    failed: Mutex<Option<String>>,
    broken: std::sync::atomic::AtomicBool,
    /// Records that were framed whole and would not unpack. Each is reported
    /// on every run that reads the file, so a file holding one is compacted
    /// whatever else it holds; see `worth_compacting`.
    damaged: usize,
    /// The file is another format version's, whose magic this is, and is
    /// left as it is; see the note on `MAGIC`.
    other_version: Option<String>,
    /// Where the part of the file this run has read ends. Anything after it
    /// was appended since: by this run (`mine`), or by another run sharing
    /// the file (`foreign`).
    read_end: u64,
    /// Where each record this run appended begins.
    mine: Mutex<std::collections::HashSet<u64>>,
    /// Where each record a compaction by this run kept moved to, by where it
    /// was; `None` when there was none. A record left out is in neither.
    moved: Option<HashMap<u64, u64>>,
    /// The file again, open for writing in place (`keep_colour`): `pwrite`
    /// on a descriptor opened to append writes at the end on Linux, whatever
    /// offset it is given.
    in_place: std::sync::OnceLock<Option<File>>,
}

/// Open the cache at `path`, read what is usable out of it, and keep it open
/// for this run's own records.
///
/// Every route out of here returns records rather than an error, because the
/// cache is an optimisation: a run whose cache is missing, stale or ruined
/// computes exactly the same pairs, only slower. What it must not do is be
/// silent about the last of those.
///
/// The file is read a record at a time rather than slurped. A large corpus's
/// cache is most of a gigabyte, and the records it holds are about to exist a
/// second time as `Record`s; holding the bytes as well, for the length of the
/// parse, is a copy the run does not have to make.
///
/// **Only the records for paths this run walks are unpacked.** The file is one
/// per machine, so it holds every folder ever scanned, and a record for a path
/// this run is not looking at is wanted for one thing: to be carried into the
/// file this run leaves behind, which needs its key, its span and nothing
/// else. Unpacking it anyway made every other corpus's analysis exist in full
/// for the length of this run's own — about 900 MB for a nine-thousand-image
/// folder, held while a five-thousand-image one was being analysed. Such a
/// record is framed and skipped, and comes back with no `Record`. What that
/// gives up is noticing a record whose compressed body is damaged before a run
/// that walks its path: the framing is still checked here, and the body is
/// checked the first time anything unpacks it.
///
/// A file another format version wrote is left as it is, and the run keeps
/// nothing (see the note on `MAGIC`). A damaged one is replaced by an empty one straight away,
/// rather than at the end of the run, since this run's records go into
/// whatever file it holds from here on.
pub fn open(path: &Path, want: Settings, walked: Walked, problems: &mut Problems) -> (HashMap<PathBuf, Entry>, Store) {
    let mut out = HashMap::new();
    let mut store = Store {
        path: path.to_path_buf(),
        settings: want,
        others: Vec::new(),
        file: None,
        records: AtomicUsize::new(0),
        failed: Mutex::new(None),
        broken: Default::default(),
        damaged: 0,
        other_version: None,
        read_end: MAGIC.len() as u64,
        mine: Default::default(),
        moved: None,
        in_place: Default::default(),
    };
    // Several runs may hold the file at once — the window's scan and one in
    // a terminal — and each appends to it as it goes. Appending is safe
    // together (`O_APPEND`, a record a write); replacing the file or cutting
    // its end is not, since another run's records then go into a file that
    // no longer has a name, or are cut in half. So every run holds the file
    // shared for as long as it has it open, and only a run that can hold it
    // alone replaces or cuts it; see `exclusive`.
    for _ in 0..8 {
        let f = match OpenOptions::new().read(true).append(true).open(path) {
            Ok(f) => f,
            // Not being there yet is the first run against this cache, which
            // is every first run and not a problem. Made here, empty; the
            // header goes in below, under the lock.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match OpenOptions::new().read(true).append(true).create_new(true).open(path) {
                    Ok(f) => f,
                    // Another run made it first.
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => {
                        problems.cache(format!("could not write {}: {e}", path.display()));
                        return (out, store);
                    }
                }
            }
            Err(e) => {
                // A cache that can be read and not written still answers this
                // run. It just keeps nothing new, and that is worth saying.
                problems.cache(format!("could not open {} for writing: {e}", path.display()));
                // What it holds is still counted, although nothing will be
                // appended: the caller measures how many records a later one
                // superseded against this, and a count of zero beside a full map
                // was an underflow.
                match File::open(path).map(|f| read_file(&f, path, want, walked, &mut out, &mut store.others, problems)) {
                    Ok(Found::Usable(tail)) => {
                        store.records = AtomicUsize::new(tail.count);
                        store.damaged = tail.damaged;
                    }
                    Ok(Found::OtherVersion(v)) => store.other_version = Some(v),
                    _ => {}
                }
                return (out, store);
            }
        };
        // Waits only while another run replaces or repairs the file, and then
        // the name may lead to its replacement: open that one instead.
        if !flock(&f, LOCK_SH) || !names(&f, path) {
            continue;
        }
        // A file just made, or one a run stopped before its header was in.
        if f.metadata().is_ok_and(|m| m.len() == 0) {
            if !exclusive(&f, path) {
                continue;
            }
            match Buf(&f).header() {
                Ok(()) => {
                    flock(&f, LOCK_SH);
                    store.file = Some(f);
                }
                Err(e) => problems.cache(format!("could not write {}: {e}", path.display())),
            }
            return (out, store);
        }
        match read_file(&f, path, want, walked, &mut out, &mut store.others, problems) {
            Found::Usable(tail) => {
                store.records = AtomicUsize::new(tail.count);
                store.damaged = tail.damaged;
                store.read_end = tail.end;
                // A partial record at the end would sit in front of every
                // record appended after it, and turn a lost record into a
                // damaged file. So would a record whose framing is damaged,
                // which ends the part of the file that can be read.
                if tail.truncated {
                    if !exclusive(&f, path) {
                        // Another run has the file, and what reads as torn may
                        // be its record still being written. This run reads
                        // what it found and appends nothing behind it.
                        return (out, store);
                    }
                    let cut = f.set_len(tail.end);
                    flock(&f, LOCK_SH);
                    if let Err(e) = cut {
                        problems.cache(format!("could not repair {}: {e}", path.display()));
                        return (out, store);
                    }
                }
                store.file = Some(f);
                return (out, store);
            }
            Found::OtherVersion(v) => {
                store.other_version = Some(v);
                return (out, store);
            }
            // Replaced by an empty one, unless another run is using it.
            Found::Unusable => {
                if exclusive(&f, path) {
                    match replace_with(path, |f| Buf(f).header()) {
                        Ok(f) => store.file = Some(f),
                        Err(e) => problems.cache(format!("could not write {}: {e}", path.display())),
                    }
                }
                return (out, store);
            }
        }
    }
    // The name kept leading to a file being replaced: this run keeps nothing.
    (out, store)
}

const LOCK_SH: i32 = 1;
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;

/// `flock(2)` on `f`: whether the lock is held. A blocking request is
/// retried when a signal interrupts it. Only another holder refuses: on a
/// filesystem that has no such locks the answer is yes, and the file is
/// used as it was before there were any.
fn flock(f: &File, op: i32) -> bool {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn flock(fd: i32, op: i32) -> i32;
    }
    loop {
        // SAFETY: `f` is open for the length of the call.
        if unsafe { flock(f.as_raw_fd(), op) } == 0 {
            return true;
        }
        match std::io::Error::last_os_error().kind() {
            std::io::ErrorKind::Interrupted if op & LOCK_NB == 0 => {}
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted => return false,
            _ => return true,
        }
    }
}

/// Whether `path` still names the file `f` has open.
fn names(f: &File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (f.metadata(), std::fs::metadata(path)) {
        (Ok(a), Ok(b)) => (a.dev(), a.ino()) == (b.dev(), b.ino()),
        _ => false,
    }
}

/// Hold `f`, which this run holds shared, alone, if no other run has it open
/// and `path` still names it: what replacing or cutting it asks. Otherwise
/// it is held shared again and the answer is no.
///
/// Never waited for: two runs waiting to hold one file alone would each wait
/// for the other to let go. A run that cannot have it leaves it as it is,
/// which costs at most a compaction put off to the next run.
fn exclusive(f: &File, path: &Path) -> bool {
    // A conversion is not atomic: one refused has given up the shared lock
    // too, so it is taken back.
    if flock(f, LOCK_EX | LOCK_NB) && names(f, path) {
        return true;
    }
    flock(f, LOCK_SH);
    false
}

/// What reading a cache file came to.
enum Found {
    Usable(Tail),
    /// Written by another format version, whose magic this is.
    OtherVersion(String),
    /// Nothing in it is usable, and that has been said.
    Unusable,
}

/// Read `f` into `out`.
fn read_file(
    f: &File,
    path: &Path,
    want: Settings,
    walked: Walked,
    out: &mut HashMap<PathBuf, Entry>,
    others: &mut Vec<(PathBuf, Span)>,
    problems: &mut Problems,
) -> Found {
    let size = f.metadata().map_or(u64::MAX, |m| m.len());
    // A megabyte at a time, and the bodies this run does not unpack skipped
    // inside it rather than copied out (see `read_record`). A smaller buffer
    // turns the skips into seeks, and reads a sixth of the bytes — but the
    // reads are then a chain of small dependent ones that defeat readahead,
    // and a scan of two files against a 1.4 GB cache, page cache cold, took
    // 3.4 s against 1.7 for the whole file read in order. Warm, the page
    // buffer was 0.25 s; this is 0.40-0.51 against 0.70-0.75 before, and
    // cold 1.35-1.61. What would read only the heads is an index, which
    // the format does not have.
    match read_stream(std::io::BufReader::with_capacity(1 << 20, f), size, want, walked, out, others) {
        Ok(tail) => {
            if let Some(why) = &tail.broken_at {
                problems.cache(format!(
                    "{} is damaged part-way through ({why}); the {} record(s) before the damage are kept",
                    path.display(),
                    tail.count
                ));
            }
            if tail.damaged > 0 {
                problems.cache(format!(
                    "ignoring {} damaged record(s) in {}; their images are analysed again",
                    tail.damaged,
                    path.display()
                ));
            }
            Found::Usable(tail)
        }
        Err(Reject::OtherVersion(v)) => {
            out.clear();
            others.clear();
            Found::OtherVersion(v)
        }
        Err(Reject::Damaged(e)) => {
            problems.cache(format!("ignoring {}: {e}", path.display()));
            out.clear();
            others.clear();
            Found::Unusable
        }
    }
}

impl Store {
    /// Put one analysis into the file, and say where it went. Called by the
    /// workers as they finish; the packing runs outside the lock, and only
    /// the write is inside it.
    pub fn append(&self, path: impl AsRef<Path>, key: Key, dims: (u32, u32), traits: &Traits, f: &Features, t: &Thumb) -> Option<Span> {
        let file = self.file.as_ref()?;
        if self.broken.load(Ordering::Relaxed) {
            return None;
        }
        let wrote = record(path.as_ref(), self.settings, key, dims, traits, f, t).and_then(|rec| {
            let _held = lock(&WRITING);
            let mut w = file;
            w.write_all(&rec)?;
            // Where this file description's offset stands, which in append
            // mode is the end of what it just wrote — whatever another run
            // appending to the same file has done meanwhile.
            let end = w.stream_position()?;
            Ok(Span { at: end - rec.len() as u64, len: rec.len() as u64 })
        });
        match wrote {
            Ok(span) => {
                lock_any(&self.mine).insert(span.at);
                self.records.fetch_add(1, Ordering::Relaxed);
                APPENDED.fetch_add(1, Ordering::Relaxed);
                Some(span)
            }
            Err(e) => {
                if !self.broken.swap(true, Ordering::Relaxed) {
                    *self.failed.lock().unwrap_or_else(PoisonError::into_inner) = Some(e.to_string());
                }
                None
            }
        }
    }

    /// The format the file was written in, when it is another version's and
    /// this run therefore reads nothing from it and keeps nothing in it.
    pub fn other_version(&self) -> Option<&str> {
        self.other_version.as_deref()
    }

    /// Records the file holds, whether or not anything will keep them.
    pub fn records(&self) -> usize {
        self.records.load(Ordering::Relaxed)
    }

    /// The first append that failed, if one did.
    pub fn failure(&self) -> Option<String> {
        self.failed.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    #[cfg(test)]
    pub fn writable(&self) -> bool {
        self.file.is_some() && !self.broken.load(Ordering::Relaxed)
    }

    /// The records made at other settings, as `open` found them: the latest
    /// for each path and settings.
    pub fn other_settings(&self) -> &[(PathBuf, Span)] {
        &self.others
    }

    /// Whether rewriting the file to hold `entries` is worth what it costs.
    ///
    /// **A rewrite loses nothing and re-describes nothing**: it copies the
    /// records worth keeping, byte for byte. What it costs is that copy, and
    /// it used to be paid for any record at all that nothing would read again
    /// — one deleted photograph made the next run copy every other folder's
    /// analysis to drop it, 263 MB for one record of 9 KB, and the window's
    /// first scan after a trip to the Trash paid it every time.
    ///
    /// So the dead records stay in the file until they are a quarter of it.
    /// That is not a figure read off a corpus; it is the trade every
    /// log-structured store makes, and a quarter states both sides of it: the
    /// file is never more than a third larger than what it holds, and a
    /// rewrite copies at most three bytes worth keeping for every byte it
    /// drops. A dead record is harmless meanwhile — a later record for the
    /// same path replaces it as the file is read, and one for a file that has
    /// gone is passed over as it was. Two things compact whatever the share:
    /// `--prune-cache`, which asks for it, and a damaged record, which is
    /// otherwise reported on every run.
    pub fn worth_compacting(&self, entries: &[(&Path, Span)], prune: bool) -> bool {
        let Some(f) = &self.file else { return false };
        if self.records() == entries.len() || self.broken.load(Ordering::Relaxed) {
            return false;
        }
        if prune || self.damaged > 0 {
            return true;
        }
        let Ok(size) = f.metadata().map(|m| m.len()) else { return true };
        // The records' bytes, the header aside.
        let held = size.saturating_sub(MAGIC.len() as u64);
        // Another run's records, appended since this one read the file, are
        // as live as this run's own: they are its analysis.
        let live: u64 = entries.iter().map(|e| e.1.len).chain(self.foreign().iter().map(|e| e.1.len)).sum();
        held.saturating_sub(live) * 4 >= held
    }

    /// Rewrite the file to hold `entries` and nothing else, sorted, so that
    /// the same corpus in the same state compacts to the same bytes.
    ///
    /// Every entry is already in the file, so this is a copy: nothing is
    /// unpacked or deflated again, and an interrupt part-way through leaves
    /// the file it was copying from exactly as it was.
    ///
    /// `false` when it was put off: not while another run has the file open,
    /// since its records would go on into the file this replaces, which by
    /// then has no name. A run that has the file alone does it.
    ///
    /// The records another run appended since this one read the file are
    /// kept too. That run has finished — nothing else holds the file — and
    /// what it described is in nothing this run was handed: dropped, a scan
    /// in the window lost the analysis of a folder a terminal had scanned
    /// meanwhile, and its bytes, counted as dead, were what made the copy
    /// look worth making.
    pub fn compact(&mut self, entries: &[(&Path, Span)]) -> Result<bool> {
        let Some(src) = &self.file else { return Ok(false) };
        if !exclusive(src, &self.path) {
            return Ok(false);
        }
        let foreign = self.foreign();
        let mut entries: Vec<(&Path, Span)> = entries.iter().copied().chain(foreign.iter().map(|(p, s)| (p.as_path(), *s))).collect();
        // By the path's bytes, which is the order the text sort gave before
        // paths were kept as bytes; and two records for one path in the
        // order they were written, since the later is the one a reader keeps.
        // Two are possible only when another run described a file this run
        // also has a record for.
        entries.sort_unstable_by(|a, b| a.0.as_os_str().cmp(b.0.as_os_str()).then(a.1.at.cmp(&b.1.at)));
        let mut moved = HashMap::with_capacity(entries.len());
        let new = replace_with(&self.path, |f| {
            let mut b = Buf(std::io::BufWriter::with_capacity(1 << 20, f));
            b.header()?;
            let mut buf = Vec::new();
            let mut to = MAGIC.len() as u64;
            for &(_, s) in entries.iter() {
                buf.resize(s.len as usize, 0);
                src.read_exact_at(&mut buf, s.at)?;
                b.0.write_all(&buf)?;
                moved.insert(s.at, to);
                to += s.len;
            }
            b.0.flush()?;
            Ok(())
        })?;
        self.moved = Some(moved);
        self.read_end = new.metadata().map_or(u64::MAX, |m| m.len());
        self.file = Some(new);
        self.records = AtomicUsize::new(entries.len());
        lock_any(&self.mine).clear();
        Ok(true)
    }

    /// Keep `colour`, a JPEG's colour as `decode::colour_of` measured it and
    /// `kept_colour` gave it back, in the record at `span` for `path`, where
    /// the next run reads it as `Traits::colour_kept`.
    ///
    /// **The suggestion decodes a JPEG whole to learn its colour**, which its
    /// analysis, decoded to grey, never measured; and nothing kept the answer,
    /// so every rescan decoded every JPEG the suggestion asked about again: a
    /// cached rescan of 120 grey JPEGs spent 0.8 s there against 0.1 s for the
    /// same pictures as PNG, four times the CPU of the rest of the run. The
    /// answer is a fact about the file's bytes, which the record's key
    /// vouches for, so it goes into the record: four bytes written in place
    /// over the decode's "not measured", in the one field that means colour.
    ///
    /// Written only over a record that is where it is said to be, for that
    /// path, still saying "not measured"; anything else is left as it is,
    /// and costs no more than a decode next time. A record a compaction by
    /// this run left out is not written at all. Under the append lock, so an
    /// interrupt finds nothing half written after it.
    pub fn keep_colour(&self, span: Span, path: &Path, colour: f32) {
        let Some(src) = &self.file else { return };
        if self.broken.load(Ordering::Relaxed) {
            return;
        }
        let at = match &self.moved {
            Some(m) => match m.get(&span.at) {
                Some(&at) => at,
                None => return,
            },
            None => span.at,
        };
        let Some(dst) = self
            .in_place
            .get_or_init(|| {
                use std::os::unix::fs::MetadataExt;
                let f = OpenOptions::new().write(true).open(&self.path).ok()?;
                let (a, b) = (f.metadata().ok()?, src.metadata().ok()?);
                ((a.dev(), a.ino()) == (b.dev(), b.ino())).then_some(f)
            })
            .as_ref()
        else {
            return;
        };
        let name = path.as_os_str().as_bytes();
        // The path's length and the path, then settings (12), key (24),
        // size (8) and detail (24), and the colour.
        let field = at + 8 + name.len() as u64 + 12 + 24 + 8 + 24;
        if field + 4 > at + span.len {
            return;
        }
        let mut head = vec![0u8; 8 + name.len()];
        let mut was = [0u8; 4];
        if src.read_exact_at(&mut head, at).is_err() || src.read_exact_at(&mut was, field).is_err() {
            return;
        }
        if head[..8] != (name.len() as u64).to_le_bytes() || head[8..] != *name || was != UNMEASURED.to_le_bytes() {
            return;
        }
        let _held = lock(&WRITING);
        let _ = dst.write_all_at(&(UNKEPT - colour).to_le_bytes(), field);
    }

    /// The records after `read_end` that this run did not append: another
    /// run's, made while this one had the file open. Read by `pread`, so the
    /// descriptor's own offset, which the appends go through, is not moved.
    /// A record being written as this reads, or cut off, ends the list.
    fn foreign(&self) -> Vec<(PathBuf, Span)> {
        let Some(f) = &self.file else { return Vec::new() };
        let Ok(size) = f.metadata().map(|m| m.len()) else { return Vec::new() };
        let mine = lock_any(&self.mine);
        let mut r = Rd { r: At { f, pos: self.read_end }, pos: self.read_end, size, ended: false };
        let mut out = Vec::new();
        loop {
            let at = r.pos;
            match read_record(&mut r, |_| false) {
                Ok(Some((head, _))) if !mine.contains(&at) => out.push((head.path, Span { at, len: r.pos - at })),
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        out
    }
}

/// A file read from an offset with `pread`; see `Store::foreign`.
struct At<'a> {
    f: &'a File,
    pos: u64,
}

impl Read for At<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.f.read_at(buf, self.pos)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Skip for At<'_> {
    fn skip(&mut self, n: u64) -> std::io::Result<()> {
        self.pos += n;
        Ok(())
    }
}

/// One record's fixed-size half, which is what the reader needs before it can
/// make sense of the packed half.
struct Head {
    path: PathBuf,
    settings: Settings,
    key: Key,
    dims: (u32, u32),
    traits: Traits,
    w: u32,
    h: u32,
    n: usize,
    tw: u32,
    th: u32,
    scale: f32,
}

/// A reader that knows where it is, how long the file is, and whether it ran
/// out of file.
struct Rd<R: Read> {
    r: R,
    pos: u64,
    size: u64,
    ended: bool,
}
impl<R: Read> Rd<R> {
    /// Whether `n` more bytes are in the file, asked before anything is
    /// allocated to hold them. A length read from the file is only as good
    /// as the file, and one damaged into a petabyte used to be handed to the
    /// allocator, which aborts the process rather than fail — on every run,
    /// until the cache was deleted by hand. One that runs past the end is
    /// what a torn append leaves too, and it is treated the same way.
    fn need(&mut self, n: u64) -> Result<()> {
        if self.pos.checked_add(n).is_none_or(|end| end > self.size) {
            self.ended = true;
            bail!("cache truncated");
        }
        Ok(())
    }
    fn fill(&mut self, v: &mut [u8]) -> Result<()> {
        match self.r.read_exact(v) {
            Ok(()) => {
                self.pos += v.len() as u64;
                Ok(())
            }
            Err(e) => {
                self.ended = e.kind() == std::io::ErrorKind::UnexpectedEof;
                Err(anyhow!("cache truncated"))
            }
        }
    }
    /// Pass over `n` bytes without reading them, which `need` has already
    /// found are in the file.
    fn skip(&mut self, n: u64) -> Result<()>
    where
        R: Skip,
    {
        self.r.skip(n).map_err(|_| anyhow!("cache truncated"))?;
        self.pos += n;
        Ok(())
    }
    fn take(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut v = vec![0u8; n];
        self.fill(&mut v)?;
        Ok(v)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut v = [0u8; N];
        self.fill(&mut v)?;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.arr()?))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    /// The one place a clean end of file is expected rather than an error.
    fn len_or_eof(&mut self) -> Result<Option<usize>> {
        let mut v = [0u8; 8];
        let mut got = 0;
        while got < 8 {
            match self.r.read(&mut v[got..]) {
                Ok(0) if got == 0 => return Ok(None),
                Ok(0) => {
                    self.ended = true;
                    bail!("cache truncated")
                }
                Ok(k) => got += k,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.pos += 8;
        Ok(Some(u64::from_le_bytes(v) as usize))
    }
}

/// A reader that can pass over bytes it has no use for. A `BufReader` over a
/// file moves within its buffer when it can and seeks when it cannot; a plain
/// `Seek` would discard the buffer every time.
trait Skip {
    fn skip(&mut self, n: u64) -> std::io::Result<()>;
}

impl<R: Read + Seek> Skip for std::io::BufReader<R> {
    fn skip(&mut self, n: u64) -> std::io::Result<()> {
        self.seek_relative(i64::try_from(n).map_err(std::io::Error::other)?)
    }
}

/// A record's path is written before its shape, so a file claiming an absurd
/// one must not be allowed to ask for that much memory first.
const MAX_PATH: usize = 1 << 16;

/// One record, and its packed half when `body` asks for it. A record this run
/// will not unpack — made at other settings, or for a path it does not walk —
/// is wanted for its key and its span alone, and its body is passed over
/// rather than copied into a buffer of its own: the file holds every folder
/// the machine has scanned, and every one of them was allocated, copied and
/// dropped on every run, whatever the run was looking at.
fn read_record<R: Read>(r: &mut Rd<R>, body: impl Fn(&Head) -> bool) -> Result<Option<(Head, Vec<u8>)>>
where
    R: Skip,
{
    let Some(len) = r.len_or_eof()? else { return Ok(None) };
    if len > MAX_PATH {
        bail!("record claims a {len}-byte path");
    }
    r.need(len as u64)?;
    let path = PathBuf::from(OsString::from_vec(r.take(len)?));
    let settings = Settings { work_size: r.u32()?, features: r.u32()?, thumb: r.u32()? };
    let key = Key { len: r.u64()?, mtime: r.i64()?, ctime: r.i64()? };
    let dims = (r.u32()?, r.u32()?);
    let mut traits = Traits::default();
    for v in traits.detail.iter_mut().chain([&mut traits.colour]).chain(traits.grid.iter_mut()) {
        *v = r.f32()?;
    }
    // A JPEG's colour measured by a later run, kept below the decode's own
    // "not measured" (`keep_colour`).
    if traits.colour < UNMEASURED - 0.5 {
        traits.colour_kept = Some(UNKEPT - traits.colour);
        traits.colour = UNMEASURED;
    }
    let flags = r.u32()?;
    traits.lossless = flags & 1 != 0;
    traits.exif = flags & 2 != 0;
    traits.frames = flags & 4 != 0;
    let (w, h, n) = (r.u32()?, r.u32()?, r.u32()? as usize);
    let (tw, th) = (r.u32()?, r.u32()?);
    let scale = r.f32()?;
    let packed = r.u64()?;
    r.need(packed)?;
    // Held as read, not narrowed, so that `read_stream` can tell a thumbnail
    // side of 65,600 from one of 64.
    let head = Head { path, settings, key, dims, traits, w, h, n, tw, th, scale };
    let blob = if body(&head) {
        r.take(packed as usize)?
    } else {
        r.skip(packed)?;
        Vec::new()
    };
    Ok(Some((head, blob)))
}

fn read_stream<R: Read + Skip>(
    r: R,
    size: u64,
    want: Settings,
    walked: Walked,
    out: &mut HashMap<PathBuf, Entry>,
    others: &mut Vec<(PathBuf, Span)>,
) -> Result<Tail, Reject> {
    let mut r = Rd { r, pos: 0, size, ended: false };
    let magic: [u8; 8] = r.arr().map_err(|_| anyhow!("not a cache file"))?;
    if &magic[..MAGIC_PREFIX.len()] != MAGIC_PREFIX {
        return Err(anyhow!("not a cache file").into());
    }
    if &magic != MAGIC {
        // A cache this build cannot read, written by a build that could. See
        // the note on MAGIC: that is the format changing, not a damaged file.
        return Err(Reject::OtherVersion(String::from_utf8_lossy(&magic).into_owned()));
    }
    // The latest record for each path made at other settings, and where it
    // is in `others`.
    let mut other_at: HashMap<(PathBuf, Settings), usize> = HashMap::new();
    let mut tail = Tail { end: r.pos, count: 0, truncated: false, damaged: 0, broken_at: None };
    let mut batch: Vec<(Head, Vec<u8>, Span)> = Vec::with_capacity(BATCH);
    loop {
        let at = r.pos;
        match read_record(&mut r, |h| h.settings == want && walked(&h.path)) {
            Ok(None) => break,
            Ok(Some((head, blob))) => {
                tail.end = r.pos;
                tail.count += 1;
                // A record framed whole whose shape its own settings cannot
                // produce: its own bytes are damaged, and the records around
                // it are not. It is counted, so that the run sees the file
                // holds something to compact away, and not kept.
                // A thumbnail's side may pass the setting on a picture wider
                // than 4:3 (`decode::area_factor`), but never its area, nor
                // `lib::THUMB_MAX_SIDES` times it.
                let s = head.settings;
                let (tw, th) = (head.tw as u64, head.th as u64);
                let (thumb, sides) = (s.thumb as u64, s.thumb as u64 * crate::THUMB_MAX_SIDES as u64);
                if head.n > s.features as usize || tw * th > thumb * thumb || tw.max(th) > sides {
                    tail.damaged += 1;
                    continue;
                }
                // Made at other settings: kept where it is, for whichever run
                // asks at those settings next.
                if s != want {
                    let span = Span { at, len: r.pos - at };
                    match other_at.entry((head.path, s)) {
                        std::collections::hash_map::Entry::Occupied(e) => others[*e.get()].1 = span,
                        std::collections::hash_map::Entry::Vacant(e) => {
                            others.push((e.key().0.clone(), span));
                            e.insert(others.len() - 1);
                        }
                    }
                    continue;
                }
                batch.push((head, blob, Span { at, len: r.pos - at }));
                if batch.len() == BATCH {
                    tail.damaged += unpack_batch(&mut batch, walked, out);
                }
            }
            Err(_) if r.ended => {
                tail.truncated = true;
                break;
            }
            // The framing is damaged, so nothing after this point can be
            // found. Everything before it is still good, and it used to be
            // thrown away with the rest: the whole file was rejected and
            // replaced, taking every other folder's records with it.
            Err(e) => {
                tail.truncated = true;
                tail.broken_at = Some(e.to_string());
                break;
            }
        }
    }
    tail.damaged += unpack_batch(&mut batch, walked, out);
    Ok(tail)
}

/// Unpacking is deflate plus a thumbnail's mip pyramid, which is real work on
/// a corpus this size, and every record is independent of every other.
///
/// A later record for a path replaces an earlier one: that is a file which
/// changed, and was described again and appended.
///
/// A record that will not unpack is left out and counted, and costs nothing
/// but itself: its image is analysed again, as if it had never been cached.
/// One such record used to reject the whole file — every folder the machine
/// had scanned, for one damaged image.
fn unpack_batch(batch: &mut Vec<(Head, Vec<u8>, Span)>, walked: Walked, out: &mut HashMap<PathBuf, Entry>) -> usize {
    let done: Vec<Result<(PathBuf, Entry)>> = batch
        .par_iter()
        .map(|(h, blob, span)| {
            if !walked(&h.path) {
                return Ok((h.path.clone(), (h.key, None, *span)));
            }
            let (kps, desc, px) = unpack(blob, h.n, h.tw as u16, h.th as u16)?;
            Ok((
                h.path.clone(),
                (
                    h.key,
                    Some(Record {
                        feats: Features { w: h.w, h: h.h, kps: kps.into(), desc: desc.into() }.spilled(),
                        thumb: Thumb::new(h.tw as u16, h.th as u16, h.scale, px).spilled(),
                        dims: h.dims,
                        traits: h.traits,
                    }),
                    *span,
                ),
            ))
        })
        .collect();
    batch.clear();
    let mut damaged = 0;
    for r in done {
        match r {
            Ok((path, rec)) => {
                out.insert(path, rec);
            }
            Err(_) => damaged += 1,
        }
    }
    damaged
}

/// What the loaded cache holds about files this run never looked at, and that
/// are still on disk.
///
/// Every walked file's record has already been taken out of the map by the
/// time this is called, so what is left *is* the untouched set.
///
/// The cache is one file per machine rather than one per corpus, so a run over
/// `~/Pictures` must not throw away what the last run over `~/Downloads` paid
/// for: what the file keeps is this run's analysis *plus* this. Without it a
/// default cache would be worse than none, silently, for anyone who scans two
/// directories in turn.
///
/// The existence check (`gone`) is what keeps the file from growing forever. A record
/// is worth keeping while the file it describes is there to be matched again;
/// once the file is gone the record can never be used, only carried. It is one
/// `stat` per record not seen this run, and it is why `--prune-cache` is a
/// flag rather than the default.
///
/// **A file on a drive that is not plugged in is not gone.** Its records
/// used to count as dead like any other, and once they were a quarter of the
/// file the next scan of anything else compacted them away: a photo library
/// on an external disk, scanned and then unplugged, was analysed from scratch
/// the next time it was plugged in. So a record whose file is missing is kept
/// when the file lies on a volume that is not mounted (`unmounted`).
pub fn carry_over(cached: &HashMap<PathBuf, Entry>) -> Vec<(&Path, Span)> {
    type Kept<'a> = Vec<(&'a Path, Span)>;
    let gone = gone(cached.keys().map(PathBuf::as_path));
    let (mut kept, missing): (Kept, Kept) = cached.iter().map(|(path, e)| (path.as_path(), e.2)).partition(|(path, _)| !gone.contains(path));
    // Each volume asked once, however many of its records are missing.
    let mut volumes: HashMap<PathBuf, bool> = HashMap::new();
    for (path, span) in missing {
        let Some(root) = volume_root(path) else { continue };
        if *volumes.entry(root).or_insert_with_key(|r| unmounted(r)) {
            kept.push((path, span));
        }
    }
    kept
}

/// Which of `paths`, the files records name, are no longer there: the
/// filesystem says so, and not merely that it cannot be asked right now.
///
/// `Path::exists` is false on any error, so a file in a folder made
/// unreadable, on an sshfs mount whose connection dropped ("Transport
/// endpoint is not connected") or behind a stale NFS handle counted as
/// deleted, and once such records were a quarter of the file the next scan
/// of anything else compacted them away. Only a folder's listing without the
/// name, or a folder that is itself no longer there (or no longer a folder),
/// says a file has gone.
///
/// **A folder at a time.** Every run asks this of every record of every
/// folder the cache has seen but the run did not scan, and it was a `stat` a
/// file: a round trip apiece on a network share, for a library there of
/// thousands of photographs on every scan of anything else. A listing of
/// each folder is one or a few, whatever it holds.
pub fn gone<'a>(paths: impl Iterator<Item = &'a Path>) -> std::collections::HashSet<&'a Path> {
    let mut by_folder: HashMap<&Path, Vec<&Path>> = HashMap::new();
    for p in paths {
        by_folder.entry(p.parent().unwrap_or(Path::new("/"))).or_default().push(p);
    }
    by_folder
        .into_par_iter()
        .flat_map_iter(|(folder, files)| {
            let names: Option<std::collections::HashSet<std::ffi::OsString>> = match std::fs::read_dir(folder) {
                Ok(list) => list.map(|e| e.map(|e| e.file_name())).collect::<std::io::Result<_>>().ok(),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => Some(Default::default()),
                Err(_) => None,
            };
            files.into_iter().filter(move |f| names.as_ref().is_some_and(|n| f.file_name().is_none_or(|f| !n.contains(f))))
        })
        .collect()
}

/// The folder a removable drive or a network share holding `path` is mounted
/// at, when `path` lies where systems mount them: `/media/USER/NAME` (udisks
/// on Debian and Ubuntu), `/run/media/USER/NAME` (udisks elsewhere),
/// `/mnt/NAME`, `/run/user/UID/gvfs/NAME` (GNOME's network shares) and
/// `/Volumes/NAME` (macOS).
fn volume_root(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let parts: Vec<&std::ffi::OsStr> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n),
            _ => None,
        })
        .collect();
    if !path.is_absolute() {
        return None;
    }
    let take = |n: usize| (parts.len() > n).then(|| Path::new("/").join(parts[..n].iter().collect::<PathBuf>()));
    match parts.as_slice() {
        [first, ..] if *first == "media" => take(3),
        [first, second, ..] if *first == "run" && *second == "media" => take(4),
        [first, ..] if *first == "mnt" => take(2),
        [first, second, _, gvfs, ..] if *first == "run" && *second == "user" && *gvfs == "gvfs" => take(5),
        [first, ..] if *first == "Volumes" => take(2),
        _ => None,
    }
}

/// Whether nothing is mounted at `root`: it is not there (udisks removes the
/// folder it made), or it is an empty folder (a mount point under `/mnt`
/// stays behind).
fn unmounted(root: &Path) -> bool {
    match std::fs::read_dir(root) {
        Ok(mut entries) => entries.next().is_none(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("img-fp-cache-test-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// `--cache` names the file, except when what it names is obviously a
    /// place to put one. The alternative to this rule is a cache file named
    /// after the scratch disk someone pointed at.
    #[test]
    fn a_named_path_is_a_file_unless_it_is_a_directory() {
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let dir = scratch("named");

        let named_file = dir.join("mine.bin");
        assert_eq!(resolve_path(Some(&named_file), &mut problems).unwrap(), named_file);

        // An existing directory, and a path written as one.
        assert_eq!(
            resolve_path(Some(&dir), &mut problems).unwrap(),
            dir.join(file_name())
        );
        let trailing = PathBuf::from(format!("{}/", dir.join("sub").display()));
        assert_eq!(
            resolve_path(Some(&trailing), &mut problems).unwrap(),
            dir.join("sub").join(file_name())
        );
        // And the parent of a named file is created, as the default one is.
        let deep = dir.join("one/two/three.bin");
        assert_eq!(resolve_path(Some(&deep), &mut problems).unwrap(), deep);
        assert!(dir.join("one/two").is_dir());
        assert!(!problems.any());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The packing is three lossless transforms and a deflate, and every one
    /// of them has an inverse that has to give back the bits it was handed —
    /// a cache that reconstructs an analysis *nearly* would be a tool that
    /// finds different pairs depending on whether it has run before.
    #[test]
    fn a_packed_record_comes_back_bit_for_bit() {
        // Shapes worth covering: a thumbnail one pixel wide (every Paeth
        // predictor degenerates), one with no keypoints at all (the featureless
        // images the corpus really contains), and an ordinary one.
        for (n, w, h) in [(0usize, 1usize, 1usize), (1, 1, 7), (3, 7, 1), (137, 40, 31)] {
            let kps: Vec<Keypoint> = (0..n)
                .map(|i| {
                    let f = i as f32;
                    Keypoint {
                        x: f * 1.37 + 0.5,
                        y: 383.0 - f * 0.919,
                        sigma: 1.6 * (1.0 + f / 64.0),
                        angle: (f * 17.3) % 360.0,
                    }
                })
                .collect();
            // Descriptor and thumbnail bytes that use the whole range, so that
            // a Paeth residual and a byte plane both have something to get
            // wrong at the edges.
            let desc: Vec<u8> = (0..n * DESC_LEN).map(|i| ((i * 37 + i / 128) % 256) as u8).collect();
            let px: Vec<u8> = (0..w * h).map(|i| ((i * 91) % 256) as u8).collect();
            let feats = Features { w: 640, h: 480, kps: kps.clone().into(), desc: desc.clone().into() };
            let thumb = Thumb::new(w as u16, h as u16, 0.25, px);

            let blob = pack(&feats, &thumb).unwrap();
            let (kps, desc, px) = unpack(&blob, n, w as u16, h as u16).unwrap();
            assert_eq!(desc, feats.desc);
            assert_eq!(px, thumb.px);
            assert_eq!(kps.len(), feats.kps.len());
            for (got, want) in kps.iter().zip(feats.kps.iter()) {
                assert_eq!(got.x.to_bits(), want.x.to_bits());
                assert_eq!(got.y.to_bits(), want.y.to_bits());
                assert_eq!(got.sigma.to_bits(), want.sigma.to_bits());
                assert_eq!(got.angle.to_bits(), want.angle.to_bits());
            }
        }
    }

    /// A truncated record is a damaged cache, not a panic and not a silently
    /// short one.
    #[test]
    fn a_short_record_is_rejected_rather_than_trusted() {
        let feats = Features { w: 8, h: 8, ..Default::default() };
        let thumb = Thumb::new(4, 4, 1.0, vec![7; 16]);
        let blob = pack(&feats, &thumb).unwrap();
        assert!(unpack(&blob[..blob.len() - 1], 0, 4, 4).is_err());
        assert!(unpack(&blob, 0, 8, 8).is_err(), "a record that claims more than it holds");
    }

    /// The cache serves every directory the machine scans, so a run writes
    /// back what it did not look at — but only while the file is still there
    /// to be matched again.
    #[test]
    fn carry_over_keeps_what_is_still_on_disk_and_not_this_run() {
        let dir = scratch("carry");
        let here = dir.join("here.jpg");
        std::fs::write(&here, b"x").unwrap();
        let gone = dir.join("gone.jpg");

        let record = || (
            Key { len: 1, mtime: 2, ctime: 2 },
            None,
            Span { at: 0, len: 0 },
        );
        let mut cached = HashMap::new();
        cached.insert(here.clone(), record());
        cached.insert(gone.clone(), record());
        cached.insert(PathBuf::from("scanned-this-run.jpg"), record());

        // The walked file's record is gone from the map before this is
        // called, so what is left is the untouched set.
        cached.remove(Path::new("scanned-this-run.jpg"));
        let kept = carry_over(&cached);
        assert_eq!(kept.len(), 1, "only the untouched file that still exists");
        assert_eq!(kept[0].0, here);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A missing file on a drive that is not mounted keeps its record; the
    /// volume is the folder the drive is mounted at, by where systems mount
    /// them, and it is unmounted when it is not there or holds nothing.
    #[test]
    fn a_record_on_an_unplugged_drive_is_kept() {
        let root = |p: &str| volume_root(Path::new(p));
        assert_eq!(root("/media/u/Photos/2019/a.jpg"), Some("/media/u/Photos".into()));
        assert_eq!(root("/run/media/u/Photos/a.jpg"), Some("/run/media/u/Photos".into()));
        assert_eq!(root("/mnt/nas/a.jpg"), Some("/mnt/nas".into()));
        assert_eq!(root("/run/user/1000/gvfs/smb-share:server=x,share=p/a.jpg"), Some("/run/user/1000/gvfs/smb-share:server=x,share=p".into()));
        assert_eq!(root("/Volumes/Card/DCIM/a.jpg"), Some("/Volumes/Card".into()));
        assert_eq!(root("/home/u/Pictures/a.jpg"), None, "a home folder is not a drive that comes and goes");
        assert_eq!(root("/media/u"), None, "nor is the folder drives are mounted in");
        assert_eq!(root("media/u/Photos/a.jpg"), None);

        let dir = scratch("volumes");
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let full = dir.join("full");
        std::fs::create_dir_all(full.join("DCIM")).unwrap();
        assert!(unmounted(&dir.join("not-there")));
        assert!(unmounted(&empty), "a mount point left behind");
        assert!(!unmounted(&full), "mounted, and the file is gone from it");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The magic, and nothing else: the settings are each record's.
    const HEADER_LEN: u64 = MAGIC.len() as u64;

    const SETTINGS: Settings = Settings { work_size: 384, features: 600, thumb: 128 };

    fn analysis(n: usize, seed: u8) -> (Features, Thumb) {
        let kps = (0..n)
            .map(|i| Keypoint { x: i as f32, y: seed as f32, sigma: 1.6, angle: 0.0 })
            .collect();
        let desc = (0..n * DESC_LEN).map(|i| (i as u8).wrapping_add(seed)).collect();
        (Features { w: 64, h: 48, kps, desc }, Thumb::new(8, 6, 0.125, vec![seed; 48]))
    }

    fn reopen(path: &Path) -> (HashMap<PathBuf, Entry>, Store, bool) {
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = open(path, SETTINGS, &|_| true, &mut problems);
        (got, store, problems.any())
    }

    /// A record for a path the run does not walk is read for its key and its
    /// span, and not unpacked — and a later one still supersedes an earlier.
    #[test]
    fn records_this_run_does_not_walk_are_framed_not_unpacked() {
        let dir = scratch("walked");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f1, t1) = analysis(3, 1);
        let (f2, t2) = analysis(5, 2);
        store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.append("b.jpg", Key { len: 2, mtime: 2, ctime: 2 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.append("b.jpg", Key { len: 3, mtime: 3, ctime: 3 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        drop(store);
        let (all, _, _) = reopen(&path);

        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = open(&path, SETTINGS, &|p| p == Path::new("a.jpg"), &mut problems);
        assert!(!problems.any() && store.records() == 3 && got.len() == 2);
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().feats.desc, f1.desc);
        let b = &got[Path::new("b.jpg")];
        assert!(b.1.is_none());
        assert_eq!((b.0.len, b.2.at, b.2.len), (3, all[Path::new("b.jpg")].2.at, all[Path::new("b.jpg")].2.len));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// What a worker appends, the next run reads — and a later record for the
    /// same path wins, since that is a file described again after it changed.
    #[test]
    fn appended_records_are_read_back_and_the_last_one_wins() {
        let dir = scratch("append");
        let path = dir.join(file_name());
        let (got, store, bad) = reopen(&path);
        assert!(got.is_empty() && !bad);
        let (f1, t1) = analysis(3, 1);
        let (f2, t2) = analysis(5, 2);
        store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.append("b.jpg", Key { len: 2, mtime: 2, ctime: 2 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.append("a.jpg", Key { len: 3, mtime: 3, ctime: 3 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        assert_eq!(store.records(), 3);
        drop(store);

        let (got, store, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!(store.records(), 3, "the superseded record is still in the file");
        assert_eq!(got.len(), 2);
        let a = &got[Path::new("a.jpg")];
        assert_eq!((a.0.len, a.1.as_ref().unwrap().feats.kps.len()), (3, 5));
        assert_eq!(a.1.as_ref().unwrap().feats.desc, f2.desc);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A JPEG's colour, measured after its record was written, is kept in the
    /// record and read back as kept, the decode's own "not measured" left as
    /// it was; through a compaction too; and never over a record that is not
    /// the one named, nor one that already says a colour.
    #[test]
    fn a_colour_measured_later_is_kept_in_the_record() {
        let dir = scratch("colour");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(3, 1);
        let jpeg = Traits { colour: UNMEASURED, ..Default::default() };
        let png = Traits { colour: 7.5, ..Default::default() };
        let a = store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &jpeg, &f, &t).unwrap();
        let b = store.append("b.png", Key { len: 2, mtime: 2, ctime: 2 }, (320, 240), &png, &f, &t).unwrap();
        let c = store.append("c.jpg", Key { len: 3, mtime: 3, ctime: 3 }, (320, 240), &jpeg, &f, &t).unwrap();
        let measured = kept_colour(12.345);
        assert_eq!(kept_colour(measured), measured, "the same value through any number of rounds");
        store.keep_colour(a, Path::new("a.jpg"), measured);
        store.keep_colour(b, Path::new("b.png"), 3.0);
        store.keep_colour(c, Path::new("not-c.jpg"), 3.0);
        drop(store);
        let (got, mut store, bad) = reopen(&path);
        assert!(!bad);
        let traits = |got: &HashMap<PathBuf, Entry>, p: &str| got[Path::new(p)].1.as_ref().unwrap().traits;
        assert_eq!((traits(&got, "a.jpg").colour, traits(&got, "a.jpg").colour_kept), (UNMEASURED, Some(measured)));
        assert_eq!((traits(&got, "b.png").colour, traits(&got, "b.png").colour_kept), (7.5, None), "a measured colour is not written over");
        assert_eq!(traits(&got, "c.jpg").colour_kept, None, "nor a record for another path");

        // Compacted to `c.jpg` and `a.jpg` (in path order), `c.jpg` moves to
        // the front, and is written where it went; `b.png`, left out, is not.
        let entries: Vec<(&Path, Span)> = ["c.jpg", "a.jpg"].iter().map(|p| (Path::new(*p), got[Path::new(*p)].2)).collect();
        assert_eq!(store.compact(&entries).unwrap(), true);
        store.keep_colour(got[Path::new("c.jpg")].2, Path::new("c.jpg"), 2.0);
        store.keep_colour(got[Path::new("b.png")].2, Path::new("b.png"), 2.0);
        drop(store);
        let (again, _, bad) = reopen(&path);
        assert!(!bad && again.len() == 2);
        assert_eq!(traits(&again, "c.jpg").colour_kept, Some(2.0));
        assert_eq!(traits(&again, "a.jpg").colour_kept, Some(measured));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A cache that can be read and not written still answers the run, and
    /// still counts what it holds: the run takes the records it superseded as
    /// the difference between that count and the map, which was an underflow
    /// when the count came back as zero.
    #[test]
    fn a_read_only_cache_counts_what_it_holds() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("readonly");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f1, t1) = analysis(3, 1);
        store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.append("a.jpg", Key { len: 2, mtime: 2, ctime: 2 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.append("b.jpg", Key { len: 3, mtime: 3, ctime: 3 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        drop(store);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        let (got, store, bad) = reopen(&path);
        assert!(bad, "not being able to write it is said");
        assert!(!store.writable());
        assert_eq!(got.len(), 2);
        assert_eq!(store.records(), 3);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A process killed part-way through an append leaves half a record. That
    /// costs the one record: the rest are read, the tail is cut off, and what
    /// is appended afterwards is readable.
    #[test]
    fn a_torn_final_record_costs_that_record_and_nothing_else() {
        let dir = scratch("torn");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(4, 3);
        store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.append("b.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        for cut in [1, 9, b.len / 2, b.len - 1] {
            let file = OpenOptions::new().write(true).open(&path).unwrap();
            file.set_len(b.at + cut).unwrap();
            drop(file);
            let (got, store, bad) = reopen(&path);
            assert!(!bad, "a torn tail is not a damaged cache");
            assert_eq!(got.keys().collect::<Vec<_>>(), [Path::new("a.jpg")]);
            assert_eq!(std::fs::metadata(&path).unwrap().len(), b.at);
            store.append("b.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
            drop(store);
            let (got, _, bad) = reopen(&path);
            assert!(!bad && got.len() == 2);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Where a record's packed length sits: after its path, its settings, the
    /// key, the picture's size and the six fields of its shape.
    /// The bytes of a record's head before its keypoint count: the path,
    /// the settings, the key, the dimensions and the traits.
    const TRAITS: u64 = 23 * 4 + 4;
    fn packed_len_at(s: Span, path: &str) -> u64 {
        s.at + 8 + path.len() as u64 + 12 + 24 + 8 + TRAITS + 6 * 4
    }

    /// A length damaged into something the file cannot hold is a torn tail,
    /// not an allocation: the records before it are kept and the run goes on.
    /// It used to abort the process — "memory allocation of 1125899906842624
    /// bytes failed" — on every run until the cache was deleted by hand.
    #[test]
    fn an_impossible_length_is_a_torn_tail_not_an_abort() {
        let dir = scratch("absurd");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(4, 3);
        store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.append("b.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        store.append("c.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.write_all_at(&(1u64 << 50).to_le_bytes(), packed_len_at(b, "b.jpg")).unwrap();
        drop(file);
        let (got, store, _) = reopen(&path);
        assert_eq!(got.keys().collect::<Vec<_>>(), [Path::new("a.jpg")]);
        assert_eq!(store.records(), 1);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), b.at, "cut where the damage starts");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Damaged framing part-way through keeps what comes before it and says
    /// so, rather than rejecting the whole file.
    #[test]
    fn damaged_framing_keeps_the_records_before_it() {
        let dir = scratch("framing");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(4, 3);
        store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.append("b.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        // A path length past `MAX_PATH` but inside the file's size.
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        file.write_all_at(&((MAX_PATH as u64) + 1).to_le_bytes(), b.at).unwrap();
        drop(file);
        let (got, store, bad) = reopen(&path);
        assert!(bad, "damage that is not a torn tail is said");
        assert_eq!(got.keys().collect::<Vec<_>>(), [Path::new("a.jpg")]);
        assert_eq!(store.records(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// One record whose body will not unpack costs that record and nothing
    /// else — not the records of the other folders the same cache serves.
    #[test]
    fn a_damaged_record_costs_itself_and_not_the_cache() {
        let dir = scratch("body");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(40, 3);
        store.append("other/x.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        store.append("a.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.append("b.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let shape = store.append("c.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        // b's first stream: its length is intact, its deflate is not.
        let body = packed_len_at(b, "b.jpg") + 8 + 8;
        file.write_all_at(&[0xff; 24], body).unwrap();
        // c claims more keypoints than a run of these settings describes.
        file.write_all_at(&(SETTINGS.features + 1).to_le_bytes(), shape.at + 8 + 5 + 12 + 24 + 8 + TRAITS + 8).unwrap();
        drop(file);
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = open(&path, SETTINGS, &|p| !p.starts_with("other"), &mut problems);
        assert!(problems.any(), "a damaged record is worth saying");
        let mut names: Vec<_> = got.keys().cloned().collect();
        names.sort();
        assert_eq!(names, [PathBuf::from("a.jpg"), PathBuf::from("other/x.jpg")]);
        assert_eq!(store.records(), 4, "counted, so that the run compacts it away");
        assert!(store.writable());
        // However little of the file it is.
        let keep: Vec<(&Path, Span)> = Vec::new();
        assert!(store.worth_compacting(&keep, false));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A wide picture's thumbnail is longer than the setting on one side
    /// (`decode::area_factor`), and is a record like any other: read back,
    /// not called damaged.
    #[test]
    fn a_wide_thumbnail_is_not_damage() {
        let dir = scratch("wide");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, _) = analysis(10, 2);
        let t = Thumb::new(400, 30, 0.125, vec![2; 400 * 30]);
        store.append("wide.jpg", Key { len: 1, mtime: 1, ctime: 1 }, (3200, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        let (got, _, bad) = reopen(&path);
        assert!(!bad, "a wide thumbnail is not damage");
        assert_eq!(got.keys().collect::<Vec<_>>(), [Path::new("wide.jpg")]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Records nothing will read are left in the file until they are a quarter
    /// of it, and `--prune-cache` compacts whatever the share.
    #[test]
    fn a_file_is_compacted_once_a_quarter_of_it_is_unused() {
        let dir = scratch("share");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(20, 5);
        let names: Vec<String> = (0..8).map(|i| format!("{i}.jpg")).collect();
        let spans: Vec<Span> =
            names.iter().map(|p| store.append(p, Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f, &t).unwrap()).collect();
        let keeping = |k: usize| -> Vec<(&Path, Span)> { (0..k).map(|i| (Path::new(names[i].as_str()), spans[i])).collect() };
        assert!(!store.worth_compacting(&keeping(8), false), "nothing unused");
        assert!(!store.worth_compacting(&keeping(7), false), "one record of eight is not a quarter");
        assert!(store.worth_compacting(&keeping(7), true), "unless asked");
        assert!(store.worth_compacting(&keeping(6), false), "two of eight is");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two runs holding one cache: neither replaces the file while the other
    /// has it, so what the other appends afterwards is still in it. A
    /// compaction used to rename a new file over the one the other run was
    /// appending to, and every record it made from then on was lost.
    #[test]
    fn a_cache_another_run_holds_is_not_replaced_under_it() {
        let dir = scratch("two-runs");
        let path = dir.join(file_name());
        let (f, t) = analysis(20, 5);
        let key = Key { len: 1, mtime: 1, ctime: 1 };
        let (_, mut a, _) = reopen(&path);
        let (_, b, _) = reopen(&path);
        a.append("old.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        // `a` drops `old.jpg` while `b` has the file: it is left as it is.
        assert!(!a.compact(&mut []).unwrap(), "put off");
        b.append("b.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop((a, b));
        let (got, mut store, _) = reopen(&path);
        assert!(got.contains_key(Path::new("b.jpg")), "b's record is in the file the name leads to");
        assert!(got.contains_key(Path::new("old.jpg")), "and nothing was compacted away under b");
        // Alone, a run compacts as before.
        assert!(store.compact(&mut []).unwrap());
        drop(store);
        let (got, _, _) = reopen(&path);
        assert!(got.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Another run that appended to the cache and finished while this one
    /// had it open: what it appended is not this run's to keep or drop, and a
    /// compaction keeps it. It used to copy only what this run knew of, and
    /// the other run's records, counted as dead, were what set it off.
    #[test]
    fn a_compaction_keeps_what_another_run_appended_meanwhile() {
        let dir = scratch("two-runs-finished");
        let path = dir.join(file_name());
        let (f, t) = analysis(20, 5);
        let (f2, t2) = analysis(20, 6);
        let key = Key { len: 1, mtime: 1, ctime: 1 };
        let newer = Key { len: 2, mtime: 2, ctime: 2 };
        let (_, mut a, _) = reopen(&path);
        let mine = a.append("mine.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        a.append("dead.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        let shared = a.append("shared.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        let (_, b, _) = reopen(&path);
        b.append("other.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        // The other run described a file this one has a record for, after it.
        b.append("shared.jpg", newer, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        drop(b);
        let keep = [(Path::new("mine.jpg"), mine), (Path::new("shared.jpg"), shared)];
        // Only `dead.jpg` is dead: the other run's two records are live.
        assert!(!a.worth_compacting(&keep, false), "a fifth of the file is dead, not three fifths");
        assert!(a.compact(&keep).unwrap());
        drop(a);
        let (got, store, _) = reopen(&path);
        assert!(got.contains_key(Path::new("other.jpg")), "the other run's record is kept");
        assert!(got.contains_key(Path::new("mine.jpg")) && !got.contains_key(Path::new("dead.jpg")));
        let s = &got[Path::new("shared.jpg")];
        assert_eq!((s.0.len, &s.1.as_ref().unwrap().feats.desc), (2, &f2.desc), "the later record still wins");
        assert_eq!(store.records(), 4);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file is gone when its folder's listing lacks it or the folder is
    /// gone; one beside it that is there is not.
    #[test]
    fn gone_is_asked_a_folder_at_a_time() {
        let dir = scratch("gone");
        let (here, deleted) = (dir.join("a.jpg"), dir.join("b.jpg"));
        std::fs::write(&here, b"x").unwrap();
        let in_gone_folder = dir.join("removed/c.jpg");
        let in_file = here.join("d.jpg");
        let asked = [here.as_path(), deleted.as_path(), in_gone_folder.as_path(), in_file.as_path()];
        let mut found: Vec<&Path> = gone(asked.into_iter()).into_iter().collect();
        found.sort();
        let mut want = vec![deleted.as_path(), in_file.as_path(), in_gone_folder.as_path()];
        want.sort();
        assert_eq!(found, want);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A record whose file cannot be looked at right now — a folder made
    /// unreadable here; a dropped sshfs or NFS mount in life — is carried,
    /// not taken for a file deleted.
    #[test]
    fn a_file_that_cannot_be_reached_is_not_gone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("unreachable");
        let locked = dir.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        let f = locked.join("a.jpg");
        std::fs::write(&f, b"x").unwrap();
        let mut m: HashMap<PathBuf, Entry> = HashMap::new();
        m.insert(f.clone(), (Key { len: 1, mtime: 1, ctime: 1 }, None, Span { at: 8, len: 10 }));
        m.insert(dir.join("deleted.jpg"), (Key { len: 1, mtime: 1, ctime: 1 }, None, Span { at: 18, len: 10 }));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let reachable = std::fs::metadata(&f).is_ok();
        let kept: Vec<PathBuf> = carry_over(&m).into_iter().map(|(p, _)| p.to_path_buf()).collect();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        // Root reads through any mode, and then the file is simply there.
        if !reachable {
            assert_eq!(kept, vec![f]);
        }
    }

    /// The default file is named for the format, so that a format change
    /// starts a new file rather than meeting the old one and caching nothing.
    #[test]
    fn the_default_file_is_named_for_its_format() {
        assert_eq!(file_name(), format!("analysis-{FORMAT}.bin"));
        assert_eq!(locate(None).file_name().unwrap().to_str().unwrap(), file_name());
        assert_eq!(locate(None).parent().unwrap(), default_dir());
    }

    /// Compaction keeps exactly what it is handed, copied rather than packed
    /// again, and a cache from another format version is left as it is: read
    /// for nothing, written to by nothing.
    #[test]
    fn compaction_keeps_what_it_is_given_and_another_versions_file_is_left_alone() {
        let dir = scratch("compact");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(6, 4);
        let spans: Vec<Span> = ["c.jpg", "a.jpg", "b.jpg", "a.jpg"]
            .iter()
            .map(|p| store.append(p, Key { len: 7, mtime: 7, ctime: 7 }, (320, 240), &Traits::default(), &f, &t).unwrap())
            .collect();
        let mut store = store;
        let mut keep = vec![(Path::new("c.jpg"), spans[0]), (Path::new("a.jpg"), spans[3])];
        assert!(store.compact(&mut keep).unwrap());
        drop(store);
        let (got, store, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!(store.records(), 2);
        let mut names: Vec<_> = got.keys().cloned().collect();
        names.sort();
        assert_eq!(names, [PathBuf::from("a.jpg"), PathBuf::from("c.jpg")]);
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().feats.desc, f.desc);
        drop(store);

        let file = OpenOptions::new().write(true).open(&path).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        for other in [b"IMGFPC08", b"IMGFPC99"] {
            file.write_all_at(other, 0).unwrap();
            let (got, store, bad) = reopen(&path);
            assert!(got.is_empty() && !bad && store.records() == 0);
            assert_eq!(store.other_version(), Some(std::str::from_utf8(other).unwrap()));
            let (f, t) = analysis(6, 4);
            assert!(store.append("d.jpg", Key { len: 7, mtime: 7, ctime: 7 }, (320, 240), &Traits::default(), &f, &t).is_none());
            assert!(!store.worth_compacting(&[], true));
            drop(store);
            assert_eq!(std::fs::metadata(&path).unwrap().len(), size, "the file is not touched");
        }
        drop(file);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A run at another working size reads none of this one's records and
    /// destroys none of them either: they are carried, unread, and the next
    /// run at this size finds them where they were. A run at 640 used to
    /// replace the whole file, every folder's analysis with it.
    #[test]
    fn records_made_at_other_settings_are_kept_not_read() {
        let dir = scratch("settings");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(6, 4);
        store.append("a.jpg", Key { len: 7, mtime: 7, ctime: 7 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        store.append("b.jpg", Key { len: 7, mtime: 7, ctime: 7 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);

        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let other = Settings { work_size: 640, ..SETTINGS };
        let (got, store) = open(&path, other, &|_| true, &mut problems);
        assert!(got.is_empty() && !problems.any());
        assert_eq!((store.records(), store.other_settings().len()), (2, 2));
        let (f2, t2) = analysis(3, 9);
        store.append("a.jpg", Key { len: 7, mtime: 7, ctime: 7 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        drop(store);

        // Back at the first size: its two records, and the 640 one carried.
        let (got, store, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!((got.len(), store.records(), store.other_settings().len()), (2, 3, 1));
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().feats.desc, f.desc);
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().dims, (320, 240));
        // A compaction that keeps both kinds keeps both.
        let mut store = store;
        let mut keep: Vec<(&Path, Span)> = got.iter().map(|(p, e)| (p.as_path(), e.2)).collect();
        let others = store.other_settings().to_vec();
        keep.extend(others.iter().map(|(p, s)| (p.as_path(), *s)));
        assert!(store.compact(&mut keep).unwrap());
        drop(store);
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = open(&path, other, &|_| true, &mut problems);
        assert_eq!((got.len(), store.records()), (1, 3));
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().feats.desc, f2.desc);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two names that are not UTF-8 and read the same once made printable
    /// are two files, and each keeps its own analysis. Keyed on the printable
    /// form, the second record superseded the first, and the first file was
    /// then handed the second's analysis on the next run.
    #[test]
    fn names_that_are_not_utf8_keep_their_own_records() {
        let dir = scratch("bytes");
        let path = dir.join(file_name());
        let fe = PathBuf::from(OsString::from_vec(b"x\xfe.jpg".to_vec()));
        let ff = PathBuf::from(OsString::from_vec(b"x\xff.jpg".to_vec()));
        assert_eq!(fe.display().to_string(), ff.display().to_string());
        let (_, store, _) = reopen(&path);
        let (f1, t1) = analysis(3, 1);
        let (f2, t2) = analysis(5, 2);
        store.append(&fe, Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.append(&ff, Key { len: 1, mtime: 1, ctime: 1 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        drop(store);
        let (got, _, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!(got.len(), 2);
        assert_eq!(got[&fe].1.as_ref().unwrap().feats.desc, f1.desc);
        assert_eq!(got[&ff].1.as_ref().unwrap().feats.desc, f2.desc);
        std::fs::remove_dir_all(&dir).ok();
    }
}

