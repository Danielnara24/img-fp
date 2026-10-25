//! The last run's matching, kept beside the cache: what a run whose every
//! input is the same as the last one's reads back instead of matching again.
//!
//! **A rescan of an unchanged folder matched it all again.** The cache spares
//! the analysis, and the analysis was then put through the vocabulary, the
//! queries, verification, the second look and propagation as if nothing had
//! been seen: on 9,191 images, 26 s and 134 CPU-seconds for the same pairs as
//! last time, of which reading the cache was 0.4 s. The window's "Scan
//! again" is that rescan. So what the matching stages hand on — the claims,
//! and what they said on the console — is kept with a fingerprint of
//! everything they read (`Fingerprint`), and a run with the same fingerprint
//! takes them and goes on to the assembly, which is worked out afresh: the
//! groups, the keep rule and the report are the same code over the same
//! claims, so the same bytes.
//!
//! What the fingerprint holds: the build (`IMGFP_BUILD`, a hash of every
//! source file, the compiler and its flags, made by `build.rs`: another build
//! may match otherwise, and a version number does not change between two
//! builds of one; `img-fp` and `img-fp-gui` of one build share it), the
//! settings the matching reads, and for every file in the order it is
//! numbered its path, its key, what reading it came to and which side of
//! `--against` it is on. Not the thread count: the claims are the same at any
//! (IMGS-ALL, cached, at `-t 3` and `-t 8`: the same report to the byte), and
//! with it a run at another `-t` matched everything again. A file without a
//! key is not vouched for by anything, and a run holding one keeps nothing.
//! One run is kept, the last; a run with `--dump`, which records every
//! verdict, neither reads nor keeps one.

use crate::analysis::{Item, Read};
use crate::claims::{Claim, Edge, How};
use crate::sift::Variant;
use crate::verify::Verdict;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"IMGFPM01";

/// The file beside `cache` the last run's matching is kept in.
pub fn path_beside(cache: &Path) -> PathBuf {
    let mut p = cache.as_os_str().to_owned();
    p.push(".matched");
    PathBuf::from(p)
}

/// Everything the matching stages read, hashed. Two runs with one
/// fingerprint make the same claims.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fingerprint(u64, u64);

/// Two FNV-1a hashes from different starts: stable across builds, and a
/// collision of both is not a thing to plan for.
struct Hash(u64, u64);

