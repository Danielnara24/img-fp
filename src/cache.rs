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
use crate::problems::{Note, Problems};
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
/// (`analysis::enlarge_below`), which changes every record below 512.
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
/// 16: a record carries the device its file lay on (`Key::dev`), which tells
/// a mount point left behind from a folder emptied by hand (`away`); and
/// `Traits::exif` is said of a WebP, a PNG whose `eXIf` follows its pixels, a
/// TIFF and a HEIF or JPEG XL whose EXIF lies past the first 128 KB, which a
/// record of 15 said had none.
/// 17: a record says whether its file has more than eight bits a sample
/// (`Traits::deep`) and carries a colour profile (`Traits::profile`), and a
/// JPEG holding further pictures (MPF) has `Traits::frames`: the keep rule
/// reads all three, and a record of 16 says none.
/// 18: `Traits::frames` is said of a JPEG holding a video or another JPEG
/// after its picture (a Motion Photo), and not of one whose MPF index adds
/// only a large thumbnail; a record of 17 says the opposite of both.
/// 19: a picture stored turned or mirrored (an EXIF orientation) is reduced
/// to the plane its upright copy is (`reduce::Turned`); a record of 18 holds
/// the plane reduced as stored and turned after, other features.
/// 20: an icon of several sizes has `Traits::frames`, and an icon is lossless
/// only as the entry read is; a record of 19 says one picture, lossless.
const MAGIC: &[u8; 8] = b"IMGFPC20";
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

/// What the file says about one path at this run's settings: its key, its
/// analysis — only for a path this run walks, see `read` — and where its
/// record sits in the file.
type Entry = (Key, Option<Record>, Span);

/// Which paths this run walks, asked of every record at this run's settings
/// as it is read, with the record's key.
type Walked<'a> = &'a (dyn Fn(&Path, Key) -> bool + Sync);

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
///
/// **The device is not part of it.** It is where the file lay when the
/// record was made, which says nothing of whether the file changed; `away`
/// reads it.
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub len: u64,
    pub mtime: i64,
    pub ctime: i64,
    pub dev: u64,
}

impl PartialEq for Key {
    fn eq(&self, o: &Key) -> bool {
        (self.len, self.mtime, self.ctime) == (o.len, o.mtime, o.ctime)
    }
}

/// The default cache file's name, which carries the format: `analysis-IMGFPC20.bin`.
///
/// **Each format has a default file of its own**, so that a format change
/// starts a new file beside the old one instead of meeting it. With one name
/// for every format, an upgrade found the old version's file there, left it
/// alone (see the note on `MAGIC`), and so cached nothing on any run until
/// someone thought of `--clear-cache`, which the window's users would not.
/// Now two versions installed side by side each keep their own default, and
/// an upgraded one starts afresh. The other formats' files in the default
/// folder are removed (`remove_other_formats`), unless a run is using one.
/// (Up to `IMGFPC11` the file was `analysis.bin`.)
///
/// A file named with `--cache` is used as named, whatever format it holds.
pub fn file_name() -> String {
    format!("analysis-{FORMAT}.bin")
}

/// Remove the caches of other formats from the default folder, with the
/// files beside them; each removed, with its format and the bytes it held.
///
/// **They were kept for good.** Each format change left the last one's file
/// behind, read by nothing (on the author's machine a 2.5 GB `IMGFPC15` beside
/// the 1.6 GB current one), and the window's users never see the folder. A
/// file another run holds is in use (an older version running now, which
/// holds it shared from start to end), and is left; another version
/// installed beside this one starts its own afresh next time it runs. Only
/// files named as a default cache is and holding a cache's header: a file a
/// person put there is not touched.
pub fn remove_other_formats() -> Vec<(String, u64)> {
    let dir = default_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mine = file_name();
    let mut removed = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        let default_named = name == "analysis.bin" || (name.starts_with("analysis-IMGFPC") && name.ends_with(".bin"));
        if !default_named || name == mine {
            continue;
        }
        let path = e.path();
        let Ok(f) = OpenOptions::new().read(true).open(&path) else { continue };
        let mut magic = [0u8; 8];
        if !f.metadata().is_ok_and(|m| m.is_file()) || f.read_exact_at(&mut magic, 0).is_err() || &magic[..MAGIC_PREFIX.len()] != MAGIC_PREFIX || &magic == MAGIC {
            continue;
        }
        if !flock(&f, LOCK_EX | LOCK_NB) {
            continue;
        }
        let bytes = f.metadata().map_or(0, |m| m.len());
        if std::fs::remove_file(&path).is_ok() {
            for p in [heads::path_of(&path), crate::matched::path_beside(&path), crate::matched::identical_beside(&path)] {
                let _ = std::fs::remove_file(p);
            }
            removed.push((String::from_utf8_lossy(&magic).into_owned(), bytes));
        }
    }
    removed.sort();
    removed
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
///
/// **A file there that is not a cache is not one to use**, nor to delete:
/// `--cache photo.jpg` (or the window's cache chooser, pointed at a photo)
/// replaced the photo with an empty cache, saying it was "ignoring" it, and
/// `--clear-cache` deleted it. Such a run has no cache, and says why.
pub fn resolve_path(explicit: Option<&Path>, problems: &mut Problems) -> Option<PathBuf> {
    let path = locate(explicit);
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        if let Err(e) = std::fs::create_dir_all(dir) {
            problems.note(Note::Cache, format_args!("could not create {}: {e}", dir.display()));
            return None;
        }
    }
    if is_stranger(&path) {
        problems.note(Note::Cache, format_args!("{} is not an img-fp cache, so it is left as it is and this run uses no cache", path.display()));
        return None;
    }
    Some(path)
}

/// `--clear-cache`: delete the cache at `path` (one `resolve_path` gave,
/// so a cache or nothing) and the files kept beside it: its index, the last
/// matching and the files the last run proved byte-identical. Those are
/// named after it, so each is deleted only if it is img-fp's.
pub fn clear(path: &Path, problems: &mut Problems) {
    let beside = [heads::path_of(path), crate::matched::path_beside(path), crate::matched::identical_beside(path)];
    for p in beside {
        let mut head = [0u8; 5];
        if File::open(&p).and_then(|f| f.read_exact_at(&mut head, 0)).is_ok() && &head == b"IMGFP" {
            let _ = std::fs::remove_file(&p);
        }
    }
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        // Asked for and not done, and the next run will read the cache this
        // one meant to be rid of.
        Err(e) => problems.note(Note::Cache, format_args!("could not delete {}: {e}", path.display())),
    }
}

