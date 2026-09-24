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
//! Records pack and unpack in parallel, in batches, so neither the clock nor
//! the peak notices — and a run whose records all came from the cache does not
//! rewrite it at all, which is what keeps a threshold sweep from paying for a
//! deflate of the whole corpus a dozen times over.

use crate::problems::Problems;
use crate::sift::{Features, Keypoint, DESC_LEN};
use crate::verify::Thumb;
use anyhow::{anyhow, bail, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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

/// Writes the flat format straight to the file. The whole cache of a large
/// corpus is hundreds of megabytes; assembling it in memory first doubles the
/// run's peak for the length of one write.
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
}

/// Why a cache file produced nothing.
///
/// The two are worth keeping apart because only one of them is worth telling
/// anyone about. A cache written at another working size holds records that
/// describe a different analysis, so discarding it whole is the format doing
/// its job — see the note on `Settings` — and it happens every time a sweep
/// changes `--work-size`. A file that is not a cache at all, or that stops in
/// the middle of a record, is a file the user pointed at and will keep
/// pointing at, and it costs a full re-analysis every run until it is noticed.
enum Reject {
    /// Written by a run with different extraction settings. By design.
    Stale,
    /// Not a cache, or truncated: something to say out loud.
    Damaged(anyhow::Error),
}

impl From<anyhow::Error> for Reject {
    fn from(e: anyhow::Error) -> Self {
        Reject::Damaged(e)
    }
}

/// Read what is usable out of the cache at `path`, reporting only what is
/// worth reporting.
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
pub fn load(path: &Path, want: Settings, problems: &mut Problems) -> HashMap<String, (Key, Record)> {
    let mut out = HashMap::new();
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        // Not being there yet is the first run against this cache, which is
        // every first run and not a problem.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return out,
        Err(e) => {
            problems.cache(format!("could not read {}: {e}", path.display()));
            return out;
        }
    };
    match read_stream(std::io::BufReader::with_capacity(1 << 20, file), want, &mut out) {
        Ok(()) => out,
        Err(Reject::Stale) => HashMap::new(),
        Err(Reject::Damaged(e)) => {
            problems.cache(format!("ignoring {}: {e}", path.display()));
            HashMap::new()
        }
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

struct Rd<R: Read>(R);
impl<R: Read> Rd<R> {
    fn take(&mut self, n: usize) -> Result<Vec<u8>> {
        let mut v = vec![0u8; n];
        self.0.read_exact(&mut v).map_err(|_| anyhow!("cache truncated"))?;
        Ok(v)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut v = [0u8; N];
        self.0.read_exact(&mut v).map_err(|_| anyhow!("cache truncated"))?;
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
            match self.0.read(&mut v[got..]) {
                Ok(0) if got == 0 => return Ok(None),
                Ok(0) => bail!("cache truncated"),
                Ok(k) => got += k,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Some(u64::from_le_bytes(v) as usize))
    }
}

/// A record's path is written before its shape, so a file claiming an absurd
/// one must not be allowed to ask for that much memory first.
const MAX_PATH: usize = 1 << 16;

fn read_stream<R: Read>(r: R, want: Settings, out: &mut HashMap<String, (Key, Record)>) -> Result<(), Reject> {
    let mut r = Rd(r);
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
    let mut batch: Vec<(Head, Vec<u8>)> = Vec::with_capacity(BATCH);
    while let Some(len) = r.len_or_eof()? {
        if len > MAX_PATH {
            return Err(anyhow!("record claims a {len}-byte path").into());
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
        batch.push((head, blob));
        if batch.len() == BATCH {
            unpack_batch(&mut batch, out)?;
        }
    }
    unpack_batch(&mut batch, out)?;
    Ok(())
}

/// Unpacking is deflate plus a thumbnail's mip pyramid, which is real work on
/// a corpus this size, and every record is independent of every other.
fn unpack_batch(batch: &mut Vec<(Head, Vec<u8>)>, out: &mut HashMap<String, (Key, Record)>) -> Result<(), Reject> {
    let done: Vec<Result<(String, (Key, Record))>> = batch
        .par_iter()
        .map(|(h, blob)| {
            let (kps, desc, px) = unpack(blob, h.n, h.tw, h.th)?;
            Ok((
                h.path.clone(),
                (
                    h.key,
                    Record {
                        feats: Features { w: h.w, h: h.h, kps, desc },
                        thumb: Thumb::new(h.tw, h.th, h.scale, px),
                    },
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
/// for: what is written back is this run's analysis *plus* this. Without it a
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
///
/// Sorted, so that the same corpus in the same state writes the same bytes.
pub fn carry_over(cached: &HashMap<String, (Key, Record)>) -> Vec<(&str, Key, &Features, &Thumb)> {
    let mut out: Vec<_> = cached
        .iter()
        .filter(|(path, _)| Path::new(path.as_str()).exists())
        .map(|(path, (k, rec))| (path.as_str(), *k, &rec.feats, &rec.thumb))
        .collect();
    out.sort_unstable_by_key(|e| e.0);
    out
}

/// The temporary file carries the process id because the cache is shared.
/// Two runs writing one cache is last-writer-wins, which costs the loser its
/// records and nothing else; two runs writing one *temporary* file would
/// interleave their bytes and then rename the result into place, which is how
/// a cache becomes damaged rather than merely out of date.
pub fn save(path: &Path, settings: Settings, entries: &[(&str, Key, &Features, &Thumb)]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    if let Some(d) = tmp.parent() {
        std::fs::create_dir_all(d).ok();
    }
    let file = std::fs::File::create(&tmp)?;
    let mut b = Buf(std::io::BufWriter::with_capacity(1 << 20, file));
    let wrote = (|| -> Result<()> {
        b.0.write_all(MAGIC)?;
        b.u32(settings.work_size)?;
        b.u32(settings.features)?;
        b.u32(settings.thumb)?;
        // A batch at a time: packing is deflate over a hundred kilobytes and
        // is worth spreading over the cores, and a batch of packed records is
        // megabytes where the whole cache is most of a gigabyte.
        for chunk in entries.chunks(BATCH) {
            let packed: Vec<Result<Vec<u8>>> =
                chunk.par_iter().map(|(_, _, f, t)| pack(f, t)).collect();
            for ((p, k, f, t), blob) in chunk.iter().zip(packed) {
                let blob = blob?;
                b.bytes(p.as_bytes())?;
                b.u64(k.len)?;
                b.i64(k.mtime)?;
                b.u32(f.w)?;
                b.u32(f.h)?;
                b.u32(f.kps.len() as u32)?;
                b.u32(t.w as u32)?;
                b.u32(t.h as u32)?;
                b.f32(t.scale)?;
                b.bytes(&blob)?;
            }
        }
        b.0.flush()?;
        Ok(())
    })();
    drop(b);
    if let Err(e) = wrote.and_then(|()| Ok(std::fs::rename(&tmp, path)?)) {
        // A half-written temporary file is this run's litter, and the next run
        // will not know to clear it: the name carries a process id precisely
        // so that no other run touches it.
        std::fs::remove_file(&tmp).ok();
        return Err(e);
    }
    Ok(())
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
}