impl Hash {
    fn new() -> Hash {
        Hash(0xcbf2_9ce4_8422_2325, 0x6c62_272e_07bb_0142)
    }
    fn bytes(&mut self, b: &[u8]) {
        const P: u64 = 0x0100_0000_01b3;
        for &x in b {
            self.0 = (self.0 ^ x as u64).wrapping_mul(P);
            self.1 = (self.1 ^ x as u64).wrapping_mul(P).rotate_left(5);
        }
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
}

/// The settings a run's matching reads.
pub struct Asked {
    pub work_size: usize,
    pub candidates: usize,
    pub points: u32,
    pub overlap: f32,
    pub correlation: f32,
    pub skip_mirrored: bool,
}

impl Fingerprint {
    /// `None` where nothing can vouch for the run's inputs: a file with no
    /// key.
    pub fn of(asked: &Asked, files: &[PathBuf], keys: &[Option<crate::cache::Key>], items: &[Item], library: &[bool]) -> Option<Fingerprint> {
        use std::os::unix::ffi::OsStrExt;
        let mut h = Hash::new();
        h.bytes(MAGIC);
        h.bytes(env!("IMGFP_BUILD").as_bytes());
        for v in [asked.work_size as u64, asked.candidates as u64, asked.points as u64, asked.overlap.to_bits() as u64, asked.correlation.to_bits() as u64, asked.skip_mirrored as u64, files.len() as u64] {
            h.u64(v);
        }
        for (i, f) in files.iter().enumerate() {
            let name = f.as_os_str().as_bytes();
            h.u64(name.len() as u64);
            h.bytes(name);
            let read = match &items[i].read {
                Read::Whole => 0,
                Read::Damaged(_) => 1,
                Read::Refused(_) => 2,
                Read::Failed(_) => 3,
            };
            h.u64(read);
            h.u64(library.get(i).copied().unwrap_or(false) as u64);
            if read <= 1 {
                let k = keys[i]?;
                for v in [k.len, k.mtime as u64, k.ctime as u64] {
                    h.u64(v);
                }
            }
        }
        Some(Fingerprint(h.0, h.1))
    }
}

/// What the matching handed on: its claims, and what it said on the console.
pub struct Matched {
    pub claims: Vec<Claim>,
    pub notes: Vec<String>,
}

/// The kept matching, if it was made by a run with fingerprint `fp`.
pub fn load(path: &Path, fp: Fingerprint) -> Option<Matched> {
    let b = std::fs::read(path).ok()?;
    let (body, sum) = b.split_at_checked(b.len().checked_sub(16)?)?;
    let mut h = Hash::new();
    h.bytes(body);
    if [h.0.to_le_bytes(), h.1.to_le_bytes()].concat() != sum {
        return None;
    }
    let mut r = Bytes { b: body, at: 0 };
    if r.take(8)? != MAGIC || (r.u64()?, r.u64()?) != (fp.0, fp.1) {
        return None;
    }
    let n = r.u64()? as usize;
    let mut claims = Vec::with_capacity(n.min(body.len() / 80));
    for _ in 0..n {
        let (i, j) = (r.u32()? as usize, r.u32()? as usize);
        let how = match r.u8()? {
            0 => How::Direct,
            1 => How::Propagated,
            2 => How::Corroborated,
            _ => return None,
        };
        let mut m = [0f32; 6];
        for v in m.iter_mut() {
            *v = r.f32()?;
        }
        let variant = Variant { mirror: r.u8()? != 0, invert: r.u8()? != 0 };
        let v = Verdict {
            m,
            variant,
            n_in: r.u32()?,
            centred: r.u8()? != 0,
            framed: r.u8()? != 0,
            n_match: r.u32()?,
            ov_a: r.f32()?,
            ov_b: r.f32()?,
            scale: r.f32()?,
            rot_deg: r.f32()?,
            blk: r.f32()?,
            blk_n: r.u32()?,
            blk_min: r.f32()?,
            ncc: r.f32()?,
        };
        claims.push(Claim { e: Edge::new(i, j, v), how });
    }
    let k = r.u64()? as usize;
    let mut notes = Vec::with_capacity(k.min(64));
    for _ in 0..k {
        let len = r.u32()? as usize;
        notes.push(String::from_utf8(r.take(len)?.to_vec()).ok()?);
    }
    (r.at == body.len()).then_some(Matched { claims, notes })
}

/// Keep `m` as the matching of a run with fingerprint `fp`, in place of the
/// one kept before. Written beside it and renamed, so a reader finds the old
/// one or the new one; one that cannot be written is the next run matching
/// again, and is not said.
pub fn save(path: &Path, fp: Fingerprint, m: &Matched) {
    let mut b = Vec::with_capacity(64 + m.claims.len() * 78);
    b.extend_from_slice(MAGIC);
    for v in [fp.0, fp.1, m.claims.len() as u64] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    let f32s = |b: &mut Vec<u8>, vs: &[f32]| vs.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
    for c in &m.claims {
        let (i, j) = c.e.ends();
        b.extend_from_slice(&(i as u32).to_le_bytes());
        b.extend_from_slice(&(j as u32).to_le_bytes());
        b.push(match c.how {
            How::Direct => 0,
            How::Propagated => 1,
            How::Corroborated => 2,
        });
        let v = &c.e.v;
        f32s(&mut b, &v.m);
        b.extend_from_slice(&[v.variant.mirror as u8, v.variant.invert as u8]);
        b.extend_from_slice(&v.n_in.to_le_bytes());
        b.extend_from_slice(&[v.centred as u8, v.framed as u8]);
        b.extend_from_slice(&v.n_match.to_le_bytes());
        f32s(&mut b, &[v.ov_a, v.ov_b, v.scale, v.rot_deg, v.blk]);
        b.extend_from_slice(&v.blk_n.to_le_bytes());
        f32s(&mut b, &[v.blk_min, v.ncc]);
    }
    b.extend_from_slice(&(m.notes.len() as u64).to_le_bytes());
    for n in &m.notes {
        b.extend_from_slice(&(n.len() as u32).to_le_bytes());
        b.extend_from_slice(n.as_bytes());
    }
    let mut h = Hash::new();
    h.bytes(&b);
    b.extend_from_slice(&h.0.to_le_bytes());
    b.extend_from_slice(&h.1.to_le_bytes());
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, &b).and_then(|()| std::fs::rename(&tmp, path)).is_err() {
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
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_bits(self.u32()?))
    }
}

