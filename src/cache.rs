//! On-disk cache of the per-image analysis.
//!
//! Describing an image is by far the most expensive stage, and it depends only
//! on the file's bytes and the analysis that reads them. Caching it makes a
//! rescan of a mostly-unchanged directory nearly free, and makes tuning the
//! matching stages practical.
//!
//! **An analysis is kept by what the file holds, not by where it is.** Each is
//! stored under its file's content hash (BLAKE3, `same::Hash`), the analysis
//! that made it (`analysis::version`) and the settings it was made at. Which
//! file holds which content is a second table: canonical path, the stat key
//! the walk read (size, modification and change time), the hash, and a
//! sample of the bytes (`decode::sample`: three pieces of the file, which is
//! all a file of a size the index knows is read for until its sample says it
//! may be a file the index knows). A file
//! whose key is the one kept takes its hash from there and is not read at
//! all; any other is hashed (read, not decoded) and its analysis found by
//! what it holds. So a moved, renamed, chmod'ed or newly hard-linked file, a
//! folder mounted elsewhere or a backup restored costs a read, not an
//! analysis; copies share one analysis; and the stat key only ever says
//! whether *this path* still holds what it held, never which file another
//! path is. Until 0.82.0 a record was keyed by path and stat key alone, every
//! one of those was analysed again, and the one attempt to find a moved file
//! by its key (0.69.0) gave two pictures one analysis, since files written
//! together share a change time to the tick.
//!
//! **Two files, each for what it does best.** The index is SQLite: small
//! rows, looked up and changed by many runs at once, kept whole across a
//! crash. The analyses are an append-only pack, read with one `pread` each:
//! measured on 9,000 records of 35 KB, SQLite holding them as blobs was 2.7%
//! larger than the records, 3-4x slower to write and twice as slow to read
//! cold, against 0.3%, 0.24 s and 0.35 s for appends to a file. The pack holds
//! nothing but records, each beginning with its hash, which a read checks: an
//! index and a pack that disagree cost a record, never a wrong analysis.
//!
//! Beside a cache `X`: `X.lock` (held shared by every run that has the cache,
//! and alone by one that rewrites the packs), `X.pack.N` (the packs), and
//! `X.matched` (`matched.rs`). A run appends to the newest pack when no other
//! run is writing to it, and to a pack of its own when one is; a run that has
//! the cache alone folds such packs into the newest, and rewrites the packs
//! when what nothing reads is a quarter of them.
//!
//! **The settings are a record's.** A run reads the records made at its own
//! working size and thumbnail and carries the rest, so a run with `--fast`
//! costs the default's records nothing. A `--detail` is not a setting of its
//! own: a record at any detail is read by a run at another as far as that run
//! goes (`Settings`), and the finer replaces the coarser.
//!
//! **What a record costs.** An analysis is 128 bytes of descriptor and 16 of
//! keypoint for each of up to 600 keypoints, plus a thumbnail of up to
//! 128x128: ~144 bytes a keypoint, and a *small* photograph is the worst case,
//! since `upsample_below` enlarges it first (a 224x224 file yields some 530
//! keypoints, 95 KB raw for a 25 KB JPEG). The packing (`pack`):
//!
//! - **The descriptors are near-incompressible.** Their bytes carry 5.81 bits
//!   of order-0 entropy each; the best context model tried reaches 5.48. zstd
//!   at its fastest takes them to 0.765 of raw where deflate took 0.80.
//! - **The keypoints are four f32 fields and compress as bytes, split by byte
//!   position** (all exponent bytes together, and so on): 0.763 against 0.823
//!   interleaved, under deflate.
//! - **The thumbnail is a picture and wants a picture's predictor**: Paeth
//!   filtered first, 0.681 against 0.852 raw, under deflate.
//!
//! Each in a zstd stream of its own at level 1. On 3,000 records of the found
//! corpus that is 0.709 of raw against deflate's 0.730, packed 3.2x and
//! unpacked 4x faster (deflate was most of a cached run's cache read). Levels
//! 2-5 are larger as well as slower: the descriptors leave an LZ nothing.
//!
//! **A record is appended the moment it exists**, and its index row is
//! committed as soon as the index is free (`Db::commit_waiting`; rows that
//! come during a commit go in the next), or at an interrupt (`seal`). So
//! Ctrl-C costs nothing, and a process killed outright the rows of the commit
//! under way: what the analysis finished is kept.

use crate::analysis::Item;
use crate::decode::Traits;
use crate::problems::{Note, Problems};
use crate::same::{Content, Hash};
use crate::sift::{Features, Keypoint, DESC_LEN};
use crate::thumb::Thumb;
use anyhow::{anyhow, bail, Result};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

/// The index's schema, as its file says (`PRAGMA user_version`) and as the
/// default cache's name carries it. Only the index's own layout and the
/// pack's are this number's: what an analysis is made by is kept on every
/// record (`analysis::version`), so a new analysis needs no new number.
///
/// D01: the index and packs of 0.82.0. Before it, `IMGFPC04`-`IMGFPC21`, one
/// file of records keyed by path (see `remove_other_formats`).
const SCHEMA: i64 = 1;

/// The format this build reads and writes, for messages and the default name.
pub const FORMAT: &str = "IMGFPD01";

/// What an index of img-fp's says it is (`PRAGMA application_id`): `IMFP`.
const APPLICATION_ID: i64 = 0x494d_4650;

/// The prefix of every cache file img-fp wrote before 0.82.0.
const OLD_PREFIX: &[u8; 6] = b"IMGFPC";

/// How many days an analysis is kept for a file that has gone, and one made
/// by another analysis is kept unused (user's decision, 0.82.0). A file moved
/// and scanned at its new place within that keeps its analysis, and so does a
/// folder on a drive left unplugged that long; two versions of img-fp used in
/// turn each keep theirs. A policy, not a measurement: it says how long a
/// cache remembers what it has not seen, against how much it holds for it.
pub const GRACE_DAYS: i64 = 30;

/// Records unpacked in one parallel batch: large enough that the threads are
/// not synchronising over nothing, small enough that what is held at once is
/// megabytes, not the corpus.
const READ_BATCH: usize = 64;

/// The bytes of the hash a record begins with, which a read checks against
/// the index's: enough that a record read at the wrong place, or from a pack
/// another has replaced, is never taken for another's.
const STAMP: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
/// The settings a run's analyses are made at. Only those a run can change;
/// the analysis's own constants are in `analysis::version`.
///
/// A run reads a record made at its own working size and thumbnail, whatever
/// the `--detail`: one at a finer detail holds the default's analysis and its
/// levels after it (`Features::ends`), and a run reads it as far as its own
/// detail goes, or analyses the file again where it holds fewer levels than
/// the picture has up to it (`analysis::cached_at`); the record made then
/// replaces it, under the same key.
pub struct Settings {
    pub work_size: u32,
    /// The most keypoints a record holds: `FEATURES` at the default, more at
    /// a finer `--detail` (`analysis::features_at`). What a record is checked
    /// against (`misshapen`), not what a run reads it by (`reads_as`).
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

/// What says a path still holds what it held: its size, its modification time
/// and its change time, in nanoseconds.
///
/// **The change time is the one that cannot be put back.** Size and mtime are
/// both under the user's control (`cp -p`, `rsync -a`, `touch -r`, `exiftool
/// -P`), so a file rewritten with another picture of the same size would keep
/// its hash. The kernel sets ctime on every write, rename and chmod, and
/// nothing in userspace sets it back. A changed key costs a read of the file
/// to hash it, not an analysis: a chmod or a rename is the same content.
///
/// It is only ever compared for one path: which content another path holds
/// is its hash's to say (see the head of the file).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub len: u64,
    pub mtime: i64,
    pub ctime: i64,
}

/// The default cache's name, which carries the schema: `analysis-IMGFPD01.db`.
///
/// **Each schema has a default file of its own**, so a new one starts a new
/// file beside the old instead of meeting it, and two versions installed side
/// by side each keep their own. Other schemas' files in the default folder
/// are removed (`remove_other_formats`), unless a run is using one.
pub fn file_name() -> String {
    format!("analysis-{FORMAT}.db")
}

/// The files of cache `path` beside it, which no output may name: its lock,
/// its journal, its packs and the kept matching.
pub fn files_beside(path: &Path) -> Vec<PathBuf> {
    let mut out = vec![lock_path(path), beside(path, "-journal"), beside(path, "-wal"), beside(path, "-shm"), crate::matched::path_beside(path)];
    out.extend(packs_on_disk(path).into_iter().map(|(_, p)| p));
    out
}

/// The bytes cache `path` holds, index and packs: what the progress bar
/// expects a read of it may cost.
pub fn bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len()) + packs_on_disk(path).iter().map(|(_, p)| std::fs::metadata(p).map_or(0, |m| m.len())).sum::<u64>()
}

/// `X.lock`, beside cache `X`.
fn lock_path(path: &Path) -> PathBuf {
    beside(path, ".lock")
}

/// `path` with `suffix` added to its name.
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(suffix);
    PathBuf::from(p)
}

/// The pack numbered `id` of cache `path`: `X.pack.N`.
fn pack_path(path: &Path, id: i64) -> PathBuf {
    beside(path, &format!(".pack.{id}"))
}

/// The packs of cache `path` there are on disk, by number, whatever the
/// index says of them.
fn packs_on_disk(path: &Path) -> Vec<(i64, PathBuf)> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return Vec::new() };
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    let mut prefix = name.as_bytes().to_vec();
    prefix.extend_from_slice(b".pack.");
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let n = e.file_name();
            let id = n.as_bytes().strip_prefix(&prefix[..])?;
            let id: i64 = std::str::from_utf8(id).ok()?.parse().ok()?;
            Some((id, e.path()))
        })
        .collect()
}

/// Remove the caches of other formats from the default folder, with the
/// files beside them; each removed, with its format and the bytes it held.
///
/// Each format change left the last one's file behind, read by nothing (on
/// the author's machine a 2.5 GB `IMGFPC15` beside the 1.6 GB current one),
/// and the window's users never see the folder. A file another run holds is
/// in use (an older version running now) and is left. Only files named as a
/// default cache is and holding a cache's header: a file a person put there
/// is not touched.
pub fn remove_other_formats() -> Vec<(String, u64)> {
    let dir = default_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mine = file_name();
    let mut removed = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        let path = e.path();
        if name == "analysis.bin" || (name.starts_with("analysis-IMGFPC") && name.ends_with(".bin")) {
            // Before 0.82.0: one file, held with `flock` by every run using it.
            let Ok(f) = File::open(&path) else { continue };
            let mut magic = [0u8; 8];
            if !f.metadata().is_ok_and(|m| m.is_file()) || f.read_exact_at(&mut magic, 0).is_err() || &magic[..OLD_PREFIX.len()] != OLD_PREFIX {
                continue;
            }
            if !flock(&f, LOCK_EX | LOCK_NB) {
                continue;
            }
            let bytes = f.metadata().map_or(0, |m| m.len());
            if std::fs::remove_file(&path).is_ok() {
                for s in [".heads", ".matched", ".same"] {
                    let _ = std::fs::remove_file(beside(&path, s));
                }
                removed.push((String::from_utf8_lossy(&magic).into_owned(), bytes));
            }
        } else if name != mine && name.starts_with("analysis-IMGFPD") && name.ends_with(".db") {
            let Some(format) = name.strip_prefix("analysis-").and_then(|n| n.strip_suffix(".db")) else { continue };
            if schema_of(&path).is_none() {
                continue;
            }
            let Ok(lock) = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path(&path)) else { continue };
            if !flock(&lock, LOCK_EX | LOCK_NB) {
                continue;
            }
            let bytes = std::fs::metadata(&path).map_or(0, |m| m.len()) + packs_on_disk(&path).iter().map(|(_, p)| std::fs::metadata(p).map_or(0, |m| m.len())).sum::<u64>();
            remove_all(&path);
            removed.push((format.to_string(), bytes));
        }
    }
    removed.sort();
    removed
}

