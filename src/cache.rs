//! On-disk cache of the per-image analysis.
//!
//! Describing an image is by far the most expensive stage, and it depends only
//! on the file and the extraction settings. Caching it makes a rescan of a
//! mostly-unchanged directory nearly free, and makes tuning the matching
//! stages practical.
//!
//! The format is a header holding the settings, then one record per file. A
//! record is keyed by path, size and modification time, so an edited file is
//! re-described rather than trusted. Settings live in the header, so changing
//! the working size or the feature budget invalidates the whole file rather
//! than silently mixing two kinds of record.
//!
//! **What a record costs, because it is more than people expect.** An analysis
//! is 128 bytes of descriptor and 20 bytes of keypoint for each of up to 600
//! keypoints, plus a thumbnail of up to 128x128. That is ~148 bytes per
//! keypoint, and a *small* photograph is the worst case rather than the best:
//! `upsample_below` enlarges anything under the working size before describing
//! it, so a 224x224 file yields some 530 keypoints — 95 KB of analysis for a
//! 25 KB JPEG. Nothing about the encoding causes that; it is how much analysis
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

use crate::problems::Problems;
use crate::sift::{Features, Keypoint, DESC_LEN};
use crate::verify::Thumb;
use anyhow::{anyhow, bail, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The last two bytes are the format's version. A file carrying the prefix and
/// another version is a cache this build cannot read, which is a *stale* cache
/// and not a damaged one: it was written by img-fp, it will be rewritten by
/// img-fp, and there is nothing for anyone to do about it.
const MAGIC: &[u8; 8] = b"IMGFPC03";
const MAGIC_PREFIX: &[u8; 6] = b"IMGFPC";

/// Records packed or unpacked in one parallel batch. Large enough that the
/// threads are not synchronising over nothing, small enough that a batch of
/// held records is megabytes rather than the whole corpus — the cache of a
/// large corpus is most of a gigabyte and assembling it in memory would double
/// the run's peak for the length of one write.
const BATCH: usize = 64;

#[derive(Clone, Copy, PartialEq, Debug)]
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
}

#[derive(Clone, Copy)]
pub struct Key {
    pub len: u64,
    pub mtime: i64,
}

/// The default cache is one file in one directory, named the way `vid-fp`
/// names its own.
const FILE_NAME: &str = "analysis.bin";

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
    let path = match explicit {
        Some(given) => {
            let names_a_dir = given.is_dir() || given.to_string_lossy().ends_with('/');
            if names_a_dir { given.join(FILE_NAME) } else { given.to_path_buf() }
        }
        None => default_dir().join(FILE_NAME),
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        if let Err(e) = std::fs::create_dir_all(dir) {
            problems.cache(format!("could not create {}: {e}", dir.display()));
            return None;
        }
    }
    Some(path)
}

pub fn key_of(path: &Path) -> Option<Key> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as i64;
    Some(Key { len: md.len(), mtime })
}

// ------------------------------------------------------------ packing

/// The five `Keypoint` fields, in the order a record stores them.
fn field_of(kp: &Keypoint, i: usize) -> f32 {
    match i {
        0 => kp.x,
        1 => kp.y,
        2 => kp.sigma,
        3 => kp.angle,
        _ => kp.response,
    }
}
const FIELDS: usize = 5;

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
    let mut out = Vec::with_capacity(n * DESC_LEN);
    for stream in [&planes, &f.desc, &resid] {
        let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
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

    let mut kps = vec![Keypoint { x: 0.0, y: 0.0, sigma: 0.0, angle: 0.0, response: 0.0 }; n];
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
                3 => kp.angle = f,
                _ => kp.response = f,
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
    fn header(&mut self, s: Settings) -> Result<()> {
        self.0.write_all(MAGIC)?;
        self.u32(s.work_size)?;
        self.u32(s.features)?;
        self.u32(s.thumb)
    }
}