/// The file beside `cache` the files the last run proved byte-identical are
/// kept in (`proven_identical`).
pub fn identical_beside(cache: &Path) -> PathBuf {
    let mut p = cache.as_os_str().to_owned();
    p.push(".same");
    PathBuf::from(p)
}

const SAME_MAGIC: &[u8; 8] = b"IMGFPS01";

/// The pairs of this run's files (by number, from `names` and `keys`, a
/// file's canonical name and key) that the last run compared byte for byte
/// and found identical, where neither has changed since: each a file and the
/// one it was found the same as.
///
/// **A rescan read every copy whole again.** The byte pass (`same::exact_groups`)
/// runs before the cache is asked, and it is a byte comparison, by design: a
/// shared hash is a reason to compare, not a verdict. So a folder of 47
/// duplicate photographs, nothing changed, was 251 MB read on every run, 0.66
/// s of a 0.72 s rescan on this machine's SSD, and a share's worth of minutes.
/// The comparison is still what proves two files the same; what is kept is
/// that it was made, and it holds while both files are as they were when it
/// was, by the key the analysis cache already trusts (size, modification and
/// change time to the nanosecond, under the canonical name). One run is kept,
/// the last, as for the matching.
pub fn proven_identical(path: &Path, names: &[PathBuf], keys: &[Option<crate::cache::Key>]) -> Vec<(usize, usize)> {
    use std::os::unix::ffi::OsStrExt;
    let Some(kept) = load_identical(path) else { return Vec::new() };
    let by_name: std::collections::HashMap<&[u8], usize> = names.iter().enumerate().map(|(i, n)| (n.as_os_str().as_bytes(), i)).collect();
    let now = |name: &[u8], key: (u64, i64, i64)| {
        let &i = by_name.get(name)?;
        let k = keys[i]?;
        ((k.len, k.mtime, k.ctime) == key).then_some(i)
    };
    kept.iter().filter_map(|(a, ka, b, kb)| Some((now(a, *ka)?, now(b, *kb)?))).filter(|(a, b)| a != b).collect()
}

type Kept = (Vec<u8>, (u64, i64, i64), Vec<u8>, (u64, i64, i64));

fn load_identical(path: &Path) -> Option<Vec<Kept>> {
    let b = std::fs::read(path).ok()?;
    let (body, sum) = b.split_at_checked(b.len().checked_sub(16)?)?;
    let mut h = Hash::new();
    h.bytes(body);
    if [h.0.to_le_bytes(), h.1.to_le_bytes()].concat() != sum {
        return None;
    }
    let mut r = Bytes { b: body, at: 0 };
    if r.take(8)? != SAME_MAGIC {
        return None;
    }
    let n = r.u64()? as usize;
    let mut out = Vec::with_capacity(n.min(body.len() / 64));
    let file = |r: &mut Bytes| -> Option<(Vec<u8>, (u64, i64, i64))> {
        let len = r.u32()? as usize;
        let name = r.take(len)?.to_vec();
        Some((name, (r.u64()?, r.u64()? as i64, r.u64()? as i64)))
    };
    for _ in 0..n {
        let (a, ka) = file(&mut r)?;
        let (b, kb) = file(&mut r)?;
        out.push((a, ka, b, kb));
    }
    (r.at == body.len()).then_some(out)
}