/// Remove cache `path` and every file of it: the index, its journal, the
/// packs, the lock and the kept matching.
fn remove_all(path: &Path) {
    for (_, p) in packs_on_disk(path) {
        let _ = std::fs::remove_file(p);
    }
    for s in ["-journal", "-wal", "-shm", ".matched", ".lock"] {
        let _ = std::fs::remove_file(beside(path, s));
    }
    let _ = std::fs::remove_file(path);
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
/// `--cache` names the FILE (the index; its packs go beside it). A path that
/// is already a directory, or that is written with a trailing slash, is
/// treated as one and gets the default name inside it. Missing parents are
/// created either way.
///
/// Nothing here is fatal: a run with no cache finds exactly the same pairs,
/// only slower. A directory that cannot be made is a problem, said out loud,
/// and the run goes on without one.
///
/// **A file there that is not a cache is not one to use**, nor to delete:
/// `--cache photo.jpg` (or the window's cache chooser, pointed at a photo)
/// once replaced the photo with an empty cache. Such a run has no cache, and
/// says why.
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

/// `--clear-cache`: delete the cache at `path` (one `resolve_path` gave, so a
/// cache or nothing) with every file of it.
pub fn clear(path: &Path, problems: &mut Problems) {
    let was = path.exists();
    remove_all(path);
    // The files of a cache from before 0.82.0 named by `--cache`.
    for s in [".heads", ".same"] {
        let p = beside(path, s);
        let mut head = [0u8; 5];
        if File::open(&p).and_then(|f| f.read_exact_at(&mut head, 0)).is_ok() && &head == b"IMGFP" {
            let _ = std::fs::remove_file(&p);
        }
    }
    if was && path.exists() {
        // Asked for and not done, and the next run will read the cache this
        // one meant to be rid of.
        problems.note(Note::Cache, format_args!("could not delete {}", path.display()));
    }
}

/// What is at `path`, as a cache: nothing (or an empty file) to make one in,
/// an index of `SCHEMA` or of another, or a cache from before 0.82.0.
enum Found {
    Nothing,
    Mine,
    Other(String),
    Stranger,
}

/// What `path` holds, by its first bytes.
fn found_at(path: &Path) -> Found {
    let f = match std::fs::metadata(path) {
        Ok(m) if m.is_file() => match File::open(path) {
            Ok(f) => f,
            // Unreadable is for the opening to say.
            Err(_) => return Found::Nothing,
        },
        Ok(_) => return Found::Stranger,
        Err(_) => return Found::Nothing,
    };
    let mut head = [0u8; 100];
    let mut got = 0;
    while got < head.len() {
        match f.read_at(&mut head[got..], got as u64) {
            Ok(0) => break,
            Ok(k) => got += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Found::Nothing,
        }
    }
    let head = &head[..got];
    if head.is_empty() {
        return Found::Nothing;
    }
    if head.starts_with(OLD_PREFIX) {
        return Found::Other(String::from_utf8_lossy(&head[..head.len().min(8)]).into_owned());
    }
    // An SQLite file: its header, then the application id (offset 68) and
    // user version (60), big-endian.
    if got == 100 && head.starts_with(b"SQLite format 3\0") {
        let word = |at: usize| i32::from_be_bytes(head[at..at + 4].try_into().unwrap()) as i64;
        if word(68) == APPLICATION_ID {
            return if word(60) == SCHEMA { Found::Mine } else { Found::Other(format!("IMGFPD{:02}", word(60))) };
        }
    }
    Found::Stranger
}

/// The schema of an index of img-fp's at `path`, any.
fn schema_of(path: &Path) -> Option<()> {
    matches!(found_at(path), Found::Mine | Found::Other(_)).then_some(())
}

/// Whether something is at `path` that is not a cache of img-fp's, of any
/// version: a file that does not begin as one, or anything not a regular
/// file. Nothing there, or an empty file, is not a stranger.
fn is_stranger(path: &Path) -> bool {
    matches!(found_at(path), Found::Stranger)
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
/// already read for every file it lists.
///
/// Both times are read as the kernel keeps them, seconds and nanoseconds,
/// signed: a file dated before 1970 has a key like any other.
pub fn key_from(md: &std::fs::Metadata) -> Option<Key> {
    use std::os::unix::fs::MetadataExt;
    let mtime = md.mtime().checked_mul(1_000_000_000)?.checked_add(md.mtime_nsec())?;
    let ctime = md.ctime().checked_mul(1_000_000_000)?.checked_add(md.ctime_nsec())?;
    Some(Key { len: md.len(), mtime, ctime })
}

/// Today, as days since 1970: what the grace period is counted in.
fn today() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| (d.as_secs() / 86_400) as i64)
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

/// One record's variable-length half: three zstd streams, each carrying data
/// laid out the way its own redundancy runs (see the head of the file), then
/// where each finer level ends, for an analysis at a finer `--detail`.
///
/// Three, because one stream has one model and these have three
/// distributions: keypoint bytes, descriptor bytes and a picture's residuals.
fn pack(f: &Features, t: &Thumb) -> Result<Vec<u8>> {
    let n = f.kps.len();
    let mut planes = Vec::with_capacity(n * FIELDS * 4);
    let mut bits: Vec<[u8; 4]> = Vec::with_capacity(n);
    for field in 0..FIELDS {
        bits.clear();
        bits.extend(f.kps.iter().map(|kp| field_of(kp, field).to_le_bytes()));
        for byte in 0..4 {
            planes.extend(bits.iter().map(|v| v[byte]));
        }
    }
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
    let mut out = Vec::with_capacity(n * DESC_LEN);
    for stream in [&planes[..], &f.desc[..], &resid[..]] {
        let z = ZSTD.with(|z| z.borrow_mut().0.compress(stream))?;
        out.extend_from_slice(&(z.len() as u32).to_le_bytes());
        out.extend_from_slice(&z);
    }
    if !f.ends.is_empty() {
        out.extend_from_slice(&(f.ends.len() as u32).to_le_bytes());
        for e in &f.ends {
            out.extend_from_slice(&e.to_le_bytes());
        }
    }
    Ok(out)
}

thread_local! {
    /// Each thread's zstd contexts, made once: a context made for each stream
    /// (three a record) cost more than unpacking it.
    static ZSTD: std::cell::RefCell<(zstd::bulk::Compressor<'static>, zstd::bulk::Decompressor<'static>)> =
        std::cell::RefCell::new((zstd::bulk::Compressor::new(1).expect("a zstd context"), zstd::bulk::Decompressor::new().expect("a zstd context")));
}

/// The inverse, given the shape the record's head carried.
fn unpack(blob: &[u8], n: usize, tw: u16, th: u16) -> Result<(Vec<Keypoint>, Vec<u8>, Vec<u8>, Vec<u32>)> {
    let (w, h) = (tw as usize, th as usize);
    let mut at = 0;
    let mut stream = |want: usize| -> Result<Vec<u8>> {
        let len = blob.get(at..at + 4).ok_or_else(|| anyhow!("record truncated"))?;
        let len = u32::from_le_bytes(len.try_into().unwrap()) as usize;
        at += 4;
        let z = blob.get(at..at + len).ok_or_else(|| anyhow!("record truncated"))?;
        at += len;
        // One byte more than wanted, so a stream holding more is told from
        // one holding exactly enough.
        let raw = ZSTD.with(|c| c.borrow_mut().1.decompress(z, want + 1))?;
        if raw.len() != want {
            bail!("a record's stream is {} bytes unpacked, not {want}", raw.len());
        }
        Ok(raw)
    };
    let planes = stream(n * FIELDS * 4)?;
    let desc = stream(n * DESC_LEN)?;
    let resid = stream(w * h)?;
    let mut ends = Vec::new();
    if at < blob.len() {
        let word = |i: usize| -> Result<u32> {
            let b = blob.get(i..i + 4).ok_or_else(|| anyhow!("record truncated"))?;
            Ok(u32::from_le_bytes(b.try_into().unwrap()))
        };
        let k = word(at)? as usize;
        if k > 8 || at + 4 + 4 * k != blob.len() {
            bail!("a record's levels are damaged");
        }
        for i in 0..k {
            let e = word(at + 4 + 4 * i)?;
            if e as usize > n || ends.last().is_some_and(|&l| e < l) {
                bail!("a record's levels are damaged");
            }
            ends.push(e);
        }
    }

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
    Ok((kps, desc, px, ends))
}

/// The fixed head of a record in a pack, after its stamp: the picture's size,
/// its traits, and the analysis's shape.
const HEAD: usize = 8 + 23 * 4 + 4 + 12 + 8 + 4;

/// A whole record as a pack holds it: the stamp (the first `STAMP` bytes of
/// its file's hash), the head, the packed analysis.
fn record(hash: &Hash, dims: (u32, u32), tr: &Traits, f: &Features, t: &Thumb) -> Result<Vec<u8>> {
    let blob = pack(f, t)?;
    let mut b = Vec::with_capacity(STAMP + HEAD + blob.len());
    b.extend_from_slice(&hash[..STAMP]);
    let u32s = |b: &mut Vec<u8>, vs: &[u32]| vs.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
    u32s(&mut b, &[dims.0, dims.1]);
    for v in tr.detail.iter().chain([&tr.colour]).chain(tr.grid.iter()) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    let flags = tr.lossless as u32 | (tr.exif as u32) << 1 | (tr.frames as u32) << 2 | (tr.deep as u32) << 3 | (tr.profile as u32) << 4;
    u32s(&mut b, &[flags, f.w, f.h, f.kps.len() as u32, t.w as u32, t.h as u32]);
    b.extend_from_slice(&t.scale.to_le_bytes());
    b.extend_from_slice(&blob);
    Ok(b)
}

/// A record read back from its bytes: checked against its hash and the
/// settings it was made at, and unpacked into the run's scratch file.
fn unrecord(b: &[u8], hash: &Hash, s: Settings) -> Result<Record> {
    if b.len() < STAMP + HEAD || b[..STAMP] != hash[..STAMP] {
        bail!("a record is not where the index says");
    }
    let mut at = STAMP;
    let mut u32_ = || {
        let v = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        at += 4;
        v
    };
    let dims = (u32_(), u32_());
    let mut traits = Traits::default();
    let fields: Vec<f32> = (0..23).map(|_| f32::from_bits(u32_())).collect();
    traits.detail.copy_from_slice(&fields[..6]);
    traits.colour = fields[6];
    traits.grid.copy_from_slice(&fields[7..]);
    let flags = u32_();
    traits.lossless = flags & 1 != 0;
    traits.exif = flags & 2 != 0;
    traits.frames = flags & 4 != 0;
    traits.deep = flags & 8 != 0;
    traits.profile = flags & 16 != 0;
    let (w, h, n) = (u32_(), u32_(), u32_() as usize);
    let (tw, th) = (u32_(), u32_());
    let scale = f32::from_bits(u32_());
    if misshapen(s, n, tw, th) {
        bail!("a record's shape is not one these settings make");
    }
    let (kps, desc, px, ends) = unpack(&b[at..], n, tw as u16, th as u16)?;
    let feats = Features { w, h, kps: kps.into(), desc: desc.into(), ends }.spilled();
    let thumb = Thumb::new(tw as u16, th as u16, scale, px).spilled();
    Ok(Record { feats, thumb, dims, traits })
}

/// A shape no record at `s` can have: more keypoints than it allows, or a
/// thumbnail larger than it makes.
fn misshapen(s: Settings, n: usize, tw: u32, th: u32) -> bool {
    let (tw, th) = (tw as u64, th as u64);
    let (thumb, sides) = (s.thumb as u64, s.thumb as u64 * crate::THUMB_MAX_SIDES as u64);
    n > s.features as usize || tw * th > thumb * thumb || tw.max(th) > sides
}

/// A JPEG's colour as the suggestion measured it, kept on its index row and
/// given back. Not quite the colour measured, since it went through the
/// representation an earlier format kept it in (`-2 - c`); but the same value
/// again through any number of rounds, so a run deciding by it decides as a
/// later run reading it will.
pub fn kept_colour(c: f32) -> f32 {
    const UNKEPT: f32 = -2.0;
    UNKEPT - (UNKEPT - c)
}

// ------------------------------------------------------------ interruption

/// Held for the length of every append, and by the interrupt handler from the
/// moment it runs until the process is gone: a record is in its pack whole,
/// and its row committed, or neither is relied on.
static WRITING: Mutex<()> = Mutex::new(());

/// The temporary files a report or a pack is being written into, while there
/// are any: the handler removes them, since what they would have replaced is
/// still there and still whole.
static PARTIAL: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Records appended by this run, for the handler to report.
static APPENDED: AtomicUsize = AtomicUsize::new(0);

/// The indexes open in this process, for the handler to commit what waits.
static OPEN: Mutex<Vec<Weak<Db>>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Make the cache final, for a process about to exit on a signal, and say how
/// many records this run put in it.
///
/// Every record went into its pack the moment its worker finished it; what
/// is left is the rows that came while a commit was under way, committed
/// here.
/// The returned guard is held until the process exits, so no worker appends
/// after this.
pub fn seal() -> (MutexGuard<'static, ()>, usize) {
    let held = lock(&WRITING);
    for db in lock(&OPEN).iter().filter_map(Weak::upgrade) {
        db.flush();
    }
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

// ------------------------------------------------------------ locks

const LOCK_SH: i32 = 1;
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;

fn flock(f: &File, op: i32) -> bool {
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    // SAFETY: a descriptor this process holds open.
    unsafe { flock(f.as_raw_fd(), op) == 0 }
}

// ------------------------------------------------------------ the index

/// The index, open, and the rows this run has made and not yet committed.
struct Db {
    conn: Mutex<Connection>,
    pending: Mutex<Vec<Row>>,
    failed: Mutex<Option<String>>,
    /// The index said it is damaged: `finish` removes it, when the run has
    /// the cache alone, so the next run starts a new one instead of failing
    /// on it every time.
    damaged: std::sync::atomic::AtomicBool,
}

/// A row this run made: an analysis appended to a pack, and the file it is
/// for.
struct Row {
    hash: Content,
    version: [u8; 16],
    settings: Settings,
    pack: i64,
    at: u64,
    len: u64,
    made_by: Option<&'static str>,
    today: i64,
    name: PathBuf,
    key: Key,
}

/// Whether SQLite says the index file is damaged or is no database: what no
/// later run will read either.
fn is_damage(e: &rusqlite::Error) -> bool {
    matches!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase))
}