/// One whole record, as the file holds it.
fn record(path: &str, k: Key, f: &Features, t: &Thumb) -> Result<Vec<u8>> {
    let blob = pack(f, t)?;
    let mut b = Buf(Vec::with_capacity(path.len() + 64 + blob.len()));
    b.bytes(path.as_bytes())?;
    b.u64(k.len)?;
    b.i64(k.mtime)?;
    b.u32(f.w)?;
    b.u32(f.h)?;
    b.u32(f.kps.len() as u32)?;
    b.u32(t.w as u32)?;
    b.u32(t.h as u32)?;
    b.f32(t.scale)?;
    b.bytes(&blob)?;
    Ok(b.0)
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

/// The temporary file a compaction or a fresh cache is being written into,
/// while there is one: the handler removes it, since the file it would have
/// replaced is still there and still whole.
static PARTIAL: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Records appended by this run, for the handler to report.
static APPENDED: AtomicUsize = AtomicUsize::new(0);

fn lock<T>(m: &'static Mutex<T>) -> MutexGuard<'static, T> {
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
    if let Some(tmp) = lock(&PARTIAL).take() {
        std::fs::remove_file(tmp).ok();
    }
    (held, APPENDED.load(Ordering::SeqCst))
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
    *lock(&PARTIAL) = Some(tmp.clone());
    let made = (|| -> Result<File> {
        let f = OpenOptions::new().read(true).append(true).create(true).open(&tmp)?;
        fill(&f)?;
        let mut partial = lock(&PARTIAL);
        if partial.take().is_none() {
            bail!("interrupted");
        }
        std::fs::rename(&tmp, path)?;
        Ok(f)
    })();
    if made.is_err() {
        // A half-written temporary file is this run's litter, and the next
        // run will not know to clear it: the name carries a process id
        // precisely so that no other run touches it.
        *lock(&PARTIAL) = None;
        std::fs::remove_file(&tmp).ok();
    }
    made
}

// ------------------------------------------------------------ the store

/// Why a cache file produced nothing.
///
/// The two are worth keeping apart because only one of them is worth telling
/// anyone about. A cache written at another working size holds records that
/// describe a different analysis, so discarding it whole is the format doing
/// its job — see the note on `Settings` — and it happens every time a sweep
/// changes `--work-size`. A file that is not a cache at all, or that is
/// corrupt in the middle, is a file the user pointed at and will keep pointing
/// at, and it costs a full re-analysis every run until it is noticed.
enum Reject {
    /// Written by a run with different extraction settings. By design.
    Stale,
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
    /// `None` when the cache could not be opened for writing, and the run
    /// keeps nothing.
    file: Option<File>,
    records: AtomicUsize,
    /// Set by the first append that fails; the rest are not attempted.
    failed: Mutex<Option<String>>,
    broken: std::sync::atomic::AtomicBool,
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
/// A file that is stale or damaged is replaced by an empty one straight away,
/// rather than at the end of the run, since this run's records go into
/// whatever file it holds from here on.
pub fn open(path: &Path, want: Settings, problems: &mut Problems) -> (HashMap<String, (Key, Record, Span)>, Store) {
    let mut out = HashMap::new();
    let mut store = Store {
        path: path.to_path_buf(),
        file: None,
        records: AtomicUsize::new(0),
        failed: Mutex::new(None),
        broken: Default::default(),
    };
    match OpenOptions::new().read(true).append(true).open(path) {
        Ok(f) => {
            if let Some(tail) = read_file(&f, path, want, &mut out, problems) {
                // A partial record at the end would sit in front of every
                // record appended after it, and turn a lost record into a
                // damaged file.
                if tail.truncated {
                    if let Err(e) = f.set_len(tail.end) {
                        problems.cache(format!("could not repair {}: {e}", path.display()));
                        return (out, store);
                    }
                }
                store.file = Some(f);
                store.records = AtomicUsize::new(tail.count);
                return (out, store);
            }
        }
        // Not being there yet is the first run against this cache, which is
        // every first run and not a problem.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            // A cache that can be read and not written still answers this
            // run. It just keeps nothing new, and that is worth saying.
            problems.cache(format!("could not open {} for writing: {e}", path.display()));
            if let Ok(f) = File::open(path) {
                read_file(&f, path, want, &mut out, problems);
            }
            return (out, store);
        }
    }
    match replace_with(path, |f| Buf(f).header(want)) {
        Ok(f) => store.file = Some(f),
        Err(e) => problems.cache(format!("could not write {}: {e}", path.display())),
    }
    (out, store)
}

/// Read `f` into `out`; `None`, having said so if it is worth saying, when
/// nothing in it is usable.
fn read_file(
    f: &File,
    path: &Path,
    want: Settings,
    out: &mut HashMap<String, (Key, Record, Span)>,
    problems: &mut Problems,
) -> Option<Tail> {
    match read_stream(std::io::BufReader::with_capacity(1 << 20, f), want, out) {
        Ok(tail) => Some(tail),
        Err(Reject::Stale) => {
            out.clear();
            None
        }
        Err(Reject::Damaged(e)) => {
            problems.cache(format!("ignoring {}: {e}", path.display()));
            out.clear();
            None
        }
    }
}

impl Store {
    /// Put one analysis into the file, and say where it went. Called by the
    /// workers as they finish; the packing runs outside the lock, and only
    /// the write is inside it.
    pub fn append(&self, path: &str, key: Key, f: &Features, t: &Thumb) -> Option<Span> {
        let file = self.file.as_ref()?;
        if self.broken.load(Ordering::Relaxed) {
            return None;
        }
        let wrote = record(path, key, f, t).and_then(|rec| {
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

    /// Records the file holds, whether or not anything will keep them.
    pub fn records(&self) -> usize {
        self.records.load(Ordering::Relaxed)
    }

    /// The first append that failed, if one did.
    pub fn failure(&self) -> Option<String> {
        self.failed.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    pub fn writable(&self) -> bool {
        self.file.is_some() && !self.broken.load(Ordering::Relaxed)
    }

    /// Rewrite the file to hold `entries` and nothing else, sorted, so that
    /// the same corpus in the same state compacts to the same bytes.
    ///
    /// Every entry is already in the file, so this is a copy: nothing is
    /// unpacked or deflated again, and an interrupt part-way through leaves
    /// the file it was copying from exactly as it was.
    pub fn compact(&mut self, settings: Settings, entries: &mut [(&str, Span)]) -> Result<()> {
        let Some(src) = &self.file else { return Ok(()) };
        entries.sort_unstable_by_key(|e| e.0);
        let new = replace_with(&self.path, |f| {
            let mut b = Buf(std::io::BufWriter::with_capacity(1 << 20, f));
            b.header(settings)?;
            let mut buf = Vec::new();
            for &(_, s) in entries.iter() {
                buf.resize(s.len as usize, 0);
                src.read_exact_at(&mut buf, s.at)?;
                b.0.write_all(&buf)?;
            }
            b.0.flush()?;
            Ok(())
        })?;
        self.file = Some(new);
        self.records = AtomicUsize::new(entries.len());
        Ok(())
    }
}

/// One record's fixed-size half, which is what the reader needs before it can
/// make sense of the packed half.
struct Head {
    path: String,
    key: Key,
    w: u32,
    h: u32,
    n: usize,
    tw: u16,
    th: u16,
    scale: f32,
}

/// A reader that knows where it is, and whether it ran out of file.
struct Rd<R: Read> {
    r: R,
    pos: u64,
    ended: bool,
}
impl<R: Read> Rd<R> {
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

/// A record's path is written before its shape, so a file claiming an absurd
/// one must not be allowed to ask for that much memory first.
const MAX_PATH: usize = 1 << 16;

fn read_record<R: Read>(r: &mut Rd<R>) -> Result<Option<(Head, Vec<u8>)>> {
    let Some(len) = r.len_or_eof()? else { return Ok(None) };
    if len > MAX_PATH {
        bail!("record claims a {len}-byte path");
    }
    let head = Head {
        path: String::from_utf8_lossy(&r.take(len)?).into_owned(),
        key: Key { len: r.u64()?, mtime: r.i64()? },
        w: r.u32()?,
        h: r.u32()?,
        n: r.u32()? as usize,
        tw: r.u32()? as u16,
        th: r.u32()? as u16,
        scale: r.f32()?,
    };
    let packed = r.u64()? as usize;
    let blob = r.take(packed)?;
    Ok(Some((head, blob)))
}

fn read_stream<R: Read>(r: R, want: Settings, out: &mut HashMap<String, (Key, Record, Span)>) -> Result<Tail, Reject> {
    let mut r = Rd { r, pos: 0, ended: false };
    let magic: [u8; 8] = r.arr().map_err(|_| anyhow!("not a cache file"))?;
    if &magic[..MAGIC_PREFIX.len()] != MAGIC_PREFIX {
        return Err(anyhow!("not a cache file").into());
    }
    if &magic != MAGIC {
        // A cache this build cannot read, written by a build that could. See
        // the note on MAGIC: that is the format changing, not a damaged file.
        return Err(Reject::Stale);
    }
    let got = Settings { work_size: r.u32()?, features: r.u32()?, thumb: r.u32()? };
    if got != want {
        return Err(Reject::Stale);
    }
    let mut tail = Tail { end: r.pos, count: 0, truncated: false };
    let mut batch: Vec<(Head, Vec<u8>, Span)> = Vec::with_capacity(BATCH);
    loop {
        let at = r.pos;
        match read_record(&mut r) {
            Ok(None) => break,
            Ok(Some((head, blob))) => {
                batch.push((head, blob, Span { at, len: r.pos - at }));
                tail.end = r.pos;
                tail.count += 1;
                if batch.len() == BATCH {
                    unpack_batch(&mut batch, out)?;
                }
            }
            Err(_) if r.ended => {
                tail.truncated = true;
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }
    unpack_batch(&mut batch, out)?;
    Ok(tail)
}

/// Unpacking is deflate plus a thumbnail's mip pyramid, which is real work on
/// a corpus this size, and every record is independent of every other.
///
/// A later record for a path replaces an earlier one: that is a file which
/// changed, and was described again and appended.
fn unpack_batch(
    batch: &mut Vec<(Head, Vec<u8>, Span)>,
    out: &mut HashMap<String, (Key, Record, Span)>,
) -> Result<(), Reject> {
    let done: Vec<Result<(String, (Key, Record, Span))>> = batch
        .par_iter()
        .map(|(h, blob, span)| {
            let (kps, desc, px) = unpack(blob, h.n, h.tw, h.th)?;
            Ok((
                h.path.clone(),
                (
                    h.key,
                    Record {
                        feats: Features { w: h.w, h: h.h, kps, desc },
                        thumb: Thumb::new(h.tw, h.th, h.scale, px),
                    },
                    *span,
                ),
            ))
        })
        .collect();
    batch.clear();
    for r in done {
        let (path, rec) = r?;
        out.insert(path, rec);
    }
    Ok(())
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
/// The existence check is what keeps the file from growing forever. A record
/// is worth keeping while the file it describes is there to be matched again;
/// once the file is gone the record can never be used, only carried. It is one
/// `stat` per record not seen this run, and it is why `--prune-cache` is a
/// flag rather than the default: an image is a few milliseconds to describe,
/// where the video fingerprint this idea comes from is minutes, so a record
/// dropped by mistake — an unmounted drive, say — costs a re-analysis rather
/// than an afternoon, and that is a cheap enough mistake to make
/// automatically.
pub fn carry_over(cached: &HashMap<String, (Key, Record, Span)>) -> Vec<(&str, Span)> {
    cached
        .iter()
        .filter(|(path, _)| Path::new(path.as_str()).exists())
        .map(|(path, e)| (path.as_str(), e.2))
        .collect()
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
            dir.join(FILE_NAME)
        );
        let trailing = PathBuf::from(format!("{}/", dir.join("sub").display()));
        assert_eq!(
            resolve_path(Some(&trailing), &mut problems).unwrap(),
            dir.join("sub").join(FILE_NAME)
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
                        response: 0.001 + f / 100_000.0,
                    }
                })
                .collect();
            // Descriptor and thumbnail bytes that use the whole range, so that
            // a Paeth residual and a byte plane both have something to get
            // wrong at the edges.
            let desc: Vec<u8> = (0..n * DESC_LEN).map(|i| ((i * 37 + i / 128) % 256) as u8).collect();
            let px: Vec<u8> = (0..w * h).map(|i| ((i * 91) % 256) as u8).collect();
            let feats = Features { w: 640, h: 480, kps, desc };
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
                assert_eq!(got.response.to_bits(), want.response.to_bits());
            }
        }
    }

    /// A truncated record is a damaged cache, not a panic and not a silently
    /// short one.
    #[test]
    fn a_short_record_is_rejected_rather_than_trusted() {
        let feats = Features { w: 8, h: 8, kps: Vec::new(), desc: Vec::new() };
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
            Key { len: 1, mtime: 2 },
            Record {
                feats: Features { w: 1, h: 1, kps: Vec::new(), desc: Vec::new() },
                thumb: Thumb::new(1, 1, 1.0, vec![0]),
            },
            Span { at: 0, len: 0 },
        );
        let mut cached = HashMap::new();
        cached.insert(here.display().to_string(), record());
        cached.insert(gone.display().to_string(), record());
        cached.insert("scanned-this-run.jpg".to_string(), record());

        // The walked file's record is gone from the map before this is
        // called, so what is left is the untouched set.
        cached.remove("scanned-this-run.jpg");
        let kept = carry_over(&cached);
        assert_eq!(kept.len(), 1, "only the untouched file that still exists");
        assert_eq!(kept[0].0, here.display().to_string());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The magic and the three settings.
    const HEADER_LEN: u64 = MAGIC.len() as u64 + 3 * 4;

    const SETTINGS: Settings = Settings { work_size: 384, features: 600, thumb: 128 };

    fn analysis(n: usize, seed: u8) -> (Features, Thumb) {
        let kps = (0..n)
            .map(|i| Keypoint { x: i as f32, y: seed as f32, sigma: 1.6, angle: 0.0, response: 0.01 })
            .collect();
        let desc = (0..n * DESC_LEN).map(|i| (i as u8).wrapping_add(seed)).collect();
        (Features { w: 64, h: 48, kps, desc }, Thumb::new(8, 6, 0.125, vec![seed; 48]))
    }

    fn reopen(path: &Path) -> (HashMap<String, (Key, Record, Span)>, Store, bool) {
        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let (got, store) = open(path, SETTINGS, &mut problems);
        (got, store, problems.any())
    }

    /// What a worker appends, the next run reads — and a later record for the
    /// same path wins, since that is a file described again after it changed.
    #[test]
    fn appended_records_are_read_back_and_the_last_one_wins() {
        let dir = scratch("append");
        let path = dir.join(FILE_NAME);
        let (got, store, bad) = reopen(&path);
        assert!(got.is_empty() && !bad);
        let (f1, t1) = analysis(3, 1);
        let (f2, t2) = analysis(5, 2);
        store.append("a.jpg", Key { len: 1, mtime: 1 }, &f1, &t1).unwrap();
        store.append("b.jpg", Key { len: 2, mtime: 2 }, &f1, &t1).unwrap();
        store.append("a.jpg", Key { len: 3, mtime: 3 }, &f2, &t2).unwrap();
        assert_eq!(store.records(), 3);
        drop(store);

        let (got, store, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!(store.records(), 3, "the superseded record is still in the file");
        assert_eq!(got.len(), 2);
        let a = &got["a.jpg"];
        assert_eq!((a.0.len, a.1.feats.kps.len()), (3, 5));
        assert_eq!(a.1.feats.desc, f2.desc);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A process killed part-way through an append leaves half a record. That
    /// costs the one record: the rest are read, the tail is cut off, and what
    /// is appended afterwards is readable.
    #[test]
    fn a_torn_final_record_costs_that_record_and_nothing_else() {
        let dir = scratch("torn");
        let path = dir.join(FILE_NAME);
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(4, 3);
        store.append("a.jpg", Key { len: 1, mtime: 1 }, &f, &t).unwrap();
        let b = store.append("b.jpg", Key { len: 1, mtime: 1 }, &f, &t).unwrap();
        drop(store);
        for cut in [1, 9, b.len / 2, b.len - 1] {
            let file = OpenOptions::new().write(true).open(&path).unwrap();
            file.set_len(b.at + cut).unwrap();
            drop(file);
            let (got, store, bad) = reopen(&path);
            assert!(!bad, "a torn tail is not a damaged cache");
            assert_eq!(got.keys().collect::<Vec<_>>(), ["a.jpg"]);
            assert_eq!(std::fs::metadata(&path).unwrap().len(), b.at);
            store.append("b.jpg", Key { len: 1, mtime: 1 }, &f, &t).unwrap();
            drop(store);
            let (got, _, bad) = reopen(&path);
            assert!(!bad && got.len() == 2);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Compaction keeps exactly what it is handed, copied rather than packed
    /// again, and a stale cache is replaced by an empty one on opening.
    #[test]
    fn compaction_keeps_what_it_is_given_and_a_stale_file_starts_again() {
        let dir = scratch("compact");
        let path = dir.join(FILE_NAME);
        let (_, store, _) = reopen(&path);
        let (f, t) = analysis(6, 4);
        let spans: Vec<Span> = ["c.jpg", "a.jpg", "b.jpg", "a.jpg"]
            .iter()
            .map(|p| store.append(p, Key { len: 7, mtime: 7 }, &f, &t).unwrap())
            .collect();
        let mut store = store;
        let mut keep = vec![("c.jpg", spans[0]), ("a.jpg", spans[3])];
        store.compact(SETTINGS, &mut keep).unwrap();
        drop(store);
        let (got, store, bad) = reopen(&path);
        assert!(!bad);
        assert_eq!(store.records(), 2);
        let mut names: Vec<_> = got.keys().cloned().collect();
        names.sort();
        assert_eq!(names, ["a.jpg", "c.jpg"]);
        assert_eq!(got["a.jpg"].1.feats.desc, f.desc);
        drop(store);

        let log = crate::problems::Log::default();
        let mut problems = Problems::new(&log);
        let other = Settings { work_size: 640, ..SETTINGS };
        let (got, store) = open(&path, other, &mut problems);
        assert!(got.is_empty() && !problems.any() && store.records() == 0);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), HEADER_LEN);
        std::fs::remove_dir_all(&dir).ok();
    }
}
