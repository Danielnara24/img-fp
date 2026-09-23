//! On-disk cache of the per-image analysis.
//!
//! Describing an image is by far the most expensive stage, and it depends only
//! on the file and the extraction settings. Caching it makes a rescan of a
//! mostly-unchanged directory nearly free, and makes tuning the matching
//! stages practical.
//!
//! The format is a flat binary blob: a header holding the settings, then one
//! record per file. A record is keyed by path, size and modification time, so
//! an edited file is re-described rather than trusted. Settings live in the
//! header, so changing the working size or the feature budget invalidates the
//! whole file rather than silently mixing two kinds of record.

use crate::problems::Problems;
use crate::sift::{Features, Keypoint, DESC_LEN};
use crate::verify::Thumb;
use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

const MAGIC: &[u8; 8] = b"IMGFPC02";

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

pub struct Key {
    pub len: u64,
    pub mtime: i64,
}

pub fn key_of(path: &Path) -> Option<Key> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as i64;
    Some(Key { len: md.len(), mtime })
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

struct Cur<'a>(&'a [u8], usize);
impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.1 + n > self.0.len() {
            bail!("cache truncated");
        }
        let s = &self.0[self.1..self.1 + n];
        self.1 += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Result<&'a [u8]> {
        let n = self.u64()? as usize;
        self.take(n)
    }
    fn done(&self) -> bool {
        self.1 >= self.0.len()
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
pub fn load(path: &Path, want: Settings, problems: &mut Problems) -> HashMap<String, (Key, Record)> {
    let mut out = HashMap::new();
    let data = match std::fs::read(path) {
        Ok(d) => d,
        // Not being there yet is the first run with `--cache`, which is how
        // the flag is meant to be used and not a problem.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return out,
        Err(e) => {
            problems.cache(format!("could not read {}: {e}", path.display()));
            return out;
        }
    };
    match read_all(&data, want, &mut out) {
        Ok(()) => out,
        Err(Reject::Stale) => HashMap::new(),
        Err(Reject::Damaged(e)) => {
            problems.cache(format!("ignoring {}: {e}", path.display()));
            HashMap::new()
        }
    }
}

fn read_all(data: &[u8], want: Settings, out: &mut HashMap<String, (Key, Record)>) -> Result<(), Reject> {
    let mut c = Cur(data, 0);
    if c.take(8)? != MAGIC {
        return Err(anyhow!("not a cache file").into());
    }
    let got = Settings { work_size: c.u32()?, features: c.u32()?, thumb: c.u32()? };
    if got != want {
        return Err(Reject::Stale);
    }
    while !c.done() {
        let path = String::from_utf8_lossy(c.bytes()?).into_owned();
        let key = Key { len: c.u64()?, mtime: c.i64()? };
        let (w, h) = (c.u32()?, c.u32()?);
        let n = c.u32()? as usize;
        let mut kps = Vec::with_capacity(n);
        for _ in 0..n {
            kps.push(Keypoint { x: c.f32()?, y: c.f32()?, sigma: c.f32()?, angle: c.f32()?, response: c.f32()? });
        }
        let desc = c.take(n * DESC_LEN)?.to_vec();
        let (tw, th, ts) = (c.u32()? as u16, c.u32()? as u16, c.f32()?);
        let px = c.take(tw as usize * th as usize)?.to_vec();
        out.insert(
            path,
            (key, Record { feats: Features { w, h, kps, desc }, thumb: Thumb::new(tw, th, ts, px) }),
        );
    }
    Ok(())
}

pub fn save(path: &Path, settings: Settings, entries: &[(&str, Key, &Features, &Thumb)]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    if let Some(d) = tmp.parent() {
        std::fs::create_dir_all(d).ok();
    }
    let file = std::fs::File::create(&tmp)?;
    let mut b = Buf(std::io::BufWriter::with_capacity(1 << 20, file));
    b.0.write_all(MAGIC)?;
    b.u32(settings.work_size)?;
    b.u32(settings.features)?;
    b.u32(settings.thumb)?;
    for (p, k, f, t) in entries {
        b.bytes(p.as_bytes())?;
        b.u64(k.len)?;
        b.i64(k.mtime)?;
        b.u32(f.w)?;
        b.u32(f.h)?;
        b.u32(f.kps.len() as u32)?;
        for kp in f.kps.iter() {
            b.f32(kp.x)?;
            b.f32(kp.y)?;
            b.f32(kp.sigma)?;
            b.f32(kp.angle)?;
            b.f32(kp.response)?;
        }
        b.0.write_all(&f.desc)?;
        b.u32(t.w as u32)?;
        b.u32(t.h as u32)?;
        b.f32(t.scale)?;
        b.0.write_all(&t.px)?;
    }
    b.0.flush()?;
    drop(b);
    std::fs::rename(&tmp, path)?;
    Ok(())
}