impl Db {
    /// Keep a failure to be said, once, and whether it was damage.
    fn note(&self, e: &rusqlite::Error) {
        if is_damage(e) {
            self.damaged.store(true, Ordering::Relaxed);
        }
        lock(&self.failed).get_or_insert_with(|| e.to_string());
    }

    /// Commit what waits, waiting for a commit under way to end first. A
    /// failure is kept to be said, once.
    fn flush(&self) {
        let conn = lock(&self.conn);
        self.commit(&conn);
    }

    /// Commit what waits unless a commit is under way, which takes it: each
    /// worker's row is committed as soon as the index is free, and rows that
    /// come while one commit runs go together in the next. So a process
    /// killed outright (SIGKILL, the OOM killer) loses the rows of a commit or
    /// two, not a batch's worth of analyses, and no worker ever waits on a
    /// commit.
    fn commit_waiting(&self) {
        loop {
            let Ok(conn) = self.conn.try_lock() else { return };
            self.commit(&conn);
            drop(conn);
            // A row that came while the index was held, whose worker found it
            // busy and left it: it is this one's to commit.
            if lock(&self.pending).is_empty() {
                return;
            }
        }
    }

    fn commit(&self, conn: &Connection) {
        let rows = std::mem::take(&mut *lock(&self.pending));
        if rows.is_empty() {
            return;
        }
        let done = (|| -> rusqlite::Result<()> {
            let tx = conn.unchecked_transaction()?;
            {
                let mut a = tx.prepare_cached(
                    "INSERT OR REPLACE INTO analyses (hash, version, work, thumb, features, pack, at, len, made_by, colour, used) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10)",
                )?;
                let mut f = tx.prepare_cached("INSERT OR REPLACE INTO files (path, len, mtime, ctime, hash, sample, gone) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)")?;
                for r in &rows {
                    let s = r.settings;
                    a.execute(params![&r.hash.hash[..], &r.version[..], s.work_size, s.thumb, s.features, r.pack, r.at as i64, r.len as i64, r.made_by, r.today])?;
                    f.execute(params![r.name.as_os_str().as_bytes(), r.key.len as i64, r.key.mtime, r.key.ctime, &r.hash.hash[..], r.hash.sample as i64])?;
                }
            }
            tx.commit()
        })();
        if let Err(e) = done {
            self.note(&e);
        }
    }
}

/// Open the index at `path`, making its tables if it has none. `Err` for one
/// of another schema (its format) or one that will not open.
fn connect(path: &Path, share: bool) -> Result<Connection, Opened> {
    let fail = |e: rusqlite::Error| Opened::Failed(e.to_string());
    let c = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(fail)?;
    c.busy_timeout(std::time::Duration::from_secs(60)).map_err(fail)?;
    // A write-ahead log, whose commits sync nothing (only its checkpoints
    // do) and which a power cut cannot leave inconsistent: it costs the last
    // batches, whose records are analysed again. With a rollback journal each
    // batch's commit was an fsync, which on ext4 also flushed the packs' and
    // the scratch file's pages: 7.9 s of a cold found-corpus run, and 32 s of
    // workers waiting behind it. Not on a network share, where a log's shared
    // memory does not work: there a rollback journal, synced.
    c.pragma_update(None, "journal_mode", if share { "TRUNCATE" } else { "WAL" }).map_err(fail)?;
    // A pack is never synced; a row whose record a power cut lost is caught
    // by its stamp (`unrecord`) and costs that record.
    c.pragma_update(None, "synchronous", "NORMAL").map_err(fail)?;
    // The index is read through a mapping and a page cache that hold it: it
    // is some 380 bytes a file, and read a page at a time through SQLite's
    // default 2 MB cache, counting 33,000 rows took 127 ms. Not mapped on a
    // share, where another machine's writes do not reach a mapping.
    c.pragma_update(None, "mmap_size", if share { 0 } else { 1i64 << 30 }).map_err(fail)?;
    c.pragma_update(None, "cache_size", -65536i64).map_err(fail)?;
    let tx = c.unchecked_transaction().map_err(fail)?;
    let app: i64 = tx.pragma_query_value(None, "application_id", |r| r.get(0)).map_err(fail)?;
    let schema: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(fail)?;
    if app == 0 && schema == 0 {
        // New, or made empty: the run that gets here first makes it.
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                path BLOB PRIMARY KEY, len INTEGER NOT NULL, mtime INTEGER NOT NULL, ctime INTEGER NOT NULL,
                hash BLOB NOT NULL, sample INTEGER NOT NULL, gone INTEGER) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS files_by_hash ON files (hash);
             CREATE TABLE IF NOT EXISTS analyses (
                hash BLOB NOT NULL, version BLOB NOT NULL, work INTEGER NOT NULL, thumb INTEGER NOT NULL,
                features INTEGER NOT NULL, pack INTEGER NOT NULL, at INTEGER NOT NULL, len INTEGER NOT NULL,
                made_by TEXT, colour REAL, used INTEGER NOT NULL,
                PRIMARY KEY (hash, version, work, thumb)) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS packs (id INTEGER PRIMARY KEY);",
        )
        .map_err(fail)?;
        tx.pragma_update(None, "application_id", APPLICATION_ID).map_err(fail)?;
        tx.pragma_update(None, "user_version", SCHEMA).map_err(fail)?;
    } else if app != APPLICATION_ID {
        return Err(Opened::Failed("not an img-fp cache".into()));
    } else if schema != SCHEMA {
        return Err(Opened::Other(format!("IMGFPD{schema:02}")));
    }
    tx.commit().map_err(fail)?;
    Ok(c)
}

/// Whether `path` lies on a filesystem another machine may share: by the
/// type `/proc/self/mountinfo` gives the deepest mount holding it. A FUSE
/// mount counts as one (sshfs, rclone), which costs a local FUSE mount only a
/// sync at each commit.
fn on_network_share(path: &Path) -> bool {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let Ok(dir) = std::fs::canonicalize(dir) else { return false };
    let Ok(info) = std::fs::read_to_string("/proc/self/mountinfo") else { return false };
    let mut best: Option<(usize, &str)> = None;
    for line in info.lines() {
        let mut halves = line.splitn(2, " - ");
        let (Some(pre), Some(post)) = (halves.next(), halves.next()) else { continue };
        let (Some(point), Some(fstype)) = (pre.split(' ').nth(4), post.split(' ').next()) else { continue };
        let point = unescape(point);
        if dir.starts_with(&point) && best.is_none_or(|(n, _)| point.as_os_str().len() >= n) {
            best = Some((point.as_os_str().len(), fstype));
        }
    }
    best.is_some_and(|(_, t)| {
        t.starts_with("nfs") || t.starts_with("fuse") || matches!(t, "cifs" | "smb3" | "smbfs" | "9p" | "ceph" | "afs" | "glusterfs" | "lustre" | "ncpfs" | "coda" | "davfs")
    })
}