/// Keep this run's sets of byte-identical files (`groups`, by number) for
/// the next (`proven_identical`), in place of the last run's: each file but
/// the first of a set, beside the first. Written beside and renamed; one
/// that cannot be written is the next run comparing again, and is not said.
pub fn keep_identical(path: &Path, groups: &[Vec<usize>], names: &[PathBuf], keys: &[Option<crate::cache::Key>]) {
    use std::os::unix::ffi::OsStrExt;
    let mut entries = Vec::new();
    for g in groups {
        let Some((&head, rest)) = g.split_first() else { continue };
        let Some(kh) = keys[head] else { continue };
        entries.extend(rest.iter().filter_map(|&i| Some((i, keys[i]?, head, kh))));
    }
    let mut b = Vec::with_capacity(16 + entries.len() * 128);
    b.extend_from_slice(SAME_MAGIC);
    b.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    for (i, ki, j, kj) in entries {
        for (f, k) in [(i, ki), (j, kj)] {
            let name = names[f].as_os_str().as_bytes();
            b.extend_from_slice(&(name.len() as u32).to_le_bytes());
            b.extend_from_slice(name);
            for v in [k.len, k.mtime as u64, k.ctime as u64] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    let mut h = Hash::new();
    h.bytes(&b);
    b.extend_from_slice(&h.0.to_le_bytes());
    b.extend_from_slice(&h.1.to_le_bytes());
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, &b).and_then(|()| std::fs::rename(&tmp, path)).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Claims kept come back as they went, every field to the bit, and only
    /// for the fingerprint they were kept under.
    #[test]
    fn kept_claims_come_back_whole_for_their_fingerprint_alone() {
        let dir = std::env::temp_dir().join(format!("img-fp-matched-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("m");
        let v = Verdict {
            m: [0.5, -0.25, 3.0, 0.125, 1.5, -7.0],
            variant: Variant { mirror: true, invert: false },
            n_in: 12,
            centred: true,
            framed: false,
            n_match: 40,
            ov_a: 0.9,
            ov_b: 0.7,
            scale: 1.25,
            rot_deg: -3.5,
            blk: 0.97,
            blk_n: 30,
            blk_min: 0.91,
            ncc: f32::NAN,
        };
        let m = Matched {
            claims: vec![Claim { e: Edge::new(3, 9, v), how: How::Propagated }, Claim { e: Edge::new(0, 1, Verdict::default()), how: How::Corroborated }],
            notes: vec!["Note: one cluster".into()],
        };
        let fp = Fingerprint(1, 2);
        save(&p, fp, &m);
        let back = load(&p, fp).expect("read back");
        assert_eq!(back.notes, m.notes);
        let bits = |c: &Claim| {
            let v = c.e.v;
            (c.e.ends(), c.how, v.m.map(f32::to_bits), v.variant, v.n_in, v.centred, v.framed, v.n_match,
             [v.ov_a, v.ov_b, v.scale, v.rot_deg, v.blk, v.blk_min, v.ncc].map(f32::to_bits), v.blk_n)
        };
        assert_eq!(back.claims.iter().map(bits).collect::<Vec<_>>(), m.claims.iter().map(bits).collect::<Vec<_>>());
        assert!(load(&p, Fingerprint(1, 3)).is_none(), "another run's");
        let mut b = std::fs::read(&p).unwrap();
        b[20] ^= 1;
        std::fs::write(&p, b).unwrap();
        assert!(load(&p, fp).is_none(), "damaged");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The sets kept are given back for the files whose names and keys are
    /// as they were, and nothing for a file changed since.
    #[test]
    fn identical_files_are_kept_for_as_long_as_neither_changes() {
        use crate::cache::Key;
        let dir = std::env::temp_dir().join(format!("img-fp-same-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("c.same");
        let key = |n: i64| Some(Key { len: 10, mtime: n, ctime: n, dev: 0 });
        let names: Vec<PathBuf> = ["/x/a", "/x/b", "/x/c", "/y/d"].iter().map(PathBuf::from).collect();
        keep_identical(&p, &[vec![0, 1, 2]], &names, &[key(1), key(2), key(3), key(4)]);
        // The next run numbers its files otherwise, and c has changed.
        let next: Vec<PathBuf> = ["/x/c", "/x/a", "/x/b"].iter().map(PathBuf::from).collect();
        let mut got = proven_identical(&p, &next, &[key(9), key(1), key(2)]);
        got.sort();
        assert_eq!(got, vec![(2, 1)]);
        std::fs::write(&p, b"IMGFPS01 damaged").unwrap();
        assert!(proven_identical(&p, &next, &[key(9), key(1), key(2)]).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