/// Whether something is at `path` that is not a cache: a file that does not
/// begin as one, or anything that is not a regular file. Nothing there, or
/// a file that cannot be read (for the reading to say), is not a stranger.
fn is_stranger(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => File::open(path).is_ok_and(|f| !begins_as_cache(&f)),
        Ok(_) => true,
        Err(_) => false,
    }
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
///
/// Both times are read as the kernel keeps them, seconds and nanoseconds,
/// signed: a file dated before 1970 has a key like any other. Read through
/// `SystemTime` since `UNIX_EPOCH`, as it was, such a file had none, so it
/// was analysed again on every run and no run holding it could take the last
/// run's matching (`matched.rs`). Times from 1970 on are the same numbers
/// either way, so the records already kept still answer.
pub fn key_from(md: &std::fs::Metadata) -> Option<Key> {
    use std::os::unix::fs::MetadataExt;
    let mtime = md.mtime().checked_mul(1_000_000_000)?.checked_add(md.mtime_nsec())?;
    let ctime = md.ctime().checked_mul(1_000_000_000)?.checked_add(md.ctime_nsec())?;
    Some(Key { len: md.len(), mtime, ctime, dev: md.dev() })
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
    b.u64(k.dev)?;
    b.u32(dims.0)?;
    b.u32(dims.1)?;
    for v in tr.detail.iter().chain([&tr.colour]).chain(tr.grid.iter()) {
        b.f32(*v)?;
    }
    b.u32(tr.lossless as u32 | (tr.exif as u32) << 1 | (tr.frames as u32) << 2 | (tr.deep as u32) << 3 | (tr.profile as u32) << 4)?;
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
struct Span {
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
    /// Not a cache at all: a file the user named by mistake.
    NotACache,
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
/// **It answers for this run's files by number**: `take` hands a walked
/// file's record over, `append` keeps one made for it, `keep_colour` writes
/// in the record kept for it, and `finish` keeps what the run took and made
/// and what it was not asked about while its file is there. Which records a
/// run keeps is decided there and nowhere else.
///
/// The file is rewritten only when it holds something the run would not keep:
/// a record superseded by a newer one for the same path, a file that has
/// gone, a `--prune-cache`. Then `compact` copies the records worth keeping
/// into a new file, sorted, byte for byte and without unpacking any of them.
pub struct Store {
    path: PathBuf,
    /// The settings this run's records are made at.
    settings: Settings,
    /// The files this run walked, by number: what the file holds for each.
    asked: Vec<Asked>,
    /// Where the record each walked file's analysis is kept in sits: one
    /// taken (`take`) or appended (`append`) by this run. The records the
    /// run keeps for its own files are these and no others.
    live: Mutex<Vec<Option<Span>>>,
    /// The records this run was not asked about: the latest for each path
    /// and settings, at this run's settings for a path it does not walk, or
    /// at any other's. Never unpacked; carried while their file is there
    /// (`carried`).
    unasked: Vec<Unasked>,
    /// Records at this run's settings the file holds, the latest for each
    /// path, walked or not.
    usable: usize,
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

/// One walked file, as the cache knows it.
struct Asked {
    /// Its name in the file (`lib::cache_names`).
    name: PathBuf,
    /// What the walk read of it, which a record must carry to answer for it;
    /// `None`, nothing does, and nothing is kept for it.
    key: Option<Key>,
    /// The latest record at this run's settings under its name, whatever
    /// key it carries.
    found: Option<Span>,
    /// That record's analysis, when its key is the file's and it has not
    /// been taken.
    record: Option<Record>,
}

/// A record this run was not asked about; see `Store::unasked`.
struct Unasked {
    path: PathBuf,
    span: Span,
    /// The device its file lay on (`Key::dev`).
    dev: u64,
    /// Made at this run's settings, for a file it did not walk.
    here: bool,
}

/// What `Store::finish` kept and did, for the run to say.
pub struct Kept {
    /// The first append that failed, if one did.
    pub failure: Option<String>,
    /// Records carried for files this run did not walk, at its settings and
    /// at others.
    pub carried: usize,
    pub other_sizes: usize,
    /// Records this run was not asked about and did not carry.
    pub dropped: usize,
    pub compacted: Compacted,
}

/// Whether `Store::finish` rewrote the file.
pub enum Compacted {
    /// Not worth it: the file's records, and how many nothing keeps.
    Unneeded { records: usize, unused: usize },
    /// Put off: another run has the file open.
    Busy,
    Done,
    Failed(String),
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
///
/// `files` are this run's, each by the name the cache knows it by and what
/// the walk read of it: what the store answers for by number from here on.
/// `met(i)` is said as the read meets a record of walked file `i` under its
/// key (once or more): what the progress bar knows of the cache's answers
/// before the read is done. A later record for the path may still replace it.
///
/// **A file found under another name than its record's is analysed again.**
/// 0.69.0 answered it with another name's record whose key (length, mtime,
/// ctime to the nanosecond) was its own, so a renamed folder or a drive
/// mounted elsewhere kept its analysis. The key does not say which file it
/// is: the kernel's change time is coarse (a clock tick), and files written
/// or `touch`ed together share it, so same-size BMPs copied as one folder
/// all had one key. Scanned in two runs, one took the other's analysis and
/// the two different pictures were reported "same pixels", one DELETE. A
/// moved file costs its analysis again; that is what is intended.
pub fn open(path: &Path, want: Settings, files: Vec<(PathBuf, Option<Key>)>, met: &(dyn Fn(usize) + Sync), problems: &mut Problems) -> Store {
    let (mut got, mut store) = {
        let walked: HashMap<&Path, (usize, Option<Key>)> = files.iter().enumerate().map(|(i, f)| (f.0.as_path(), (i, f.1))).collect();
        let walked = |p: &Path, k: Key| match walked.get(p) {
            Some(&(i, key)) => {
                if key == Some(k) {
                    met(i);
                }
                true
            }
            None => false,
        };
        read(path, want, &walked, problems)
    };
    store.usable = got.len();
    store.asked = files
        .into_iter()
        .map(|(name, key)| match got.remove(&name) {
            Some((k, record, span)) => Asked { record: record.filter(|_| key == Some(k)), name, key, found: Some(span) },
            None => Asked { name, key, found: None, record: None },
        })
        .collect();
    // What is left is every record at this run's settings for a path it did
    // not walk.
    store.unasked.extend(got.into_iter().map(|(path, (k, _, span))| Unasked { path, span, dev: k.dev, here: true }));
    store.live = Mutex::new(vec![None; store.asked.len()]);
    store
}

/// The file at `path`, read: the latest record at `want` for each path,
/// unpacked for the paths `walked` says this run walks, and the store with
/// what else it holds (`Store::unasked`).
fn read(path: &Path, want: Settings, walked: Walked, problems: &mut Problems) -> (HashMap<PathBuf, Entry>, Store) {
    let mut out = HashMap::new();
    let mut store = Store {
        path: path.to_path_buf(),
        settings: want,
        asked: Vec::new(),
        live: Default::default(),
        unasked: Vec::new(),
        usable: 0,
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
                        problems.note(Note::Cache, format_args!("could not write {}: {e}", path.display()));
                        return (out, store);
                    }
                }
            }
            Err(e) => {
                // A cache that can be read and not written still answers this
                // run. It just keeps nothing new, and that is worth saying.
                problems.note(Note::Cache, format_args!("could not open {} for writing: {e}", path.display()));
                // What it holds is still counted, although nothing will be
                // appended: `stale` measures how many records a later one
                // superseded against this, and a count of zero beside a full map
                // was an underflow.
                match File::open(path).map(|f| read_file(&f, path, want, walked, &mut out, &mut store.unasked, problems)) {
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
                Err(e) => problems.note(Note::Cache, format_args!("could not write {}: {e}", path.display())),
            }
            return (out, store);
        }
        match read_file(&f, path, want, walked, &mut out, &mut store.unasked, problems) {
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
                        problems.note(Note::Cache, format_args!("could not repair {}: {e}", path.display()));
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
            Found::NotACache => return (out, store),
            // Replaced by an empty one, unless another run is using it.
            Found::Unusable => {
                if exclusive(&f, path) {
                    heads::remove(path);
                    match replace_with(path, |f| Buf(f).header()) {
                        Ok(f) => store.file = Some(f),
                        Err(e) => problems.note(Note::Cache, format_args!("could not write {}: {e}", path.display())),
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
    /// Not a cache at all, and that has been said: left as it is.
    NotACache,
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
    others: &mut Vec<Unasked>,
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
    // A file that does not begin as a cache does is not one, however short:
    // a photo or a note named by mistake with `--cache` was replaced by an
    // empty cache, and the message said it was being ignored.
    if !begins_as_cache(f) {
        let read = Err(Reject::NotACache);
        return found(read, path, out, others, problems);
    }
    let index = heads::load(f, path, size);
    let covered = index.as_ref().map_or(0, |i| i.end);
    let mut seen = Vec::new();
    let read = match index.and_then(|i| read_indexed(f, size, i, want, walked, out, others, &mut seen)) {
        Some(read) => read,
        None => {
            // No index, or one this file does not bear out: read it whole.
            out.clear();
            others.clear();
            seen.clear();
            (|| {
                let mut r = Rd { r: std::io::BufReader::with_capacity(1 << 20, f), pos: 0, size, ended: false };
                (&*f).rewind().map_err(|e| Reject::Damaged(e.into()))?;
                let mut reading = Reading::new(want, walked, out, others, &mut seen);
                reading.magic(&mut r)?;
                reading.stream(&mut r);
                Ok(reading.done())
            })()
        }
    };
    if let Ok(tail) = &read {
        heads::refresh(f, path, covered, tail, &seen);
    }
    found(read, path, out, others, problems)
}

/// Whether the file's first bytes are a cache's (`MAGIC_PREFIX`), or as much
/// of them as it holds: a header cut short is a damaged cache, not a stranger.
fn begins_as_cache(f: &File) -> bool {
    let mut head = [0u8; MAGIC_PREFIX.len()];
    let mut got = 0;
    while got < head.len() {
        match f.read_at(&mut head[got..], got as u64) {
            Ok(0) => break,
            Ok(k) => got += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            // Unreadable is for the reading to say.
            Err(_) => return true,
        }
    }
    head[..got] == MAGIC_PREFIX[..got]
}

/// What a reading of the file came to, said where it is a problem.
fn found(read: Result<Tail, Reject>, path: &Path, out: &mut HashMap<PathBuf, Entry>, others: &mut Vec<Unasked>, problems: &mut Problems) -> Found {
    match read {
        Ok(tail) => {
            if let Some(why) = &tail.broken_at {
                problems.note(Note::Cache, format_args!(
                    "{} is damaged part-way through ({why}); the {} record(s) before the damage are kept",
                    path.display(),
                    tail.count
                ));
            }
            if tail.damaged > 0 {
                problems.note(Note::Cache, format_args!(
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
        Err(Reject::NotACache) => {
            problems.note(Note::Cache, format_args!("{} is not an img-fp cache, so it is left as it is and this run keeps nothing in it", path.display()));
            out.clear();
            others.clear();
            Found::NotACache
        }
        Err(Reject::Damaged(e)) => {
            problems.note(Note::Cache, format_args!("ignoring {}: {e}", path.display()));
            out.clear();
            others.clear();
            Found::Unusable
        }
    }
}

impl Store {
    /// The record of walked file `i`, taken out of the store: there only if
    /// it is still that file's, by the key the walk read, and only once.
    /// What the run takes is what it keeps (`finish`): a record of a file
    /// whose bytes another file answers for, never taken, is the same
    /// analysis stored twice and is left out at the next compaction.
    pub fn take(&mut self, i: usize) -> Option<Record> {
        let a = &mut self.asked[i];
        let record = a.record.take()?;
        self.live.get_mut().unwrap_or_else(PoisonError::into_inner)[i] = a.found;
        Some(record)
    }

    /// Put walked file `i`'s analysis into the file, as soon as it is made,
    /// keyed by what the walk read before anything was: a file changing
    /// under the analysis is described again next time rather than trusted.
    /// Called by the workers as they finish.
    pub fn append(&self, i: usize, dims: (u32, u32), traits: &Traits, f: &Features, t: &Thumb) {
        let a = &self.asked[i];
        let Some(key) = a.key else { return };
        if let Some(span) = self.put(&a.name, key, dims, traits, f, t) {
            lock_any(&self.live)[i] = Some(span);
        }
    }

    /// Keep `colour` in walked file `i`'s record, if this run keeps one for
    /// it; see `write_colour`.
    pub fn keep_colour(&self, i: usize, colour: f32) {
        let span = lock_any(&self.live)[i];
        if let Some(span) = span {
            self.write_colour(span, &self.asked[i].name, colour);
        }
    }

    /// Walked file `i`'s name in the file: its canonical path
    /// (`lib::cache_names`), however the run spelled it.
    pub fn name(&self, i: usize) -> &Path {
        &self.asked[i].name
    }

    /// Records at this run's settings the file holds, the latest for each
    /// path.
    pub fn usable(&self) -> usize {
        self.usable
    }

    /// Whether the file holds records this run will not keep, as far as can
    /// be told before the analysis and without asking which files have gone:
    /// one superseded or damaged, or a walked file's not taken. What the
    /// progress bar expects of `finish`, and nothing else.
    pub fn stale(&self) -> bool {
        let found = self.asked.iter().filter(|a| a.found.is_some()).count();
        let live = lock_any(&self.live);
        self.records().saturating_sub(found + self.unasked.len()) > 0
            || self.asked.iter().zip(live.iter()).any(|(a, l)| a.found.is_some() && l.is_none())
    }

    /// Settle the file at the end of the analysis: it keeps this run's
    /// records (`take`, `append`), every record it was not asked about whose
    /// file is still there (`carried`), and what another run appended
    /// meanwhile; and it is rewritten to hold just that when what else it
    /// holds is worth a copy of the rest (`worth_compacting`).
    ///
    /// `--prune-cache` (`prune`) keeps only this run's.
    pub fn finish(&mut self, prune: bool) -> Kept {
        let failure = self.failure();
        // Every record worth keeping is already in the file, since each went
        // in as it was made. The file is rewritten only when what else it
        // holds — records superseded, for files that have gone, or pruned —
        // is worth a copy of the rest. A threshold sweep, a dozen runs over
        // one unchanged corpus, finds the file already says what it would
        // write and writes nothing.
        //
        // The names are lent to the compaction and handed back: the colours
        // the suggestion measures later are kept by them.
        let (asked, unasked) = (std::mem::take(&mut self.asked), std::mem::take(&mut self.unasked));
        let live = lock_any(&self.live).clone();
        let carried = if prune { Vec::new() } else { carried(&unasked) };
        let entries: Vec<(&Path, Span)> = asked
            .iter()
            .zip(live)
            .filter_map(|(a, s)| Some((a.name.as_path(), s?)))
            .chain(carried.iter().map(|u| (u.path.as_path(), u.span)))
            .collect();
        let compacted = if !self.worth_compacting(&entries, prune) {
            Compacted::Unneeded { records: self.records(), unused: self.records().saturating_sub(entries.len()) }
        } else {
            match self.compact(&entries) {
                Ok(true) => Compacted::Done,
                Ok(false) => Compacted::Busy,
                Err(e) => Compacted::Failed(e.to_string()),
            }
        };
        let other_sizes = carried.iter().filter(|u| !u.here).count();
        let kept = Kept { failure, carried: carried.len() - other_sizes, other_sizes, dropped: unasked.len() - carried.len(), compacted };
        drop((entries, carried));
        (self.asked, self.unasked) = (asked, unasked);
        kept
    }

    /// Put one analysis into the file, and say where it went. The packing
    /// runs outside the lock, and only the write is inside it.
    fn put(&self, path: impl AsRef<Path>, key: Key, dims: (u32, u32), traits: &Traits, f: &Features, t: &Thumb) -> Option<Span> {
        self.file.as_ref()?;
        if self.broken.load(Ordering::Relaxed) {
            return None;
        }
        match record(path.as_ref(), self.settings, key, dims, traits, f, t) {
            Ok(rec) => self.write(rec),
            Err(e) => {
                self.broke(e);
                None
            }
        }
    }

    /// Append one record's bytes, and say where they went.
    fn write(&self, rec: Vec<u8>) -> Option<Span> {
        let file = self.file.as_ref()?;
        if self.broken.load(Ordering::Relaxed) {
            return None;
        }
        let wrote = (|| -> Result<Span> {
            let _held = lock(&WRITING);
            let mut w = file;
            w.write_all(&rec)?;
            // Where this file description's offset stands, which in append
            // mode is the end of what it just wrote — whatever another run
            // appending to the same file has done meanwhile.
            let end = w.stream_position()?;
            Ok(Span { at: end - rec.len() as u64, len: rec.len() as u64 })
        })();
        match wrote {
            Ok(span) => {
                lock_any(&self.mine).insert(span.at);
                self.records.fetch_add(1, Ordering::Relaxed);
                APPENDED.fetch_add(1, Ordering::Relaxed);
                Some(span)
            }
            Err(e) => {
                self.broke(e);
                None
            }
        }
    }

    /// The first append that fails is kept to be said; the rest are not
    /// attempted.
    fn broke(&self, e: anyhow::Error) {
        if !self.broken.swap(true, Ordering::Relaxed) {
            *self.failed.lock().unwrap_or_else(PoisonError::into_inner) = Some(e.to_string());
        }
    }

    /// The format the file was written in, when it is another version's and
    /// this run therefore reads nothing from it and keeps nothing in it.
    pub fn other_version(&self) -> Option<&str> {
        self.other_version.as_deref()
    }

    /// Records the file holds, whether or not anything will keep them.
    fn records(&self) -> usize {
        self.records.load(Ordering::Relaxed)
    }

    /// The first append that failed, if one did.
    fn failure(&self) -> Option<String> {
        self.failed.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    #[cfg(test)]
    pub fn writable(&self) -> bool {
        self.file.is_some() && !self.broken.load(Ordering::Relaxed)
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
    fn worth_compacting(&self, entries: &[(&Path, Span)], prune: bool) -> bool {
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
    fn compact(&mut self, entries: &[(&Path, Span)]) -> Result<bool> {
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
            for &(p, s) in entries.iter() {
                buf.resize(s.len as usize, 0);
                src.read_exact_at(&mut buf, s.at)?;
                // A span is where a record of its path begins; anything else
                // would copy the middle of one.
                let name = p.as_os_str().as_bytes();
                if buf.get(..8) != Some(&(name.len() as u64).to_le_bytes()[..]) || buf.get(8..8 + name.len()) != Some(name) {
                    bail!("a record is not where it was read");
                }
                b.0.write_all(&buf)?;
                moved.insert(s.at, to);
                to += s.len;
            }
            b.0.flush()?;
            Ok(())
        })?;
        heads::remove(&self.path);
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
    fn write_colour(&self, span: Span, path: &Path, colour: f32) {
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
        // device (8), size (8) and detail (24), and the colour.
        let field = at + 8 + name.len() as u64 + 12 + 24 + 8 + 8 + 24;
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
    let key = Key { len: r.u64()?, mtime: r.i64()?, ctime: r.i64()?, dev: r.u64()? };
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
    traits.deep = flags & 8 != 0;
    traits.profile = flags & 16 != 0;
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

/// Reading the file's records, in the order they were written, wherever
/// they are read from: the whole file in order (`stream`), or the heads the
/// index holds and the bodies this run unpacks (`read_indexed`).
struct Reading<'a, 'o> {
    want: Settings,
    walked: Walked<'a>,
    out: &'o mut HashMap<PathBuf, Entry>,
    others: &'o mut Vec<Unasked>,
    /// The latest record for each path made at other settings, and where it
    /// is in `others`.
    other_at: HashMap<(PathBuf, Settings), usize>,
    tail: Tail,
    batch: Vec<(Head, Vec<u8>, Span)>,
    /// Every record met, for the index (`heads::refresh`).
    seen: &'o mut Vec<heads::Entry>,
}

impl<'a, 'o> Reading<'a, 'o> {
    fn new(want: Settings, walked: Walked<'a>, out: &'o mut HashMap<PathBuf, Entry>, others: &'o mut Vec<Unasked>, seen: &'o mut Vec<heads::Entry>) -> Self {
        let tail = Tail { end: MAGIC.len() as u64, count: 0, truncated: false, damaged: 0, broken_at: None };
        Reading { want, walked, out, others, other_at: HashMap::new(), tail, batch: Vec::with_capacity(BATCH), seen }
    }

    /// The header: this format's, or the reason not to read on.
    fn magic<R: Read>(&mut self, r: &mut Rd<R>) -> Result<(), Reject> {
        let magic: [u8; 8] = r.arr().map_err(|_| anyhow!("its header is cut off"))?;
        if &magic[..MAGIC_PREFIX.len()] != MAGIC_PREFIX {
            return Err(Reject::NotACache);
        }
        if &magic != MAGIC {
            // A cache this build cannot read, written by a build that could. See
            // the note on MAGIC: that is the format changing, not a damaged file.
            return Err(Reject::OtherVersion(String::from_utf8_lossy(&magic).into_owned()));
        }
        Ok(())
    }

    /// Whether a record's body is unpacked: made at this run's settings, for
    /// a path it walks.
    fn wants(&self, settings: Settings, path: &Path, key: Key) -> bool {
        settings == self.want && (self.walked)(path, key)
    }

    /// Every record from where `r` stands to the end of the file.
    fn stream<R: Read + Skip>(&mut self, r: &mut Rd<R>) {
        loop {
            let at = r.pos;
            let (want, walked) = (self.want, self.walked);
            match read_record(r, |h| h.settings == want && walked(&h.path, h.key)) {
                Ok(None) => break,
                Ok(Some((head, blob))) => {
                    self.tail.end = r.pos;
                    let span = Span { at, len: r.pos - at };
                    self.seen.push(heads::Entry::of(&head, span));
                    self.record(head, blob, span);
                }
                Err(_) if r.ended => {
                    self.tail.truncated = true;
                    break;
                }
                // The framing is damaged, so nothing after this point can be
                // found. Everything before it is still good, and it used to be
                // thrown away with the rest: the whole file was rejected and
                // replaced, taking every other folder's records with it.
                Err(e) => {
                    self.tail.truncated = true;
                    self.tail.broken_at = Some(e.to_string());
                    break;
                }
            }
        }
    }

    /// One record framed whole, and its body if `wants` asked for it.
    fn record(&mut self, head: Head, blob: Vec<u8>, span: Span) {
        self.tail.count += 1;
        // A record framed whole whose shape its own settings cannot
        // produce: its own bytes are damaged, and the records around
        // it are not. It is counted, so that the run sees the file
        // holds something to compact away, and not kept.
        if misshapen(&head) {
            self.tail.damaged += 1;
            return;
        }
        // Made at other settings: kept where it is, for whichever run
        // asks at those settings next.
        let s = head.settings;
        if s != self.want {
            self.other(head.path, s, span, head.key.dev);
            return;
        }
        self.batch.push((head, blob, span));
        if self.batch.len() == BATCH {
            self.tail.damaged += unpack_batch(&mut self.batch, self.walked, self.out);
        }
    }

    fn other(&mut self, path: PathBuf, s: Settings, span: Span, dev: u64) {
        match self.other_at.entry((path, s)) {
            std::collections::hash_map::Entry::Occupied(e) => {
                let u = &mut self.others[*e.get()];
                (u.span, u.dev) = (span, dev);
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                self.others.push(Unasked { path: e.key().0.clone(), span, dev, here: false });
                e.insert(self.others.len() - 1);
            }
        }
    }

    fn done(mut self) -> Tail {
        self.tail.damaged += unpack_batch(&mut self.batch, self.walked, self.out);
        self.tail
    }
}

/// A record whose shape its own settings cannot produce. A thumbnail's side
/// may pass the setting on a picture wider than 4:3 (`decode::area_factor`),
/// but never its area, nor `lib::THUMB_MAX_SIDES` times it.
fn misshapen(head: &Head) -> bool {
    let s = head.settings;
    let (tw, th) = (head.tw as u64, head.th as u64);
    let (thumb, sides) = (s.thumb as u64, s.thumb as u64 * crate::THUMB_MAX_SIDES as u64);
    head.n > s.features as usize || tw * th > thumb * thumb || tw.max(th) > sides
}

/// The file read through its index (`heads`): the heads of the records it
/// covers from the index, the bodies this run unpacks by `pread`, and what
/// was appended after it in order. `None` where the file does not bear the
/// index out, and it is read whole instead.
///
/// **Only the bodies this run unpacks are read.** Read in order, a scan of
/// four files against a 529 MB cache read all 529 MB, page cache cold, to
/// learn the paths and keys in the heads: 0.65 s against 0.10 s without the
/// cache, and on a spinning disk with a cache of a few gigabytes, tens of
/// seconds for any scan at all. A smaller buffer made the skips seeks, and
/// the chain of small reads defeated readahead (twice as slow cold). Where
/// this run unpacks most of the file anyway, it is read in order as before.
#[allow(clippy::too_many_arguments)]
fn read_indexed(
    f: &File,
    size: u64,
    index: heads::Index,
    want: Settings,
    walked: Walked,
    out: &mut HashMap<PathBuf, Entry>,
    others: &mut Vec<Unasked>,
    seen: &mut Vec<heads::Entry>,
) -> Option<Result<Tail, Reject>> {
    let mut magic = [0u8; 8];
    f.read_exact_at(&mut magic, 0).ok()?;
    if &magic != MAGIC {
        return None;
    }
    let mut reading = Reading::new(want, walked, out, others, seen);
    let wanted: Vec<&heads::Entry> = index.entries.iter().filter(|e| !e.misshapen && reading.wants(e.settings, &e.path, e.key)).collect();
    if wanted.iter().map(|e| e.span.len).sum::<u64>() * 2 > size {
        return None;
    }
    // The bodies, a run of neighbours at a time, checked against the index
    // as they are read: a head that is not the index's ends the reading.
    let mut bodies: HashMap<u64, (Head, Vec<u8>)> = HashMap::with_capacity(wanted.len());
    let mut k = 0;
    while k < wanted.len() {
        let from = wanted[k].span.at;
        let mut to = k + 1;
        while to < wanted.len() && wanted[to].span.at == wanted[to - 1].span.at + wanted[to - 1].span.len && wanted[to].span.at - from < 8 << 20 {
            to += 1;
        }
        let end = wanted[to - 1].span.at + wanted[to - 1].span.len;
        let mut buf = vec![0u8; (end - from) as usize];
        f.read_exact_at(&mut buf, from).ok()?;
        let mut r = Rd { r: std::io::Cursor::new(&buf[..]), pos: from, size: end, ended: false };
        for e in &wanted[k..to] {
            let (head, blob) = read_record(&mut r, |_| true).ok()??;
            if head.path != e.path || head.key != e.key || head.key.dev != e.key.dev || head.settings != e.settings || r.pos != e.span.at + e.span.len {
                return None;
            }
            bodies.insert(e.span.at, (head, blob));
        }
        k = to;
    }
    for e in &index.entries {
        reading.seen.push(e.clone());
        reading.tail.end = e.span.at + e.span.len;
        if e.misshapen {
            reading.tail.count += 1;
            reading.tail.damaged += 1;
            continue;
        }
        match bodies.remove(&e.span.at) {
            Some((head, blob)) => reading.record(head, blob, e.span),
            None if e.settings != want => {
                reading.tail.count += 1;
                reading.other(e.path.clone(), e.settings, e.span, e.key.dev);
            }
            // At this run's settings for a path it does not walk: its key
            // and where it is, which `unpack_batch` keeps of such a record.
            None => {
                reading.tail.count += 1;
                reading.out.insert(e.path.clone(), (e.key, None, e.span));
            }
        }
    }
    // What was appended after the index was written.
    if index.end < size {
        let mut b = std::io::BufReader::with_capacity(1 << 20, f);
        b.seek(std::io::SeekFrom::Start(index.end)).ok()?;
        let mut r = Rd { r: b, pos: index.end, size, ended: false };
        reading.stream(&mut r);
    }
    Some(Ok(reading.done()))
}

impl Skip for std::io::Cursor<&[u8]> {
    fn skip(&mut self, n: u64) -> std::io::Result<()> {
        self.seek_relative(i64::try_from(n).map_err(std::io::Error::other)?)
    }
}

/// The index beside a cache file: the head of every record it covers (path,
/// settings, key, where it lies), so that a run reads the bodies it unpacks
/// and nothing else (`read_indexed`).
///
/// **The cache file is what is true; the index is a hint it must bear
/// out.** It is not written as records are appended, and so needs no lock
/// and no order with the appends of other runs: a run that read past what
/// the index covers by `WORTH` writes a new one, beside it and renamed into
/// place, covering the records it read. It is used only when it names the
/// file as it is — the same device, inode and birth time, no shorter than
/// what it covers, and the last 4 KB it covers as they were — and every
/// body read through it must have the head the index gives, or the file is
/// read whole as it was before there was one. A compaction removes it.
pub(crate) mod heads {
    use super::{Head, Key, Settings, Span, Tail, MAGIC};
    use std::fs::File;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::fs::{FileExt, MetadataExt};
    use std::path::{Path, PathBuf};

    const INDEX_MAGIC: &[u8; 8] = b"IMGFPH01";

    /// How far past what an index covers a run reads before it writes a new
    /// one. Below it the file is read whole in about the time an index takes
    /// to write; a cache of a few hundred photographs never has one.
    const WORTH: u64 = 16 << 20;

    /// The bytes before the end of what an index covers that it holds a hash
    /// of, as a check that they are still the file's.
    const CHECKED: u64 = 4096;

    #[derive(Clone, Debug)]
    pub(super) struct Entry {
        pub(super) span: Span,
        pub(super) settings: Settings,
        pub(super) key: Key,
        pub(super) path: PathBuf,
        /// `super::misshapen`, which a head alone says.
        pub(super) misshapen: bool,
    }

    impl Entry {
        pub(super) fn of(h: &Head, span: Span) -> Entry {
            Entry { span, settings: h.settings, key: h.key, path: h.path.clone(), misshapen: super::misshapen(h) }
        }
    }

    pub(super) struct Index {
        pub(super) entries: Vec<Entry>,
        /// Where the last record it covers ends.
        pub(super) end: u64,
    }

    /// The index beside the cache file `cache`.
    pub fn path_of(cache: &Path) -> PathBuf {
        let mut p = cache.as_os_str().to_owned();
        p.push(".heads");
        PathBuf::from(p)
    }

    /// Remove the index beside `cache`: after a compaction, which moves every
    /// record, or with the cache itself.
    pub fn remove(cache: &Path) {
        let _ = std::fs::remove_file(path_of(cache));
    }

    /// FNV-1a: stable across builds, which a std hasher is not promised to be.
    fn hash(b: &[u8]) -> u64 {
        b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| (h ^ x as u64).wrapping_mul(0x0100_0000_01b3))
    }

    fn birth(m: &std::fs::Metadata) -> i64 {
        m.created().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as i64)
    }

    /// The hash of what `f` holds just before `end`.
    fn checked(f: &File, end: u64) -> Option<u64> {
        let from = end.saturating_sub(CHECKED).max(MAGIC.len() as u64);
        let mut b = vec![0u8; end.checked_sub(from)? as usize];
        f.read_exact_at(&mut b, from).ok()?;
        Some(hash(&b))
    }

    /// The index beside `cache`, if it is one for `f` (of `size` bytes) as
    /// `f` is now; see the module's note.
    pub(super) fn load(f: &File, cache: &Path, size: u64) -> Option<Index> {
        let b = std::fs::read(path_of(cache)).ok()?;
        let (body, sum) = b.split_at_checked(b.len().checked_sub(8)?)?;
        if hash(body).to_le_bytes() != sum {
            return None;
        }
        let mut r = Bytes { b: body, at: 0 };
        if r.take(8)? != INDEX_MAGIC || r.take(8)? != MAGIC {
            return None;
        }
        let m = f.metadata().ok()?;
        let (dev, ino, born, end, check) = (r.u64()?, r.u64()?, r.u64()? as i64, r.u64()?, r.u64()?);
        if (dev, ino, born) != (m.dev(), m.ino(), birth(&m)) || end > size || checked(f, end)? != check {
            return None;
        }
        let n = r.u64()? as usize;
        let mut entries = Vec::with_capacity(n.min(body.len() / 64));
        let mut next = MAGIC.len() as u64;
        for _ in 0..n {
            let span = Span { at: r.u64()?, len: r.u64()? };
            let settings = Settings { work_size: r.u32()?, features: r.u32()?, thumb: r.u32()? };
            let key = Key { len: r.u64()?, mtime: r.u64()? as i64, ctime: r.u64()? as i64, dev: r.u64()? };
            let misshapen = r.take(1)? != [0];
            let len = r.u32()? as usize;
            let path = PathBuf::from(std::ffi::OsString::from_vec(r.take(len)?.to_vec()));
            // The records it covers lie end to end from the header.
            if span.at != next || span.len == 0 {
                return None;
            }
            next = span.at + span.len;
            entries.push(Entry { span, settings, key, path, misshapen });
        }
        (next == end && r.at == body.len()).then_some(Index { entries, end })
    }

    /// Write a new index for `f` when this run read past what the old one
    /// covered (`covered`, 0 for none) by `WORTH`, covering what it read
    /// (`seen`, every record to `tail.end`). Nothing is said when it cannot
    /// be written: the next run reads the file whole, as without one.
    pub(super) fn refresh(f: &File, cache: &Path, covered: u64, tail: &Tail, seen: &[Entry]) {
        if tail.broken_at.is_some() || tail.end < covered.max(MAGIC.len() as u64) + WORTH {
            return;
        }
        let Some(m) = f.metadata().ok() else { return };
        let mut next = MAGIC.len() as u64;
        if seen.iter().any(|e| std::mem::replace(&mut next, e.span.at + e.span.len) != e.span.at) || next != tail.end {
            return;
        }
        let Some(check) = checked(f, tail.end) else { return };
        let mut b = Vec::with_capacity(64 + seen.len() * 96);
        b.extend_from_slice(INDEX_MAGIC);
        b.extend_from_slice(MAGIC);
        for v in [m.dev(), m.ino(), birth(&m) as u64, tail.end, check, seen.len() as u64] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for e in seen {
            for v in [e.span.at, e.span.len] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            for v in [e.settings.work_size, e.settings.features, e.settings.thumb] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            for v in [e.key.len, e.key.mtime as u64, e.key.ctime as u64, e.key.dev] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.push(e.misshapen as u8);
            let name = e.path.as_os_str().as_bytes();
            b.extend_from_slice(&(name.len() as u32).to_le_bytes());
            b.extend_from_slice(name);
        }
        b.extend_from_slice(&hash(&b).to_le_bytes());
        let to = path_of(cache);
        let mut tmp = to.as_os_str().to_owned();
        tmp.push(format!(".{}", std::process::id()));
        let tmp = PathBuf::from(tmp);
        if std::fs::write(&tmp, &b).and_then(|()| std::fs::rename(&tmp, &to)).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    struct Bytes<'a> {
        b: &'a [u8],
        at: usize,
    }

    impl<'a> Bytes<'a> {
        fn take(&mut self, n: usize) -> Option<&'a [u8]> {
            let s = self.b.get(self.at..self.at.checked_add(n)?)?;
            self.at += n;
            Some(s)
        }
        fn u64(&mut self) -> Option<u64> {
            Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
        }
        fn u32(&mut self) -> Option<u32> {
            Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
        }
    }
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
            if !walked(&h.path, h.key) {
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

/// Which of `unasked`, the records a run was not asked about, it carries:
/// those whose file is still there, or lies on a drive that is not plugged
/// in. One rule, for a file this run did not walk and for a record made at
/// other settings alike.
///
/// The cache is one file per machine rather than one per corpus, so a run over
/// `~/Pictures` must not throw away what the last run over `~/Downloads` paid
/// for: what the file keeps is this run's analysis *plus* this. Without it a
/// default cache would be worse than none, silently, for anyone who scans two
/// directories in turn. Nor a run at another `--work-size` what this one made,
/// and the other way round.
///
/// The existence check (`gone`) is what keeps the file from growing forever. A record
/// is worth keeping while the file it describes is there to be matched again;
/// once the file is gone the record can never be used, only carried, and it
/// is why `--prune-cache` is a flag rather than the default.
///
/// **A file on a drive that is not plugged in is not gone.** Its records
/// used to count as dead like any other, and once they were a quarter of the
/// file the next scan of anything else compacted them away: a photo library
/// on an external disk, scanned and then unplugged, was analysed from scratch
/// the next time it was plugged in. So a record whose file is missing is kept
/// when the file lies on a volume that is not mounted (`away`). Records
/// made at other settings were asked `gone` alone, and lost there what the
/// rest kept.
fn carried(unasked: &[Unasked]) -> Vec<&Unasked> {
    let gone = gone(unasked.iter().map(|u| u.path.as_path()));
    if gone.is_empty() {
        return unasked.iter().collect();
    }
    let mounts = Mounts::here();
    // Each folder asked once, however many of its records are missing.
    let mut folders: HashMap<&Path, bool> = HashMap::new();
    let mut volumes: HashMap<PathBuf, bool> = HashMap::new();
    let mut kept = Vec::with_capacity(unasked.len());
    for u in unasked {
        if !gone.contains(u.path.as_path()) || u.path.parent().is_some_and(|f| *folders.entry(f).or_insert_with(|| away(&u.path, u.dev, &mounts, &mut volumes))) {
            kept.push(u);
        }
    }
    kept
}

/// Whether a missing file, and so every missing file of its folder, lies on
/// a volume that is not mounted, by any of three signs, each asked once a
/// volume (`volumes`):
///
/// - it is where systems mount drives (`volume_root`), and nothing is there
///   now (`unmounted`): udisks removes the folder it made, so this is the
///   one sign left of a drive under `/media`;
/// - it is under a mount point `/etc/fstab` names, and that is not mounted;
///   when it is, the third sign is still asked, since a share can be
///   mounted inside it (`~/nas` on a `/home` partition: Fedora's default
///   layout lists `/home`, and such a share's records were dropped);
/// - the folder is not there, and the nearest folder above it that is, is
///   empty and nothing is mounted on it: the mount point an sshfs, NFS or
///   SMB share, or a disk mounted by hand, leaves behind (`~/nas`, `/srv`).
///
/// The first was all there was, so a share mounted anywhere else lost every
/// record each time it was away. A folder that is there with the file gone
/// from it is a deletion, whatever is above it.
///
/// **The third asks the device the file lay on (`dev`, the record's).** An
/// empty folder is also what deleting the one folder another held leaves
/// (`rm -r photos/2019` from a `photos` holding nothing else), and those
/// records were kept as a share's, on every run, for good. A share's files
/// lay on another device than the folder it was mounted on; deleted files
/// lay on the folder's own.
fn away(file: &Path, dev: u64, mounts: &Mounts, volumes: &mut HashMap<PathBuf, bool>) -> bool {
    let Some(folder) = file.parent() else { return false };
    let mut asked = |root: PathBuf, empty_too: bool| {
        *volumes.entry(root).or_insert_with_key(|r| !mounts.mounted(r) && (!empty_too || unmounted(r)))
    };
    if volume_root(file).is_some_and(|r| asked(r, true)) {
        return true;
    }
    if mounts.configured(folder).is_some_and(|r| asked(r.to_path_buf(), false)) {
        return true;
    }
    use std::os::unix::fs::MetadataExt;
    left_behind(folder).is_some_and(|r| std::fs::metadata(&r).is_ok_and(|m| m.dev() != dev) && asked(r, true))
}

/// The nearest folder above `folder` that is there, when `folder` is not;
/// not `/`.
fn left_behind(folder: &Path) -> Option<PathBuf> {
    let missing = |p: &Path| matches!(std::fs::symlink_metadata(p), Err(e) if e.kind() == std::io::ErrorKind::NotFound);
    if !missing(folder) {
        return None;
    }
    let mut at = folder.parent()?;
    while missing(at) {
        at = at.parent()?;
    }
    (at.parent().is_some() && at.is_dir()).then(|| at.to_path_buf())
}

/// What is mounted now (`/proc/self/mountinfo`) and where `/etc/fstab` says
/// something is mounted. Both empty where there is no such file.
struct Mounts {
    now: std::collections::HashSet<PathBuf>,
    configured: Vec<PathBuf>,
}

impl Mounts {
    fn here() -> Mounts {
        let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
        Mounts::of(&read("/proc/self/mountinfo"), &read("/etc/fstab"))
    }

    /// From the two files' text: mountinfo's fifth field, fstab's second,
    /// each with its octal escapes (`\040` a space) read.
    fn of(mountinfo: &str, fstab: &str) -> Mounts {
        let field = |line: &str, n: usize| line.split_whitespace().nth(n).map(unescape);
        let now = mountinfo.lines().filter_map(|l| field(l, 4)).collect();
        let configured = fstab
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(|l| field(l, 1))
            .filter(|p| p.is_absolute() && p.parent().is_some())
            .collect();
        Mounts { now, configured }
    }

    fn mounted(&self, p: &Path) -> bool {
        self.now.contains(p)
    }

    /// The deepest mount point `/etc/fstab` names that holds `p`.
    fn configured(&self, p: &Path) -> Option<&Path> {
        self.configured.iter().filter(|m| p.starts_with(m)).max_by_key(|m| m.components().count()).map(PathBuf::as_path)
    }
}

/// A mount table's field, its `\NNN` octal escapes read.
fn unescape(s: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let octal = b.get(i + 1..i + 4).filter(|d| b[i] == b'\\' && d.iter().all(|c| (b'0'..=b'7').contains(c)));
        match octal {
            Some(d) => {
                out.push(d.iter().fold(0u32, |v, c| v * 8 + (c - b'0') as u32) as u8);
                i += 4;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    PathBuf::from(std::ffi::OsString::from_vec(out))
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
fn gone<'a>(paths: impl Iterator<Item = &'a Path>) -> std::collections::HashSet<&'a Path> {
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
    /// to be matched again; at this run's settings or another's alike.
    #[test]
    fn what_a_run_was_not_asked_about_is_carried_while_it_is_there() {
        let dir = scratch("carry");
        let here = dir.join("here.jpg");
        std::fs::write(&here, b"x").unwrap();
        let gone = dir.join("gone.jpg");
        let unasked = |path: &Path, here: bool| Unasked { path: path.to_path_buf(), span: Span { at: 0, len: 0 }, dev: 0, here };
        // And a drive that is not plugged in, which `volume_root` reads off
        // where systems mount them; nothing is mounted at this one.
        let away = Path::new("/media/img-fp-test-nobody/Photos/a.jpg");
        let all = [unasked(&here, true), unasked(&gone, true), unasked(&here, false), unasked(&gone, false), unasked(away, true), unasked(away, false)];
        let kept: Vec<(&Path, bool)> = carried(&all).into_iter().map(|u| (u.path.as_path(), u.here)).collect();
        assert_eq!(kept, [(here.as_path(), true), (here.as_path(), false), (away, true), (away, false)], "the files that are still there");
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

    /// Anywhere else, a share or disk is away when `/etc/fstab` names its
    /// mount point and nothing is mounted there, or when the files' folder
    /// is gone and the nearest folder above it is an empty one nothing is
    /// mounted on. A folder still there, the file gone from it, is a
    /// deletion.
    #[test]
    fn a_record_on_a_share_mounted_anywhere_is_kept_while_it_is_away() {
        let m = Mounts::of(
            "22 1 0:5 / / rw - ext4 /dev/sda1 rw\n40 22 0:9 / /srv/my\\040disk rw - ext4 /dev/sdb1 rw\n",
            "# comment /nowhere\nUUID=a / ext4 defaults 0 1\nUUID=b /srv/my\\040disk ext4 noauto 0 2\nnas:/p /srv/nas nfs noauto 0 0\n/swapfile none swap sw 0 0\n",
        );
        assert!(m.mounted(Path::new("/srv/my disk")) && !m.mounted(Path::new("/srv/nas")));
        assert_eq!(m.configured(Path::new("/srv/nas/2019")), Some(Path::new("/srv/nas")));
        assert_eq!(m.configured(Path::new("/srv/my disk/a")), Some(Path::new("/srv/my disk")));
        assert_eq!(m.configured(Path::new("/home/u")), None, "/ is no drive that comes and goes");
        let mut volumes = HashMap::new();
        assert!(away(Path::new("/srv/nas/2019/a.jpg"), 0, &m, &mut volumes), "named in fstab, not mounted");

        let dir = scratch("left-behind");
        let (nas, here) = (dir.join("nas"), dir.join("here"));
        std::fs::create_dir_all(&nas).unwrap();
        std::fs::create_dir_all(&here).unwrap();
        std::fs::write(here.join("other.jpg"), b"x").unwrap();
        let none = Mounts::of("", "");
        let mut volumes = HashMap::new();
        assert!(away(&nas.join("photos/2019/a.jpg"), 0, &none, &mut volumes), "an empty mount point left behind");
        assert!(!away(&here.join("photos/a.jpg"), 0, &none, &mut volumes), "a folder deleted from one that holds more");
        assert!(!away(&here.join("a.jpg"), 0, &none, &mut volumes), "the folder is there: the file was deleted");
        let mounted = Mounts::of(&format!("1 1 0:1 / {} rw - fuse.sshfs h: rw\n", nas.display()), "");
        assert!(!away(&nas.join("photos/a.jpg"), 0, &mounted, &mut HashMap::new()), "an empty share mounted: the files are gone");
        assert_eq!(left_behind(Path::new("/img-fp-not-there/a")), None, "never /");
        // An empty folder whose own device the files lay on: the folder they
        // were in was deleted from it, and nothing was mounted there.
        use std::os::unix::fs::MetadataExt;
        let own = std::fs::metadata(&nas).unwrap().dev();
        assert!(!away(&nas.join("photos/2019/a.jpg"), own, &none, &mut HashMap::new()), "a folder deleted, its parent left empty");
        // A drive fstab names, mounted: the files are gone from it.
        let disk = dir.join("my disk");
        std::fs::create_dir_all(disk.join("other")).unwrap();
        let on = Mounts::of(&format!("40 22 0:9 / {} rw - ext4 /dev/sdb1 rw\n", disk.display()), &format!("UUID=b {} ext4 noauto 0 2\n", disk.display()));
        assert!(!away(&disk.join("2019/a.jpg"), 0, &on, &mut HashMap::new()), "mounted: the files are gone");
        // The same share's mount point on a partition fstab names and that
        // is mounted, as `/home` is.
        let tmp = std::env::temp_dir();
        let home = Mounts::of(&format!("22 1 0:5 / {} rw - ext4 /dev/sda2 rw\n", tmp.display()), &format!("UUID=h {} ext4 defaults 0 2\n", tmp.display()));
        assert!(away(&nas.join("photos/2019/a.jpg"), 0, &home, &mut HashMap::new()), "a share left behind inside a mounted partition");
        assert!(!away(&here.join("photos/a.jpg"), 0, &home, &mut HashMap::new()), "and a folder deleted there is still deleted");

        // Carried through it, a file this run was not asked about.
        let unasked = Unasked { path: nas.join("photos/a.jpg"), span: Span { at: 0, len: 0 }, dev: 0, here: true };
        assert_eq!(carried(std::slice::from_ref(&unasked)).len(), 1);
        std::fs::write(nas.join("lost.txt"), b"x").unwrap();
        assert_eq!(carried(std::slice::from_ref(&unasked)).len(), 0, "something is there now, and the file is not");
        std::fs::remove_dir_all(&dir).ok();
    }

    const SETTINGS: Settings = Settings { work_size: 384, features: 600, thumb: 128 };

    fn analysis(n: usize, seed: u8) -> (Features, Thumb) {
        let kps = (0..n)
            .map(|i| Keypoint { x: i as f32, y: seed as f32, sigma: 1.6, angle: 0.0 })
            .collect();
        let desc = (0..n * DESC_LEN).map(|i| (i as u8).wrapping_add(seed)).collect();
        (Features { w: 64, h: 48, kps, desc }, Thumb::new(8, 6, 0.125, vec![seed; 48]))
    }

    /// Records the file holds at other settings, as `read` found them.
    fn others(store: &Store) -> usize {
        store.unasked.iter().filter(|u| !u.here).count()
    }

    fn reopen(path: &Path) -> (HashMap<PathBuf, Entry>, Store, bool) {
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = read(path, SETTINGS, &|_, _| true, &mut problems);
        (got, store, problems.any())
    }

    /// A file found under another name than its record's — its folder moved
    /// — is answered by that record when the key is the file's to the
    /// nanosecond, and the record is written again under the new name; a key
    /// a coarse clock could give two files answers for no other name.
    #[test]
    fn records_this_run_does_not_walk_are_framed_not_unpacked() {
        let dir = scratch("walked");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f1, t1) = analysis(3, 1);
        let (f2, t2) = analysis(5, 2);
        store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.put("b.jpg", Key { len: 2, mtime: 2, ctime: 2, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.put("b.jpg", Key { len: 3, mtime: 3, ctime: 3, dev: 0 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        drop(store);
        let (all, _, _) = reopen(&path);

        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = read(&path, SETTINGS, &|p, _| p == Path::new("a.jpg"), &mut problems);
        assert!(!problems.any() && store.records() == 3 && got.len() == 2);
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().feats.desc, f1.desc);
        let b = &got[Path::new("b.jpg")];
        assert!(b.1.is_none());
        assert_eq!((b.0.len, b.2.at, b.2.len), (3, all[Path::new("b.jpg")].2.at, all[Path::new("b.jpg")].2.len));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file under another name than its record's is analysed again, even
    /// when the record's key is its own to the nanosecond: files written
    /// together share one (see `open`).
    #[test]
    fn another_names_record_is_not_taken_whatever_its_key() {
        let dir = scratch("other-name");
        let path = dir.join(file_name());
        let (f, t) = analysis(6, 4);
        let key = Key { len: 10, mtime: 7, ctime: 1_700_000_000_123_456_789, dev: 3 };
        let (_, store, _) = reopen(&path);
        store.put(dir.join("sub1/p1.bmp"), key, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let mut store = open(&path, SETTINGS, vec![(dir.join("sub2/p3.bmp"), Some(key))], &|_| panic!("nothing answers for it"), &mut problems);
        assert!(store.take(0).is_none());
        drop(store);
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
        store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.put("b.jpg", Key { len: 2, mtime: 2, ctime: 2, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.put("a.jpg", Key { len: 3, mtime: 3, ctime: 3, dev: 0 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
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
        let a = store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &jpeg, &f, &t).unwrap();
        let b = store.put("b.png", Key { len: 2, mtime: 2, ctime: 2, dev: 0 }, (320, 240), &png, &f, &t).unwrap();
        let c = store.put("c.jpg", Key { len: 3, mtime: 3, ctime: 3, dev: 0 }, (320, 240), &jpeg, &f, &t).unwrap();
        let measured = kept_colour(12.345);
        assert_eq!(kept_colour(measured), measured, "the same value through any number of rounds");
        store.write_colour(a, Path::new("a.jpg"), measured);
        store.write_colour(b, Path::new("b.png"), 3.0);
        store.write_colour(c, Path::new("not-c.jpg"), 3.0);
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
        store.write_colour(got[Path::new("c.jpg")].2, Path::new("c.jpg"), 2.0);
        store.write_colour(got[Path::new("b.png")].2, Path::new("b.png"), 2.0);
        drop(store);
        let (again, _, bad) = reopen(&path);
        assert!(!bad && again.len() == 2);
        assert_eq!(traits(&again, "c.jpg").colour_kept, Some(2.0));
        assert_eq!(traits(&again, "a.jpg").colour_kept, Some(measured));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A cache that can be read and not written still answers the run, and
    /// still counts what it holds: `stale` takes the records it superseded as
    /// the difference between that count and the map, which was an underflow
    /// when the count came back as zero.
    #[test]
    fn a_read_only_cache_counts_what_it_holds() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("readonly");
        let path = dir.join(file_name());
        let (_, store, _) = reopen(&path);
        let (f1, t1) = analysis(3, 1);
        store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.put("a.jpg", Key { len: 2, mtime: 2, ctime: 2, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.put("b.jpg", Key { len: 3, mtime: 3, ctime: 3, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
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
        store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.put("b.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        for cut in [1, 9, b.len / 2, b.len - 1] {
            let file = OpenOptions::new().write(true).open(&path).unwrap();
            file.set_len(b.at + cut).unwrap();
            drop(file);
            let (got, store, bad) = reopen(&path);
            assert!(!bad, "a torn tail is not a damaged cache");
            assert_eq!(got.keys().collect::<Vec<_>>(), [Path::new("a.jpg")]);
            assert_eq!(std::fs::metadata(&path).unwrap().len(), b.at);
            store.put("b.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
            drop(store);
            let (got, _, bad) = reopen(&path);
            assert!(!bad && got.len() == 2);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Where a record's packed length sits: after its path, its settings, the
    /// key, the picture's size and the six fields of its shape.
    /// The bytes of a record's head before its keypoint count: the path,
    /// the settings, the key, the device, the dimensions and the traits.
    const TRAITS: u64 = 23 * 4 + 4;
    fn packed_len_at(s: Span, path: &str) -> u64 {
        s.at + 8 + path.len() as u64 + 12 + 24 + 8 + 8 + TRAITS + 6 * 4
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
        store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.put("b.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        store.put("c.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
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
        store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.put("b.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
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
        store.put("other/x.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        store.put("a.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let b = store.put("b.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        let shape = store.put("c.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);
        let file = OpenOptions::new().write(true).open(&path).unwrap();
        // b's first stream: its length is intact, its deflate is not.
        let body = packed_len_at(b, "b.jpg") + 8 + 8;
        file.write_all_at(&[0xff; 24], body).unwrap();
        // c claims more keypoints than a run of these settings describes.
        file.write_all_at(&(SETTINGS.features + 1).to_le_bytes(), shape.at + 8 + 5 + 12 + 24 + 8 + 8 + TRAITS + 8).unwrap();
        drop(file);
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = read(&path, SETTINGS, &|p, _| !p.starts_with("other"), &mut problems);
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
        store.put("wide.jpg", Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (3200, 240), &Traits::default(), &f, &t).unwrap();
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
            names.iter().map(|p| store.put(p, Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap()).collect();
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
        let key = Key { len: 1, mtime: 1, ctime: 1, dev: 0 };
        let (_, mut a, _) = reopen(&path);
        let (_, b, _) = reopen(&path);
        a.put("old.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        // `a` drops `old.jpg` while `b` has the file: it is left as it is.
        assert!(!a.compact(&mut []).unwrap(), "put off");
        b.put("b.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
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
        let key = Key { len: 1, mtime: 1, ctime: 1, dev: 0 };
        let newer = Key { len: 2, mtime: 2, ctime: 2, dev: 0 };
        let (_, mut a, _) = reopen(&path);
        let mine = a.put("mine.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        a.put("dead.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        let shared = a.put("shared.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        let (_, b, _) = reopen(&path);
        b.put("other.jpg", key, (320, 240), &Traits::default(), &f, &t).unwrap();
        // The other run described a file this one has a record for, after it.
        b.put("shared.jpg", newer, (320, 240), &Traits::default(), &f2, &t2).unwrap();
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

    /// What a run keeps is what it took and what it made, and what it was
    /// not asked about while its file is there: a walked file's record that
    /// no longer fits it, or that the run left untaken (a copy whose bytes
    /// another file answers for), is not kept, nor one for a file that has
    /// gone. A colour kept afterwards goes into the record where it went.
    #[test]
    fn a_run_keeps_what_it_took_and_made() {
        let dir = scratch("run");
        let path = dir.join(file_name());
        let [a, b, c, other, gone] = ["a.jpg", "b.jpg", "c.jpg", "other.jpg", "gone.jpg"].map(|n| dir.join(n));
        for f in [&a, &b, &c, &other] {
            std::fs::write(f, b"x").unwrap();
        }
        let (f, t) = analysis(6, 4);
        let (f2, t2) = analysis(3, 9);
        let jpeg = Traits { colour: UNMEASURED, ..Default::default() };
        let (old, new) = (Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, Key { len: 2, mtime: 2, ctime: 2, dev: 0 });
        let (_, store, _) = reopen(&path);
        for p in [&a, &b, &c, &other, &gone] {
            store.put(p, old, (320, 240), &jpeg, &f, &t).unwrap();
        }
        drop(store);

        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let files = vec![(a.clone(), Some(old)), (b.clone(), Some(old)), (c.clone(), Some(new))];
        let mut store = open(&path, SETTINGS, files, &|_| (), &mut problems);
        assert_eq!(store.usable(), 5);
        assert!(store.take(0).is_some_and(|r| r.feats.desc == f.desc));
        assert!(store.take(0).is_none(), "taken once");
        assert!(store.take(2).is_none(), "c has changed since its record");
        assert!(store.stale(), "b's record is not taken and c's no longer fits");
        store.append(2, (320, 240), &jpeg, &f2, &t2);
        let kept = store.finish(false);
        assert_eq!((kept.carried, kept.other_sizes, kept.dropped), (1, 0, 1), "other.jpg carried, gone.jpg not");
        assert!(matches!(kept.compacted, Compacted::Done), "three of six records are dead");
        store.keep_colour(0, 5.0);
        store.keep_colour(1, 5.0);
        drop(store);

        let (got, store, bad) = reopen(&path);
        assert!(!bad && store.records() == 3);
        let mut names: Vec<&Path> = got.keys().map(PathBuf::as_path).collect();
        names.sort();
        assert_eq!(names, [a.as_path(), c.as_path(), other.as_path()]);
        assert_eq!(got[&c].0, new);
        assert_eq!(got[&c].1.as_ref().unwrap().feats.desc, f2.desc);
        assert_eq!(got[&a].1.as_ref().unwrap().traits.colour_kept, Some(5.0), "written where the compaction put it");
        drop(store);

        // Pruned: only what the run took or made.
        let mut store = open(&path, SETTINGS, vec![(a.clone(), Some(old))], &|_| (), &mut problems);
        assert!(store.take(0).is_some() && !store.stale());
        let kept = store.finish(true);
        assert_eq!((kept.carried, kept.dropped), (0, 2));
        drop(store);
        let (got, _, _) = reopen(&path);
        assert_eq!(got.keys().collect::<Vec<_>>(), [&a]);
        assert!(!problems.any());
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
        let m = [
            Unasked { path: f.clone(), span: Span { at: 8, len: 10 }, dev: 0, here: true },
            Unasked { path: dir.join("deleted.jpg"), span: Span { at: 18, len: 10 }, dev: 0, here: false },
        ];
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let reachable = std::fs::metadata(&f).is_ok();
        let kept: Vec<PathBuf> = carried(&m).into_iter().map(|u| u.path.clone()).collect();
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
            .map(|p| store.put(p, Key { len: 7, mtime: 7, ctime: 7, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap())
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
            assert!(store.put("d.jpg", Key { len: 7, mtime: 7, ctime: 7, dev: 0 }, (320, 240), &Traits::default(), &f, &t).is_none());
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
        store.put("a.jpg", Key { len: 7, mtime: 7, ctime: 7, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        store.put("b.jpg", Key { len: 7, mtime: 7, ctime: 7, dev: 0 }, (320, 240), &Traits::default(), &f, &t).unwrap();
        drop(store);

        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let other = Settings { work_size: 640, ..SETTINGS };
        let (got, store) = read(&path, other, &|_, _| true, &mut problems);
        assert!(got.is_empty() && !problems.any());
        assert_eq!((store.records(), others(&store)), (2, 2));
        let (f2, t2) = analysis(3, 9);
        store.put("a.jpg", Key { len: 7, mtime: 7, ctime: 7, dev: 0 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        drop(store);

        // Back at the first size: its two records, and the 640 one carried.
        let (got, store, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!((got.len(), store.records(), others(&store)), (2, 3, 1));
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().feats.desc, f.desc);
        assert_eq!(got[Path::new("a.jpg")].1.as_ref().unwrap().dims, (320, 240));
        // A compaction that keeps both kinds keeps both.
        let mut store = store;
        let mut keep: Vec<(&Path, Span)> = got.iter().map(|(p, e)| (p.as_path(), e.2)).collect();
        let others: Vec<(PathBuf, Span)> = store.unasked.iter().map(|u| (u.path.clone(), u.span)).collect();
        keep.extend(others.iter().map(|(p, s)| (p.as_path(), *s)));
        assert!(store.compact(&mut keep).unwrap());
        drop(store);
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = read(&path, other, &|_, _| true, &mut problems);
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
        store.put(&fe, Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f1, &t1).unwrap();
        store.put(&ff, Key { len: 1, mtime: 1, ctime: 1, dev: 0 }, (320, 240), &Traits::default(), &f2, &t2).unwrap();
        drop(store);
        let (got, _, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!(got.len(), 2);
        assert_eq!(got[&fe].1.as_ref().unwrap().feats.desc, f1.desc);
        assert_eq!(got[&ff].1.as_ref().unwrap().feats.desc, f2.desc);
        std::fs::remove_dir_all(&dir).ok();
    }
}