#[cfg(test)]
thread_local! {
    /// A test's answer to `on_network_share`, for this thread's opens.
    static AS_SHARE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// A mount table's field, its `\NNN` octal escapes read.
fn unescape(s: &str) -> PathBuf {
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

/// Why an index did not open.
enum Opened {
    Other(String),
    Failed(String),
}

// ------------------------------------------------------------ the store

/// The run's cache: the index open, the packs read from and the one appended
/// to, and what it knows of each of the run's files by number.
///
/// **It answers for the run's files by number**: `hash_of` gives a file's
/// hash where its key says the index still knows it, `read` finds the
/// analyses of the files' hashes and unpacks them, `take` hands one over,
/// `append` keeps one made for a file, `keep_colour` keeps a JPEG's colour,
/// and `finish` decides what stays. Which records a run keeps is decided
/// there and nowhere else.
pub struct Store {
    path: PathBuf,
    db: Option<Arc<Db>>,
    /// `X.lock`, held shared while the store is open.
    held: Option<File>,
    settings: Settings,
    version: [u8; 16],
    today: i64,
    asked: Vec<Asked>,
    /// Analyses of the run's files' hashes, at its settings, the read found.
    usable: usize,
    other_version: Option<String>,
    /// The pack this run appends to, opened at its first append.
    writer: Mutex<Option<Writer>>,
    /// The files whose analysis the run took or made: what it keeps.
    live: Mutex<Vec<bool>>,
    /// The hash of each file whose row went in with the analysis made for
    /// it, which `finish` does not write again: the hash its decode took,
    /// where it was not read before (`same::content_hashes`).
    rowed: Mutex<Vec<Option<Content>>>,
}

/// One walked file, as the index knows it.
struct Asked {
    /// Its name in the index (`lib::cache_names`).
    name: PathBuf,
    /// What the walk read of it; `None`, nothing is kept for it.
    key: Option<Key>,
    /// Its row in the index, as it was: key and hash.
    row: Option<(Key, Content)>,
    /// Its content this run, once known (`read`).
    hash: Option<Content>,
    /// The analysis read for it, until taken.
    record: Option<Record>,
    /// The day its analysis's row says it was last used, when it was read.
    used: Option<i64>,
}

/// Where a wanted file's analysis is: file, pack, offset, length, the day it
/// was last used, and the colour kept on it.
type Place = (usize, i64, u64, u64, i64, Option<f64>);

/// The pack a run appends to: its number, the file, held `flock`ed alone, and
/// where the next record goes.
struct Writer {
    id: i64,
    file: File,
    end: u64,
}

/// What `Store::finish` kept and did, for the run to say.
pub struct Kept {
    /// The first write that failed, if one did.
    pub failure: Option<String>,
    /// Files this run did not walk whose rows were kept.
    pub carried: usize,
    /// Of those, the ones that have gone and are kept for the grace period.
    pub gone: usize,
    /// Rows of files and analyses dropped.
    pub dropped: usize,
    pub compacted: Compacted,
}

/// Whether `Store::finish` rewrote the packs.
pub enum Compacted {
    /// Not worth it: the bytes the packs hold, and how many nothing reads.
    Unneeded { bytes: u64, unused: u64 },
    /// Put off: another run has the cache open.
    Busy,
    /// Side packs folded into the newest, without a rewrite.
    Folded,
    Done,
    Failed(String),
}

/// Open the cache at `path` for this run's `files` (each by the name the
/// index knows it by and what the walk read of it): read which of them the
/// index still knows by their key, and keep it open for what the run adds.
///
/// Every route out of here returns a store rather than an error, because the
/// cache is an optimisation: a run whose cache is missing or ruined computes
/// exactly the same pairs, only slower. What it must not be is silent about
/// the last of those. A cache of another version is left as it is and the run
/// keeps nothing in it, since that version may be in use.
pub fn open(path: &Path, settings: Settings, version: [u8; 16], files: Vec<(PathBuf, Option<Key>)>, problems: &mut Problems) -> Store {
    let asked: Vec<Asked> = files.into_iter().map(|(name, key)| Asked { name, key, row: None, hash: None, record: None, used: None }).collect();
    let n = asked.len();
    let mut store = Store {
        path: path.to_path_buf(),
        db: None,
        held: None,
        settings,
        version,
        today: today(),
        asked,
        usable: 0,
        other_version: None,
        writer: Mutex::new(None),
        live: Mutex::new(vec![false; n]),
        rowed: Mutex::new(vec![None; n]),
    };
    if let Found::Other(v) = found_at(path) {
        store.other_version = Some(v);
        return store;
    }
    let held = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path(path));
    let held = match held {
        Ok(f) => f,
        Err(e) => {
            problems.note(Note::Cache, format_args!("could not open {}: {e}", lock_path(path).display()));
            return store;
        }
    };
    flock(&held, LOCK_SH);
    let share = on_network_share(path);
    #[cfg(test)]
    let share = AS_SHARE.with(|s| s.get()).unwrap_or(share);
    // The index open, and each walked file's row in it, by its name.
    type Rows = Vec<Option<(Key, Content)>>;
    let load = |asked: &[Asked]| -> Result<(Connection, Rows), Opened> {
        let conn = connect(path, share)?;
        let rows: rusqlite::Result<Rows> = (|| {
            let mut q = conn.prepare("SELECT len, mtime, ctime, hash, sample FROM files WHERE path = ?1")?;
            asked
                .iter()
                .map(|a| {
                    q.query_row([a.name.as_os_str().as_bytes()], |r| {
                        let hash: Vec<u8> = r.get(3)?;
                        Ok((Key { len: r.get::<_, i64>(0)? as u64, mtime: r.get(1)?, ctime: r.get(2)? }, hash, r.get::<_, i64>(4)? as u64))
                    })
                    .optional()
                    .map(|o| o.and_then(|(k, h, sample)| Some((k, Content { hash: Hash::try_from(&h[..]).ok()?, sample }))))
                })
                .collect()
        })();
        let rows = rows.map_err(|e| Opened::Failed(e.to_string()))?;
        Ok((conn, rows))
    };
    let (conn, rows) = match load(&store.asked) {
        Ok(got) => got,
        Err(Opened::Other(v)) => {
            store.other_version = Some(v);
            return store;
        }
        Err(Opened::Failed(e)) => {
            // Damaged, or not a database after all: it costs the analysis,
            // so it is said, and replaced by an empty cache when nothing else
            // has it open.
            problems.note(Note::Cache, format_args!("{} could not be read ({e}); starting a new cache", path.display()));
            if !flock(&held, LOCK_EX | LOCK_NB) {
                return store;
            }
            remove_all(path);
            flock(&held, LOCK_SH);
            match load(&store.asked) {
                Ok(got) => got,
                Err(_) => return store,
            }
        }
    };
    for (a, r) in store.asked.iter_mut().zip(rows) {
        a.row = r;
    }
    let db = Arc::new(Db { conn: Mutex::new(conn), pending: Mutex::new(Vec::new()), failed: Mutex::new(None), damaged: Default::default() });
    lock(&OPEN).push(Arc::downgrade(&db));
    store.db = Some(db);
    store.held = Some(held);
    store
}

impl Store {
    /// The length and sample of each file the index knows: a walked file of
    /// another length and sample cannot hold the content of any analysis kept
    /// (`same::content_hashes`).
    pub fn samples(&self) -> HashSet<(u64, u64)> {
        let Some(db) = &self.db else { return HashSet::new() };
        let conn = lock(&db.conn);
        let got: rusqlite::Result<HashSet<(u64, u64)>> = (|| {
            let mut q = conn.prepare("SELECT DISTINCT len, sample FROM files")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)))?;
            rows.collect()
        })();
        got.unwrap_or_default()
    }

    /// Walked file `i`'s hash, where the index knows it: its row's, when the
    /// walk read the key the row was made with.
    pub fn hash_of(&self, i: usize) -> Option<Content> {
        let a = &self.asked[i];
        a.row.filter(|(k, _)| Some(*k) == a.key).map(|(_, h)| h)
    }

    /// Learn each walked file's hash (`None` for a file not hashed), and read
    /// the analyses of those `wanted` says to: the newest at this run's
    /// settings made by this build's analysis (`version`), not by another
    /// version of a library outside it (`made_by`). `met(i)` is said for
    /// each file the index answers, before its record is read, for the
    /// progress bar; `read_bytes(n)` as each batch is read.
    pub fn read(&mut self, hashes: Vec<Option<Content>>, wanted: &(dyn Fn(usize) -> bool + Sync), met: &(dyn Fn(usize) + Sync), problems: &mut Problems) -> u64 {
        for (a, h) in self.asked.iter_mut().zip(hashes) {
            a.hash = h;
        }
        let Some(db) = &self.db else { return 0 };
        let s = self.settings;
        // Where each wanted file's analysis is, if it has one.
        let found: rusqlite::Result<Vec<Place>> = (|| {
            let conn = lock(&db.conn);
            let mut q = conn.prepare_cached("SELECT pack, at, len, made_by, used, colour FROM analyses WHERE hash = ?1 AND version = ?2 AND work = ?3 AND thumb = ?4")?;
            let mut out = Vec::new();
            for (i, a) in self.asked.iter().enumerate() {
                let Some(h) = a.hash.filter(|_| wanted(i)) else { continue };
                let row = q
                    .query_row(params![&h.hash[..], &self.version[..], s.work_size, s.thumb], |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64, r.get::<_, i64>(2)? as u64, r.get::<_, Option<String>>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<f64>>(5)?))
                    })
                    .optional()?;
                let Some((pack, at, len, made_by, used, colour)) = row else { continue };
                // Pixels another version of libheif made are not this one's.
                if made_by.is_some_and(|m| Some(m.as_str()) != crate::heif::version()) {
                    continue;
                }
                met(i);
                out.push((i, pack, at, len, used, colour));
            }
            Ok(out)
        })();
        let mut found = match found {
            Ok(f) => f,
            Err(e) => {
                problems.note(Note::Cache, format_args!("could not read {}: {e}", self.path.display()));
                return 0;
            }
        };
        self.usable = found.len();
        // In each pack's order, so a cold read runs forward through the file.
        found.sort_unstable_by_key(|&(_, pack, at, ..)| (pack, at));
        let mut packs: HashMap<i64, Option<File>> = HashMap::new();
        for &(_, pack, ..) in &found {
            packs.entry(pack).or_insert_with(|| File::open(pack_path(&self.path, pack)).ok());
        }
        let bytes: u64 = found.iter().map(|f| f.3).sum();
        let (settings, asked) = (self.settings, &self.asked);
        let got: Vec<(usize, Option<Record>, i64)> = found
            .par_chunks(READ_BATCH)
            .flat_map_iter(|chunk| {
                let mut b = Vec::new();
                chunk
                    .iter()
                    .map(|&(i, pack, at, len, used, colour)| {
                        let rec = packs[&pack].as_ref().and_then(|f| {
                            b.resize(len as usize, 0);
                            f.read_exact_at(&mut b, at).ok()?;
                            let mut r = unrecord(&b, &asked[i].hash?.hash, settings).ok()?;
                            // A colour kept on the row, which the record's
                            // own traits do not carry.
                            r.traits.colour_kept = colour.map(|c| c as f32);
                            Some(r)
                        });
                        (i, rec, used)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut damaged = 0;
        for (i, rec, used) in got {
            match rec {
                Some(r) => {
                    self.asked[i].record = Some(r);
                    self.asked[i].used = Some(used);
                }
                None => damaged += 1,
            }
        }
        if damaged > 0 {
            // Each costs itself: its file is analysed again, and the record
            // made then replaces the row.
            problems.note(Note::Cache, format_args!("{}: {damaged} record(s) would not read and are made again", self.path.display()));
            self.usable -= damaged;
        }
        bytes
    }

    /// The analysis of walked file `i`, taken out of the store: there only if
    /// the read found one for its hash, and only once.
    pub fn take(&mut self, i: usize) -> Option<Record> {
        let rec = self.asked[i].record.take()?;
        self.live.get_mut().unwrap_or_else(PoisonError::into_inner)[i] = true;
        Some(rec)
    }

    /// Put walked file `i`'s analysis into a pack as soon as it is made, under
    /// its hash, and its row with the next batch. Not if the file has changed
    /// since the walk read its key: then what was hashed may not be what was
    /// described. Called by the workers as they finish.
    pub fn append(&self, i: usize, it: &Item) {
        let Some(db) = &self.db else { return };
        let a = &self.asked[i];
        let (Some(key), Some(hash)) = (a.key, a.hash.or(it.hash)) else { return };
        if std::fs::metadata(&a.name).ok().and_then(|m| key_from(&m)) != Some(key) {
            return;
        }
        let rec = match record(&hash.hash, it.dims, &it.traits, &it.feats, &it.thumb) {
            Ok(r) => r,
            Err(e) => {
                lock(&db.failed).get_or_insert_with(|| e.to_string());
                return;
            }
        };
        let _w = lock(&WRITING);
        let mut writer = lock(&self.writer);
        if writer.is_none() {
            match self.pack_to_write(db) {
                Ok(w) => *writer = Some(w),
                Err(e) => {
                    match e.downcast_ref::<rusqlite::Error>() {
                        Some(e) => db.note(e),
                        None => {
                            lock(&db.failed).get_or_insert_with(|| e.to_string());
                        }
                    }
                    return;
                }
            }
        }
        let w = writer.as_mut().unwrap();
        if let Err(e) = w.file.write_all_at(&rec, w.end) {
            lock(&db.failed).get_or_insert_with(|| e.to_string());
            return;
        }
        let row = Row {
            hash,
            version: self.version,
            settings: self.settings,
            pack: w.id,
            at: w.end,
            len: rec.len() as u64,
            made_by: it.made_by,
            today: self.today,
            name: a.name.clone(),
            key,
        };
        w.end += rec.len() as u64;
        drop(writer);
        APPENDED.fetch_add(1, Ordering::Relaxed);
        lock(&self.live)[i] = true;
        lock(&self.rowed)[i] = Some(hash);
        lock(&db.pending).push(row);
        // Committed with the append lock let go: the other workers append
        // meanwhile, and an interrupt commits whatever this has not.
        drop(_w);
        db.commit_waiting();
    }

    /// The pack to append to: the newest, when no other run is writing to
    /// it, else a new one of this run's own.
    fn pack_to_write(&self, db: &Db) -> Result<Writer> {
        let conn = lock(&db.conn);
        let newest: Option<i64> = conn.query_row("SELECT MAX(id) FROM packs", [], |r| r.get(0))?;
        let file = newest.and_then(|id| OpenOptions::new().read(true).write(true).open(pack_path(&self.path, id)).ok().map(|f| (id, f)));
        if let Some((id, file)) = file.filter(|(_, f)| flock(f, LOCK_EX | LOCK_NB)) {
            let end = file.metadata()?.len();
            return Ok(Writer { id, file, end });
        }
        conn.execute("INSERT INTO packs (id) VALUES (NULL)", [])?;
        let id = conn.last_insert_rowid();
        let file = OpenOptions::new().read(true).write(true).create_new(true).open(pack_path(&self.path, id))?;
        flock(&file, LOCK_EX);
        Ok(Writer { id, file, end: 0 })
    }

    /// Keep `colour` on walked file `i`'s analysis, if the run keeps one for
    /// it. Every file of its content shares it.
    pub fn keep_colour(&self, i: usize, colour: f32) {
        let Some(db) = &self.db else { return };
        let a = &self.asked[i];
        let Some(h) = a.hash.or(lock(&self.rowed)[i]).filter(|_| lock(&self.live)[i]) else { return };
        db.flush();
        let s = self.settings;
        let conn = lock(&db.conn);
        if let Err(e) = conn.execute(
            "UPDATE analyses SET colour = ?1 WHERE hash = ?2 AND version = ?3 AND work = ?4 AND thumb = ?5",
            params![colour as f64, &h.hash[..], &self.version[..], s.work_size, s.thumb],
        ) {
            db.note(&e);
        }
    }

    /// Walked file `i`'s name in the index: its canonical path
    /// (`lib::cache_names`), however the run spelled it.
    pub fn name(&self, i: usize) -> &Path {
        &self.asked[i].name
    }

    /// Analyses of the run's files the read found at its settings.
    pub fn usable(&self) -> usize {
        self.usable
    }

    /// The format the cache was written in, when it is another version's and
    /// this run therefore reads nothing from it and keeps nothing in it.
    pub fn other_version(&self) -> Option<&str> {
        self.other_version.as_deref()
    }

    /// Whether `finish` will have rows to write, as far as can be told before
    /// the analysis: a walked file whose row is not what the run read of it.
    /// What the progress bar expects, and nothing else.
    pub fn stale(&self) -> bool {
        self.db.is_some() && self.asked.iter().any(|a| a.hash.is_some() && a.key.is_some() && a.row != a.key.zip(a.hash))
    }

    /// Settle the cache at the end of the analysis.
    ///
    /// The index keeps a row for every walked file the run hashed (its key
    /// and hash), and for every file it did not walk while the file is there,
    /// or for `GRACE_DAYS` after a run first found it gone; an analysis while
    /// a kept row holds its hash, and one by another analysis than this
    /// build's for `GRACE_DAYS` unused. `--prune-cache` (`prune`) keeps only
    /// the run's own rows and analyses.
    ///
    /// The packs are rewritten, by a run that has the cache alone, when what
    /// no row reads is a quarter of them: the trade every log-structured
    /// store makes, the file never more than a third larger than what it
    /// holds and a rewrite copying at most three bytes worth keeping for each
    /// it drops. A pack another run wrote beside the others is folded into
    /// the largest, a copy of the smaller packs' records alone.
    pub fn finish(&mut self, prune: bool) -> Kept {
        let mut kept = Kept { failure: None, carried: 0, gone: 0, dropped: 0, compacted: Compacted::Unneeded { bytes: 0, unused: 0 } };
        let Some(db) = self.db.clone() else { return kept };
        db.flush();
        // This run's own pack is let go of, so a compaction can have it.
        drop(lock(&self.writer).take());
        let done = self.settle(&db, prune);
        if let Err(e) = &done {
            db.note(e);
        }
        if let Ok((carried, gone, dropped)) = done {
            (kept.carried, kept.gone, kept.dropped) = (carried, gone, dropped);
        }
        kept.compacted = match self.compact(&db, prune) {
            Ok(c) => c,
            Err(e) => {
                if e.downcast_ref::<rusqlite::Error>().is_some_and(is_damage) {
                    db.damaged.store(true, Ordering::Relaxed);
                }
                Compacted::Failed(e.to_string())
            }
        };
        kept.failure = lock(&db.failed).clone();
        if db.damaged.load(Ordering::Relaxed) {
            // Every later run would fail on it as this one did: removed, when
            // no other run has it, and the next starts a new cache.
            let alone = self.held.as_ref().is_some_and(|h| flock(h, LOCK_EX | LOCK_NB));
            let e = kept.failure.take().unwrap_or_default();
            kept.failure = Some(if alone {
                self.db = None;
                drop(db);
                remove_all(&self.path);
                format!("its index is damaged ({e}), so it is removed and the next run starts a new cache")
            } else {
                format!("its index is damaged ({e}); the next run that has it alone starts a new cache")
            });
        }
        kept
    }

    /// The index's rows as `finish` keeps them: (rows of other files carried,
    /// of them gone, rows dropped).
    fn settle(&self, db: &Db, prune: bool) -> rusqlite::Result<(usize, usize, usize)> {
        let today = self.today;
        let live = lock(&self.live).clone();
        let rowed = lock(&self.rowed).clone();
        let conn = lock(&db.conn);
        let tx = conn.unchecked_transaction()?;
        let mut dropped = 0;
        // Each walked file's row as the run found it: its key and hash, there.
        {
            let mut put = tx.prepare("INSERT OR REPLACE INTO files (path, len, mtime, ctime, hash, sample, gone) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)")?;
            let mut del = tx.prepare("DELETE FROM files WHERE path = ?1")?;
            for (a, _) in self.asked.iter().zip(&rowed).filter(|(_, r)| r.is_none()) {
                match a.key.zip(a.hash) {
                    Some((k, h)) if a.row != Some((k, h)) => {
                        put.execute(params![a.name.as_os_str().as_bytes(), k.len as i64, k.mtime, k.ctime, &h.hash[..], h.sample as i64])?;
                    }
                    Some(_) => {}
                    // Not hashed this run (unreadable, or no picture): what
                    // the row said is not known to hold.
                    None if a.row.is_some() => dropped += del.execute([a.name.as_os_str().as_bytes()])?,
                    None => {}
                }
            }
            // An analysis the run used is used today; told only once a day.
            let mut used = tx.prepare("UPDATE analyses SET used = ?1 WHERE hash = ?2 AND version = ?3 AND work = ?4 AND thumb = ?5")?;
            let s = self.settings;
            for (a, _) in self.asked.iter().zip(&live).filter(|(a, l)| **l && a.hash.is_some() && a.used.is_some_and(|u| u < today)) {
                used.execute(params![today, &a.hash.unwrap().hash[..], &self.version[..], s.work_size, s.thumb])?;
            }
        }
        // The rows of files this run did not walk, if there are any: when
        // the index holds no more rows than the walked files that have one,
        // there are none, and the rows are not read (a rescan of 33,000
        // files spent most of its finish reading them).
        let rows: i64 = tx.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
        let walked_rows = self.asked.iter().zip(&rowed).filter(|(a, r)| a.key.is_some() && a.hash.or(**r).is_some()).count();
        let walked: HashSet<&[u8]> = if rows as usize > walked_rows { self.asked.iter().map(|a| a.name.as_os_str().as_bytes()).collect() } else { HashSet::new() };
        let others: Vec<(PathBuf, Option<i64>)> = if walked.is_empty() { Vec::new() } else {
            let mut q = tx.prepare("SELECT path, gone FROM files")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Option<i64>>(1)?)))?;
            rows.filter_map(|r| r.ok()).filter(|(p, _)| !walked.contains(&p[..])).map(|(p, g)| (PathBuf::from(std::ffi::OsString::from_vec(p)), g)).collect()
        };
        let (mut carried, mut gone_n) = (0, 0);
        if prune {
            let mut del = tx.prepare("DELETE FROM files WHERE path = ?1")?;
            for (p, _) in &others {
                dropped += del.execute([p.as_os_str().as_bytes()])?;
            }
            // Only this run's analyses: its hashes, at its version and settings.
            tx.execute("CREATE TEMP TABLE IF NOT EXISTS keep (hash BLOB PRIMARY KEY)", [])?;
            tx.execute("DELETE FROM keep", [])?;
            {
                let mut k = tx.prepare("INSERT OR IGNORE INTO keep (hash) VALUES (?1)")?;
                for (h, _) in self.asked.iter().zip(&rowed).map(|(a, r)| a.hash.or(*r)).zip(&live).filter(|(_, l)| **l) {
                    if let Some(h) = h {
                        k.execute([&h.hash[..]])?;
                    }
                }
            }
            let s = self.settings;
            dropped += tx.execute(
                "DELETE FROM analyses WHERE version != ?1 OR work != ?2 OR thumb != ?3 OR hash NOT IN (SELECT hash FROM keep)",
                params![&self.version[..], s.work_size, s.thumb],
            )?;
        } else {
            let gone = gone(others.iter().map(|(p, _)| p.as_path()));
            let mut mark = tx.prepare("UPDATE files SET gone = ?2 WHERE path = ?1")?;
            let mut del = tx.prepare("DELETE FROM files WHERE path = ?1")?;
            for (p, was) in &others {
                let path = p.as_os_str().as_bytes();
                match (gone.contains(p.as_path()), *was) {
                    (true, Some(since)) if today - since >= GRACE_DAYS => {
                        dropped += del.execute([path])?;
                        continue;
                    }
                    (true, None) => {
                        mark.execute(params![path, today])?;
                    }
                    (false, Some(_)) => {
                        mark.execute(params![path, None::<i64>])?;
                    }
                    _ => {}
                }
                carried += 1;
                gone_n += gone.contains(p.as_path()) as usize;
            }
            dropped += tx.execute("DELETE FROM analyses WHERE version != ?1 AND used <= ?2", params![&self.version[..], today - GRACE_DAYS])?;
        }
        // An analysis no row holds the hash of is no file's. Only a row this
        // run dropped or gave another hash can have left one: a run that did
        // neither does not look (it was most of a cold run's finish).
        let rehashed = self.asked.iter().zip(&rowed).any(|(a, r)| a.row.is_some_and(|(_, h)| Some(h) != a.hash.or(*r)));
        if prune || dropped > 0 || rehashed {
            dropped += tx.execute("DELETE FROM analyses WHERE NOT EXISTS (SELECT 1 FROM files WHERE files.hash = analyses.hash)", [])?;
        }
        tx.commit()?;
        Ok((carried, gone_n, dropped))
    }

    /// Fold side packs into the largest and rewrite the packs when worth it,
    /// if the run has the cache alone.
    fn compact(&self, db: &Db, prune: bool) -> Result<Compacted> {
        let conn = lock(&db.conn);
        let live: HashMap<i64, u64> = {
            let mut q = conn.prepare("SELECT pack, SUM(len) FROM analyses GROUP BY pack")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let listed: Vec<i64> = {
            let mut q = conn.prepare("SELECT id FROM packs ORDER BY id")?;
            let rows = q.query_map([], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let on_disk = packs_on_disk(&self.path);
        let size = |id: i64| std::fs::metadata(pack_path(&self.path, id)).map_or(0, |m| m.len());
        let bytes: u64 = listed.iter().map(|&id| size(id)).sum();
        let used: u64 = live.values().sum();
        let unused = bytes.saturating_sub(used);
        let litter = on_disk.iter().any(|(id, _)| !listed.contains(id));
        let rewrite = (unused > 0 && prune) || (bytes > 0 && unused * 4 >= bytes);
        let fold = listed.len() > 1;
        if !rewrite && !fold && !litter {
            return Ok(Compacted::Unneeded { bytes, unused });
        }
        let Some(held) = &self.held else { return Ok(Compacted::Busy) };
        if !flock(held, LOCK_EX | LOCK_NB) {
            // Converting a lock is not atomic: the shared lock is taken again.
            flock(held, LOCK_SH);
            return Ok(Compacted::Busy);
        }
        // A pack no row of the index names is a run's that was killed before
        // it committed one, or a rewrite's that was: nobody's.
        for (id, p) in &on_disk {
            if !listed.contains(id) {
                let _ = std::fs::remove_file(p);
            }
        }
        let outcome = if rewrite {
            self.rewrite(&conn, &listed, None).map(|()| Compacted::Done)
        } else if fold {
            // Into the largest: the others are what a run beside another
            // wrote, a few records, and the largest is the cache.
            let into = *listed.iter().max_by_key(|&&id| (size(id), id)).unwrap();
            let from: Vec<i64> = listed.iter().copied().filter(|&id| id != into).collect();
            self.rewrite(&conn, &from, Some(into)).map(|()| Compacted::Folded)
        } else {
            Ok(Compacted::Unneeded { bytes, unused })
        };
        flock(held, LOCK_SH);
        outcome
    }

    /// Copy every record the index names in packs `from` into one pack (`into`,
    /// appended to, or a new one), point their rows at it in one commit, and
    /// remove the packs copied from. Records are copied as bytes, not
    /// unpacked. An interrupt before the commit leaves the old packs named and
    /// whole, and the new one nobody's (removed next time); after it, the old
    /// ones nobody's.
    fn rewrite(&self, conn: &Connection, from: &[i64], into: Option<i64>) -> Result<()> {
        let rows: Vec<(Vec<u8>, Vec<u8>, i64, i64, i64, i64, i64)> = {
            let mut q = conn.prepare(&format!(
                "SELECT hash, version, work, thumb, pack, at, len FROM analyses WHERE pack IN ({}) ORDER BY pack, at",
                from.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
            ))?;
            let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let (id, out, mut end) = match into {
            Some(id) => {
                let f = OpenOptions::new().read(true).write(true).open(pack_path(&self.path, id))?;
                let end = f.metadata()?.len();
                (id, f, end)
            }
            None => {
                conn.execute("INSERT INTO packs (id) VALUES (NULL)", [])?;
                let id = conn.last_insert_rowid();
                let f = OpenOptions::new().read(true).write(true).create_new(true).open(pack_path(&self.path, id))?;
                (id, f, 0)
            }
        };
        let fresh = into.is_none();
        let made = pack_path(&self.path, id);
        if fresh {
            begin_partial(&made);
        }
        let copied = (|| -> Result<Vec<u64>> {
            let mut packs: HashMap<i64, File> = HashMap::new();
            let mut at = Vec::with_capacity(rows.len());
            let mut buf = Vec::new();
            for r in &rows {
                let src = match packs.entry(r.4) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => e.insert(File::open(pack_path(&self.path, r.4))?),
                };
                buf.resize(r.6 as usize, 0);
                src.read_exact_at(&mut buf, r.5 as u64)?;
                out.write_all_at(&buf, end)?;
                at.push(end);
                end += r.6 as u64;
            }
            Ok(at)
        })();
        let at = match copied {
            Ok(at) => at,
            Err(e) => {
                if fresh {
                    abandon_partial(&made);
                }
                return Err(e);
            }
        };
        let tx = conn.unchecked_transaction()?;
        {
            let mut q = tx.prepare("UPDATE analyses SET pack = ?1, at = ?2 WHERE hash = ?3 AND version = ?4 AND work = ?5 AND thumb = ?6")?;
            for (r, &at) in rows.iter().zip(&at) {
                q.execute(params![id, at as i64, &r.0, &r.1, r.2, r.3])?;
            }
            let mut d = tx.prepare("DELETE FROM packs WHERE id = ?1")?;
            for &p in from {
                d.execute([p])?;
            }
        }
        if fresh {
            // Named by the index once this commits: not the handler's to
            // remove after.
            let mut partial = lock(&PARTIAL);
            if let Some(k) = partial.iter().position(|p| *p == made) {
                partial.swap_remove(k);
            } else {
                bail!("interrupted");
            }
            tx.commit()?;
        } else {
            tx.commit()?;
        }
        for &p in from {
            let _ = std::fs::remove_file(pack_path(&self.path, p));
        }
        Ok(())
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        if let Some(db) = &self.db {
            db.flush();
        }
    }
}

/// Which of `paths`, files rows name, are no longer there: the filesystem
/// says so, and not merely that it cannot be asked right now.
///
/// `Path::exists` is false on any error, so a file in a folder made
/// unreadable, on an sshfs mount whose connection dropped or behind a stale
/// NFS handle would count as deleted. Only a folder's listing without the
/// name, or a folder that is itself no longer there (or no longer a folder),
/// says a file has gone.
///
/// **A folder at a time.** Every run asks this of every row of every folder
/// the cache has seen but the run did not scan, and a `stat` a file is a
/// round trip apiece on a network share. A listing of each folder is one or
/// a few, whatever it holds.
fn gone<'a>(paths: impl Iterator<Item = &'a Path>) -> HashSet<&'a Path> {
    let mut by_folder: HashMap<&Path, Vec<&Path>> = HashMap::new();
    for p in paths {
        by_folder.entry(p.parent().unwrap_or(Path::new("/"))).or_default().push(p);
    }
    by_folder
        .into_par_iter()
        .flat_map_iter(|(folder, files)| {
            let names: Option<HashSet<std::ffi::OsString>> = match std::fs::read_dir(folder) {
                Ok(list) => list.map(|e| e.map(|e| e.file_name())).collect::<std::io::Result<_>>().ok(),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => Some(Default::default()),
                Err(_) => None,
            };
            files.into_iter().filter(move |f| names.as_ref().is_some_and(|n| f.file_name().is_none_or(|f| !n.contains(f))))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::Read;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("img-fp-cache-test-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const SETTINGS: Settings = Settings { work_size: 384, features: 600, thumb: 128 };
    const V: [u8; 16] = [1; 16];

    fn analysis(n: usize, seed: u8) -> Item {
        let kps: Vec<Keypoint> = (0..n).map(|i| Keypoint { x: i as f32, y: seed as f32, sigma: 1.6, angle: 0.0 }).collect();
        let desc: Vec<u8> = (0..n * DESC_LEN).map(|i| (i as u8).wrapping_add(seed)).collect();
        Item {
            feats: Arc::new(Features { w: 64, h: 48, kps: kps.into(), desc: desc.into(), ends: Vec::new() }),
            thumb: Arc::new(Thumb::new(8, 6, 0.125, vec![seed; 48])),
            dims: (320, 240),
            read: Read::Whole,
            traits: Traits { colour: -1.0, ..Default::default() },
            made_by: None,
            hash: None,
        }
    }

    /// What a run does with the store, as the pipeline does it: open it for
    /// `files`, hash each file the index does not know by its key, read the
    /// analyses, and for each file `make` says to, take its analysis or make
    /// one. Each file's analysis as the run had it: `Some(true)` read,
    /// `Some(false)` made, `None` neither.
    fn run(path: &Path, version: [u8; 16], files: &[PathBuf], make: impl Fn(usize) -> Option<Item>) -> (Store, Vec<Option<bool>>, Problems<'static>) {
        let log = Box::leak(Box::new(crate::problems::Log::default()));
        let mut problems = Problems::new(log);
        let named = files.iter().map(|f| (f.clone(), std::fs::metadata(f).ok().and_then(|m| key_from(&m)))).collect();
        let mut store = open(path, SETTINGS, version, named, &mut problems);
        let samples = store.samples();
        let stats: Vec<crate::walk::Stat> = files.iter().map(|f| crate::walk::Stat { len: std::fs::metadata(f).map_or(0, |m| m.len()), key: None }).collect();
        let hashes = crate::same::content_hashes(files, &stats, |_| true, |i| store.hash_of(i), |l| samples.iter().any(|s| s.0 == l), |l, s| samples.contains(&(l, s)), || ());
        let late: Vec<Option<Content>> = (0..files.len()).map(|i| hashes[i].or_else(|| crate::same::content_hash(&files[i]))).collect();
        store.read(hashes, &|_| true, &|_| (), &mut problems);
        let got = (0..files.len())
            .map(|i| match store.take(i) {
                Some(_) => Some(true),
                None => make(i).map(|it| {
                    // As a decode gives it.
                    store.append(i, &Item { hash: late[i], ..it });
                    false
                }),
            })
            .collect();
        (store, got, problems)
    }

    fn write(p: &Path, b: &[u8]) {
        std::fs::write(p, b).unwrap();
    }

    /// Shapes worth covering: a thumbnail one pixel wide (every Paeth
    /// predictor degenerates), one with no keypoints at all (the featureless
    /// images the corpus really contains), and an ordinary one.
    #[test]
    fn a_packed_record_comes_back_bit_for_bit() {
        for (n, w, h) in [(0usize, 1usize, 1usize), (1, 1, 7), (3, 7, 1), (137, 40, 31)] {
            let kps: Vec<Keypoint> = (0..n)
                .map(|i| {
                    let f = i as f32;
                    Keypoint { x: f * 1.37 + 0.5, y: 383.0 - f * 0.919, sigma: 1.6 * (1.0 + f / 64.0), angle: (f * 17.3) % 360.0 }
                })
                .collect();
            let desc: Vec<u8> = (0..n * DESC_LEN).map(|i| ((i * 37 + i / 128) % 256) as u8).collect();
            let px: Vec<u8> = (0..w * h).map(|i| ((i * 91) % 256) as u8).collect();
            let feats = Features { w: 640, h: 480, kps: kps.clone().into(), desc: desc.clone().into(), ends: Vec::new() };
            let thumb = Thumb::new(w as u16, h as u16, 0.25, px);
            let traits = Traits { detail: [1.0, 2.0, 3.0, 4.0, 5.0, 6.0], colour: 0.5, lossless: true, frames: true, profile: true, ..Default::default() };
            let hash = [9u8; 32];
            let b = record(&hash, (4000, 3000), &traits, &feats, &thumb).unwrap();
            let r = unrecord(&b, &hash, SETTINGS).unwrap();
            assert_eq!(r.feats.desc, feats.desc);
            assert_eq!(r.thumb.px, thumb.px);
            assert_eq!((r.thumb.w, r.thumb.h, r.thumb.scale.to_bits()), (thumb.w, thumb.h, thumb.scale.to_bits()));
            assert_eq!((r.dims, r.feats.w, r.feats.h), ((4000, 3000), 640, 480));
            assert_eq!(r.traits.detail, traits.detail);
            assert_eq!((r.traits.colour, r.traits.lossless, r.traits.exif, r.traits.frames, r.traits.deep, r.traits.profile), (0.5, true, false, true, false, true));
            assert_eq!(r.feats.kps.len(), feats.kps.len());
            for (got, want) in r.feats.kps.iter().zip(feats.kps.iter()) {
                assert_eq!([got.x, got.y, got.sigma, got.angle].map(f32::to_bits), [want.x, want.y, want.sigma, want.angle].map(f32::to_bits));
            }
            assert!(unrecord(&b, &[8u8; 32], SETTINGS).is_err(), "another file's hash");
        }
    }

    /// An analysis at a finer `--detail` keeps where its levels end, after the
    /// streams; levels that do not make sense are damage.
    #[test]
    fn a_records_levels_come_back() {
        let n = 9;
        let kps: Vec<Keypoint> = (0..n).map(|i| Keypoint { x: i as f32, y: 2.0, sigma: 1.6, angle: 10.0 }).collect();
        let desc: Vec<u8> = (0..n * DESC_LEN).map(|i| (i % 251) as u8).collect();
        let thumb = Thumb::new(4, 3, 0.25, vec![9; 12]);
        let plain = Features { w: 64, h: 48, kps: kps.clone().into(), desc: desc.clone().into(), ends: Vec::new() };
        let finer = Features { ends: vec![5, 7, 9], ..plain.clone() };
        let (a, b) = (pack(&plain, &thumb).unwrap(), pack(&finer, &thumb).unwrap());
        assert_eq!(&b[..a.len()], &a[..], "the levels follow the streams");
        assert_eq!(unpack(&a, n, 4, 3).unwrap().3, Vec::<u32>::new());
        assert_eq!(unpack(&b, n, 4, 3).unwrap().3, vec![5, 7, 9]);
        let bad = Features { ends: vec![5, 3], ..plain.clone() };
        assert!(unpack(&pack(&bad, &thumb).unwrap(), n, 4, 3).is_err(), "levels that end before the one below");
        let over = Features { ends: vec![5, 10], ..plain };
        assert!(unpack(&pack(&over, &thumb).unwrap(), n, 4, 3).is_err(), "a level past the keypoints");
    }

    /// A truncated record is damage, not a panic and not a silently short
    /// one; so is one claiming a shape its settings do not make.
    #[test]
    fn a_short_or_misshapen_record_is_rejected_rather_than_trusted() {
        let it = analysis(3, 1);
        let hash = [3u8; 32];
        let b = record(&hash, it.dims, &it.traits, &it.feats, &it.thumb).unwrap();
        for cut in [1, 10, b.len() - STAMP - 1] {
            assert!(unrecord(&b[..b.len() - cut], &hash, SETTINGS).is_err(), "cut {cut}");
        }
        let few = Settings { features: 2, ..SETTINGS };
        assert!(unrecord(&b, &hash, few).is_err(), "more keypoints than its settings allow");
    }

    /// `--cache` names the file, except when what it names is obviously a
    /// place to put one.
    #[test]
    fn a_named_path_is_a_file_unless_it_is_a_directory() {
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let dir = scratch("named");
        let named_file = dir.join("mine.db");
        assert_eq!(resolve_path(Some(&named_file), &mut problems).unwrap(), named_file);
        assert_eq!(resolve_path(Some(&dir), &mut problems).unwrap(), dir.join(file_name()));
        let trailing = PathBuf::from(format!("{}/", dir.join("sub").display()));
        assert_eq!(resolve_path(Some(&trailing), &mut problems).unwrap(), dir.join("sub").join(file_name()));
        let deep = dir.join("one/two/three.db");
        assert_eq!(resolve_path(Some(&deep), &mut problems).unwrap(), deep);
        assert!(dir.join("one/two").is_dir());
        assert!(!problems.any());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file that is no cache is neither used nor deleted; a cache of
    /// another version is left as it is and the run keeps nothing in it.
    #[test]
    fn a_stranger_is_not_used_and_another_versions_cache_is_left_alone() {
        let dir = scratch("stranger");
        let photo = dir.join("photo.jpg");
        write(&photo, b"\xff\xd8\xff\xe0 a picture");
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        assert!(resolve_path(Some(&photo), &mut problems).is_none());
        assert!(problems.any());
        assert!(!is_stranger(&dir.join("nothing-yet.db")));
        let empty = dir.join("empty.db");
        write(&empty, b"");
        assert!(!is_stranger(&empty));

        let old = dir.join("old.bin");
        write(&old, b"IMGFPC21 the records of 0.81.0");
        let pic = dir.join("a.jpg");
        write(&pic, b"a");
        let (store, got, _) = run(&old, V, std::slice::from_ref(&pic), |_| Some(analysis(2, 1)));
        assert_eq!(store.other_version(), Some("IMGFPC21"));
        assert_eq!(got, [Some(false)]);
        drop(store);
        assert_eq!(std::fs::read(&old).unwrap(), b"IMGFPC21 the records of 0.81.0");
        assert!(packs_on_disk(&old).is_empty());

        // An index of another schema, likewise.
        let other = dir.join("other.db");
        {
            let c = Connection::open(&other).unwrap();
            c.pragma_update(None, "application_id", APPLICATION_ID).unwrap();
            c.pragma_update(None, "user_version", SCHEMA + 1).unwrap();
            c.execute_batch("CREATE TABLE t (x)").unwrap();
        }
        let (store, _, _) = run(&other, V, std::slice::from_ref(&pic), |_| Some(analysis(2, 1)));
        assert_eq!(store.other_version(), Some(format!("IMGFPD{:02}", SCHEMA + 1).as_str()));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A run keeps what it made; the next reads it back, once a file, and the
    /// index knows each file by its key so reads none of them.
    #[test]
    fn an_analysis_made_is_read_back_by_the_next_run() {
        let dir = scratch("back");
        let path = dir.join(file_name());
        let files: Vec<PathBuf> = ["a.jpg", "b.jpg"].iter().map(|n| dir.join(n)).collect();
        write(&files[0], b"first picture");
        write(&files[1], b"second picture");
        let (mut store, got, problems) = run(&path, V, &files, |i| Some(analysis(5 + i, i as u8)));
        assert_eq!(got, [Some(false), Some(false)]);
        let kept = store.finish(false);
        assert!(kept.failure.is_none() && !problems.any());
        drop(store);

        let (mut store, got, _) = run(&path, V, &files, |_| None);
        assert_eq!(got, [Some(true), Some(true)]);
        assert!(store.hash_of(0).is_some() && store.hash_of(1).is_some(), "known by their keys");
        assert!(!store.stale());
        assert!(store.take(0).is_none(), "taken once");
        drop(store);
        // The record is the analysis made.
        let (mut s, _, _) = {
            let log = Box::leak(Box::new(crate::problems::Log::default()));
            let mut problems = Problems::new(log);
            let named = files.iter().map(|f| (f.clone(), std::fs::metadata(f).ok().and_then(|m| key_from(&m)))).collect();
            let mut s = open(&path, SETTINGS, V, named, &mut problems);
            let h = (0..2).map(|i| s.hash_of(i)).collect();
            s.read(h, &|_| true, &|_| (), &mut problems);
            (s, (), problems)
        };
        assert_eq!(s.take(1).unwrap().feats.desc, analysis(6, 1).feats.desc);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file moved or renamed, chmod'ed, or copied under another name keeps
    /// its analysis: found by what it holds.
    #[test]
    fn a_moved_changed_or_copied_file_keeps_its_analysis() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("moved");
        let path = dir.join(file_name());
        let a = dir.join("a.jpg");
        write(&a, b"one picture");
        let (mut store, got, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(4, 7)));
        assert_eq!(got, [Some(false)]);
        store.finish(false);
        drop(store);

        std::fs::create_dir_all(dir.join("moved")).unwrap();
        let b = dir.join("moved/b.jpg");
        std::fs::rename(&a, &b).unwrap();
        std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o600)).unwrap();
        let c = dir.join("copy.jpg");
        std::fs::copy(&b, &c).unwrap();
        let (mut store, got, _) = run(&path, V, &[b.clone(), c.clone()], |_| Some(analysis(4, 7)));
        assert_eq!(got, [Some(true), Some(true)], "the copy is its content's too");
        assert!(store.hash_of(0).is_none(), "its key was another path's");
        store.finish(false);
        drop(store);
        let (store, got, _) = run(&path, V, &[c.clone()], |_| None);
        assert_eq!(got, [Some(true)]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two files of one size touched together share their key to the
    /// nanosecond; each is still only its own content (0.69.0 gave one the
    /// other's analysis).
    #[test]
    fn files_of_one_key_are_not_each_other() {
        let dir = scratch("onekey");
        let path = dir.join(file_name());
        let (a, b) = (dir.join("sub1/p.bmp"), dir.join("sub2/p.bmp"));
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        write(&a, b"picture one!");
        write(&b, b"picture two!");
        let (mut store, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(3, 1)));
        store.finish(false);
        drop(store);
        let (store, got, _) = run(&path, V, std::slice::from_ref(&b), |_| None);
        assert_eq!(got, [None]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file rewritten with its size and mtime put back is read again (its
    /// change time moved), and the old analysis is not its.
    #[test]
    fn a_rewritten_file_is_not_its_old_content() {
        let dir = scratch("rewritten");
        let path = dir.join(file_name());
        let a = dir.join("a.jpg");
        write(&a, b"before");
        let (mut store, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(3, 1)));
        store.finish(false);
        drop(store);
        let mtime = std::fs::metadata(&a).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&a, b"after!");
        File::options().write(true).open(&a).unwrap().set_modified(mtime).unwrap();
        let (store, got, _) = run(&path, V, std::slice::from_ref(&a), |_| None);
        assert!(store.hash_of(0).is_none());
        assert_eq!(got, [None]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An analysis is not kept when the file changed between the walk and its
    /// description: what was hashed may not be what was described.
    #[test]
    fn a_file_changed_under_the_analysis_is_not_kept() {
        let dir = scratch("under");
        let path = dir.join(file_name());
        let a = dir.join("a.jpg");
        write(&a, b"before");
        let (mut store, got, _) = run(&path, V, std::slice::from_ref(&a), |_| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            write(&a, b"during");
            Some(analysis(3, 1))
        });
        assert_eq!(got, [Some(false)]);
        store.finish(false);
        drop(store);
        let (store, got, _) = run(&path, V, std::slice::from_ref(&a), |_| None);
        assert_eq!(got, [None]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Another analysis (`version`) reads none of this one's, and each is kept
    /// unused for the grace period; another library's pixels are not read.
    #[test]
    fn another_analysis_reads_nothing_and_is_kept_for_a_while() {
        let dir = scratch("version");
        let path = dir.join(file_name());
        let a = dir.join("a.heic");
        write(&a, b"a picture");
        let other = [2u8; 16];
        let (mut store, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(3, 1)));
        store.finish(false);
        drop(store);
        let (mut store, got, _) = run(&path, other, std::slice::from_ref(&a), |_| Some(analysis(3, 2)));
        assert_eq!(got, [Some(false)]);
        store.finish(false);
        drop(store);
        let (mut store, got, _) = run(&path, V, std::slice::from_ref(&a), |_| None);
        assert_eq!(got, [Some(true)], "the first's still there");
        // Long after: the other version's goes, this one's (used) stays.
        store.today += GRACE_DAYS + 1;
        store.finish(false);
        drop(store);
        let (store, got, _) = run(&path, other, std::slice::from_ref(&a), |_| None);
        assert_eq!(got, [None]);
        drop(store);

        // A libheif other than the one loaded made it: not read.
        let b = dir.join("b.heic");
        write(&b, b"b picture");
        let (mut store, _, _) = run(&path, V, std::slice::from_ref(&b), |_| Some(Item { made_by: Some("libheif 0.0.0-test"), ..analysis(3, 1) }));
        store.finish(false);
        drop(store);
        let (store, got, _) = run(&path, V, std::slice::from_ref(&b), |_| None);
        assert_eq!(got, [None]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The cache serves every folder the machine scans: a file this run did
    /// not walk is kept while it is there, and for the grace period after a
    /// run first finds it gone; `--prune-cache` keeps only this run's.
    #[test]
    fn what_a_run_did_not_walk_is_kept_while_there_and_a_while_after() {
        let dir = scratch("carry");
        let path = dir.join(file_name());
        let files: Vec<PathBuf> = ["here.jpg", "gone.jpg", "mine.jpg"].iter().map(|n| dir.join(n)).collect();
        for (i, f) in files.iter().enumerate() {
            write(f, format!("picture {i}").as_bytes());
        }
        let (mut store, _, _) = run(&path, V, &files, |i| Some(analysis(3, i as u8)));
        store.finish(false);
        drop(store);
        let moved = dir.join("later/gone.jpg");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&files[1], &moved).unwrap();
        let (mut store, _, _) = run(&path, V, &files[2..], |_| None);
        let kept = store.finish(false);
        assert_eq!((kept.carried, kept.gone), (2, 1));
        drop(store);
        // Within the grace, the moved file is found at its new place.
        let (mut store, got, _) = run(&path, V, std::slice::from_ref(&moved), |_| None);
        assert_eq!(got, [Some(true)]);
        drop(store.finish(false));
        drop(store);

        // A file gone past the grace is dropped, and its analysis with it.
        std::fs::remove_file(&moved).unwrap();
        let (mut store, _, _) = run(&path, V, &files[2..], |_| None);
        store.finish(false);
        drop(store);
        let (mut store, _, _) = run(&path, V, &files[2..], |_| None);
        store.today += GRACE_DAYS;
        let kept = store.finish(false);
        assert_eq!((kept.carried, kept.gone), (1, 0));
        drop(store);
        write(&moved, b"picture 1");
        let (store, got, _) = run(&path, V, std::slice::from_ref(&moved), |_| None);
        assert_eq!(got, [None]);
        drop(store);

        let (mut store, _, _) = run(&path, V, &files[2..], |_| None);
        let kept = store.finish(true);
        assert_eq!(kept.carried, 0);
        drop(store);
        let (store, got, _) = run(&path, V, &files[..1], |_| None);
        assert_eq!(got, [None], "pruned");
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A JPEG's colour measured later is kept on its analysis, for every file
    /// of its content, as `kept_colour` gave it.
    #[test]
    fn a_colour_measured_later_is_kept() {
        let dir = scratch("colour");
        let path = dir.join(file_name());
        let a = dir.join("a.jpg");
        write(&a, b"a picture");
        let (mut store, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(3, 1)));
        store.finish(false);
        store.keep_colour(0, kept_colour(1.7));
        drop(store);
        let c = dir.join("c.jpg");
        std::fs::copy(&a, &c).unwrap();
        let (mut store, _, _) = {
            let log = Box::leak(Box::new(crate::problems::Log::default()));
            let mut problems = Problems::new(log);
            let named = vec![(c.clone(), std::fs::metadata(&c).ok().and_then(|m| key_from(&m)))];
            let mut s = open(&path, SETTINGS, V, named, &mut problems);
            s.read(vec![crate::same::content_hash(&c)], &|_| true, &|_| (), &mut problems);
            (s, (), problems)
        };
        assert_eq!(store.take(0).unwrap().traits.colour_kept, Some(kept_colour(1.7)));
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Rows not yet committed are committed when the store goes, and when the
    /// process is interrupted (`seal`).
    #[test]
    fn rows_waiting_are_committed_at_an_interrupt() {
        let dir = scratch("seal");
        let path = dir.join(file_name());
        let a = dir.join("a.jpg");
        write(&a, b"a picture");
        let (store, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(3, 1)));
        let db = store.db.clone().unwrap();
        assert_eq!(lock(&db.pending).len(), 0, "committed as it was kept");
        // A worker whose commit finds the index busy leaves its row waiting.
        {
            let _busy = lock(&db.conn);
            std::thread::scope(|t| t.spawn(|| store.append(0, &Item { hash: Some(Content::of(b"a picture")), ..analysis(3, 1) })).join().unwrap());
            assert_eq!(lock(&db.pending).len(), 1);
        }
        {
            let (_held, _) = seal();
        }
        assert_eq!(lock(&db.pending).len(), 0);
        std::mem::forget(store);
        let (store, got, _) = run(&path, V, std::slice::from_ref(&a), |_| None);
        assert_eq!(got, [Some(true)]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_process_killed_outright_keeps_what_it_finished() {
        let dir = scratch("killed");
        let path = dir.join(file_name());
        let files: Vec<PathBuf> = (0..5).map(|k| dir.join(format!("{k}.jpg"))).collect();
        for (k, f) in files.iter().enumerate() {
            write(f, format!("picture {k}").as_bytes());
        }
        let (store, got, _) = run(&path, V, &files, |i| Some(analysis(4, i as u8)));
        assert_eq!(got, [Some(false); 5]);
        // No finish, no drop, no seal: as SIGKILL leaves it.
        std::mem::forget(store);
        let (store, got, _) = run(&path, V, &files, |_| None);
        assert_eq!(got, [Some(true); 5]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_damaged_index_is_replaced_rather_than_failed_on() {
        let dir = scratch("damaged-index");
        let path = dir.join(file_name());
        let files: Vec<PathBuf> = (0..40).map(|k| dir.join(format!("{k}.jpg"))).collect();
        for (k, f) in files.iter().enumerate() {
            write(f, format!("picture {k}").as_bytes());
        }
        let (mut store, _, _) = run(&path, V, &files, |i| Some(analysis(4, i as u8)));
        store.finish(false);
        drop(store);
        // Every page after the header's overwritten: the header still says
        // it is ours, and nothing in it reads.
        let len = std::fs::metadata(&path).unwrap().len();
        let f = OpenOptions::new().write(true).open(&path).unwrap();
        f.write_all_at(&vec![0x5A; len as usize - 100], 100).unwrap();
        drop(f);
        let (mut store, got, problems) = run(&path, V, &files, |i| Some(analysis(4, i as u8)));
        assert!(problems.any(), "said");
        assert!(got.iter().all(|g| *g == Some(false)), "analysed again");
        assert!(store.finish(false).failure.is_none(), "and the new cache works");
        drop(store);
        let (store, got, problems) = run(&path, V, &files, |_| None);
        assert!(!problems.any());
        assert!(got.iter().all(|g| *g == Some(true)));
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_cache_on_a_share_keeps_and_reads_as_a_local_one() {
        let dir = scratch("share");
        let path = dir.join(file_name());
        let (a, b) = (dir.join("a.jpg"), dir.join("b.jpg"));
        write(&a, b"a picture");
        write(&b, b"b picture");
        AS_SHARE.with(|s| s.set(Some(true)));
        let (mut first, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(40, 1)));
        let (mut second, _, _) = run(&path, V, std::slice::from_ref(&b), |_| Some(analysis(3, 2)));
        let mode: String = lock(&first.db.as_ref().unwrap().conn).pragma_query_value(None, "journal_mode", |r| r.get(0)).unwrap();
        assert_eq!(mode, "truncate");
        assert!(matches!(second.finish(false).compacted, Compacted::Busy));
        drop(second);
        assert!(matches!(first.finish(false).compacted, Compacted::Folded));
        drop(first);
        // A rollback journal, left empty between commits.
        assert!(!beside(&path, "-wal").exists());
        assert_eq!(std::fs::metadata(beside(&path, "-journal")).map_or(0, |m| m.len()), 0);
        let (store, got, _) = run(&path, V, &[a.clone(), b.clone()], |_| None);
        assert_eq!(got, [Some(true), Some(true)]);
        drop(store);
        AS_SHARE.with(|s| s.set(None));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_local_cache_leaves_no_log_once_closed() {
        let dir = scratch("closed");
        let path = dir.join(file_name());
        let a = dir.join("a.jpg");
        write(&a, b"a picture");
        let (mut store, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(3, 1)));
        assert!(beside(&path, "-wal").exists(), "a log while open");
        store.finish(false);
        drop(store);
        assert!(!beside(&path, "-wal").exists() && !beside(&path, "-shm").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn two_runs_writing_at_once_keep_both_and_are_folded() {
        let dir = scratch("two");
        let path = dir.join(file_name());
        let (a, b) = (dir.join("a.jpg"), dir.join("b.jpg"));
        write(&a, b"a picture");
        write(&b, b"b picture");
        let (mut first, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(40, 1)));
        let (mut second, _, _) = run(&path, V, std::slice::from_ref(&b), |_| Some(analysis(3, 2)));
        assert_eq!(packs_on_disk(&path).len(), 2);
        let first_pack = packs_on_disk(&path).into_iter().min_by_key(|(id, _)| *id).unwrap().0;
        assert!(matches!(second.finish(false).compacted, Compacted::Busy), "the first still has it");
        drop(second);
        assert!(matches!(first.finish(false).compacted, Compacted::Folded));
        drop(first);
        assert_eq!(packs_on_disk(&path).into_iter().map(|(id, _)| id).collect::<Vec<_>>(), [first_pack], "folded into the larger");
        let (store, got, _) = run(&path, V, &[a.clone(), b.clone()], |_| None);
        assert_eq!(got, [Some(true), Some(true)]);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The packs are rewritten once a quarter of them is read by nothing, and
    /// not before; what is kept reads back, and a pack nobody names goes.
    #[test]
    fn the_packs_are_rewritten_once_a_quarter_is_unused() {
        let dir = scratch("compact");
        let path = dir.join(file_name());
        let files: Vec<PathBuf> = (0..8).map(|i| dir.join(format!("{i}.jpg"))).collect();
        for (i, f) in files.iter().enumerate() {
            write(f, format!("picture {i}").as_bytes());
        }
        let (mut store, _, _) = run(&path, V, &files, |i| Some(analysis(20, i as u8)));
        store.finish(false);
        drop(store);
        write(&pack_path(&path, 99), b"a killed run's");
        // One of eight gone and dropped (pruned): an eighth unused.
        let (mut store, _, _) = run(&path, V, &files[1..], |_| None);
        assert!(matches!(store.finish(true).compacted, Compacted::Done), "pruned, and litter");
        drop(store);
        assert!(!pack_path(&path, 99).exists());
        let (mut store, _, _) = run(&path, V, &files[1..], |_| None);
        assert!(matches!(store.finish(false).compacted, Compacted::Unneeded { unused: 0, .. }));
        drop(store);
        // Two of seven rewritten: a quarter and more unused.
        for f in &files[1..3] {
            write(f, b"changed");
        }
        let (mut store, got, _) = run(&path, V, &files[1..], |i| Some(analysis(20, 100 + i as u8)));
        assert_eq!(got[..3], [Some(false), Some(false), Some(true)]);
        let before = bytes(&path);
        assert!(matches!(store.finish(false).compacted, Compacted::Done));
        drop(store);
        assert!(bytes(&path) < before);
        let (store, got, _) = run(&path, V, &files[1..], |_| None);
        assert!(got.iter().all(|g| *g == Some(true)));
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A record the index names that is not where it says (a pack damaged or
    /// replaced) costs itself, and is made again.
    #[test]
    fn a_damaged_record_costs_itself() {
        let dir = scratch("damaged");
        let path = dir.join(file_name());
        let files: Vec<PathBuf> = ["a.jpg", "b.jpg"].iter().map(|n| dir.join(n)).collect();
        write(&files[0], b"one");
        write(&files[1], b"two");
        let (mut store, _, _) = run(&path, V, &files, |i| Some(analysis(5, i as u8)));
        store.finish(false);
        drop(store);
        let (id, pack) = packs_on_disk(&path).pop().unwrap();
        let _ = id;
        let f = OpenOptions::new().write(true).open(&pack).unwrap();
        f.write_all_at(&[0xAA; 4], 0).unwrap();
        let (store, got, problems) = run(&path, V, &files, |_| Some(analysis(5, 9)));
        assert!(problems.any());
        assert_eq!(got.iter().filter(|g| **g == Some(true)).count(), 1);
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Names that are not UTF-8 are their own files.
    #[test]
    fn names_that_are_not_utf8_keep_their_own_rows() {
        let dir = scratch("utf8");
        let path = dir.join(file_name());
        let names: Vec<PathBuf> = [b"x\xfe.jpg", b"x\xff.jpg"].iter().map(|n| dir.join(std::ffi::OsStr::from_bytes(*n))).collect();
        write(&names[0], b"one");
        write(&names[1], b"two");
        let (mut store, _, _) = run(&path, V, &names, |i| Some(analysis(3, i as u8)));
        store.finish(false);
        drop(store);
        let (store, got, _) = run(&path, V, &names, |_| None);
        assert_eq!(got, [Some(true), Some(true)]);
        assert!(store.hash_of(0).is_some() && store.hash_of(0) != store.hash_of(1));
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// `--clear-cache` removes the cache and every file of it, and nothing
    /// else.
    #[test]
    fn clearing_removes_every_file_of_the_cache() {
        let dir = scratch("clear");
        let path = dir.join(file_name());
        let a = dir.join("a.jpg");
        write(&a, b"a picture");
        let (mut store, _, _) = run(&path, V, std::slice::from_ref(&a), |_| Some(analysis(3, 1)));
        store.finish(false);
        drop(store);
        write(&crate::matched::path_beside(&path), b"IMGFPM02");
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        clear(&path, &mut problems);
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("a.jpg")]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file is gone when its folder's listing lacks it or the folder is
    /// gone; one beside it that is there is not.
    #[test]
    fn gone_is_asked_a_folder_at_a_time() {
        let dir = scratch("gone");
        let (here, deleted) = (dir.join("a.jpg"), dir.join("b.jpg"));
        write(&here, b"x");
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

    /// A file in a folder that cannot be listed is not gone: it cannot be
    /// asked.
    #[test]
    fn a_file_that_cannot_be_reached_is_not_gone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("unreachable");
        let locked = dir.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        let f = locked.join("a.jpg");
        write(&f, b"x");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let reachable = std::fs::read_dir(&locked).is_ok();
        let found: Vec<PathBuf> = gone([f.as_path()].into_iter()).into_iter().map(Path::to_path_buf).collect();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        if !reachable {
            assert!(found.is_empty());
        }
    }

    #[test]
    fn the_default_file_is_named_for_its_schema() {
        assert_eq!(file_name(), "analysis-IMGFPD01.db");
        assert_eq!(FORMAT, format!("IMGFPD{SCHEMA:02}"));
    }
}
