//! One file's analysis, and what reading the file came to.
//!
//! A file is decoded to its working plane and described (`analyse`), or its
//! record is read back from the cache (`Item::from`): either way an `Item`,
//! whose `read` says whether there is an analysis and, when there is none,
//! why. Everything after the analysis reads that one field; what it means to
//! the run (a skip, a problem, a record worth keeping) is asked of it.

use crate::{cache, decode, sift, timed};
use crate::sift::Features;
use crate::thumb::Thumb;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(crate) const THUMB_LONG: usize = 128;
/// The pixel check's thumbnail is `THUMB_LONG` times `decode::area_factor`
/// on its long side, and never more than this many times `THUMB_LONG`: a
/// strip of a few hundred thousand pixels by one would otherwise ask for a
/// side past what a thumbnail's `u16` holds.
pub(crate) const THUMB_MAX_SIDES: usize = 16;

/// Local features described per image.
///
/// Not an option, because it has no usable range. Measured over 300 to 900 at
/// a fixed vocabulary, it moves F1 by 0.007 and never changes a verdict that
/// matters: 0.971, 0.975, 0.975, 0.976, 0.976, 0.978, 0.977. Above 600 it buys
/// nothing and costs 11% of the run; below it, nothing either, until the point
/// where it stops being about features at all.
///
/// It used to look load-bearing — 400 features merged seven pairs of families
/// on the benchmark corpus — and that was the vocabulary. Both this and
/// `--work-size` move the descriptor count, the descriptor count sized the
/// tree, and the tree could only be sized to a factor of sixteen. With
/// `VocabParams::for_corpus` holding occupancy instead, the cliff is gone and
/// what is left is a plateau, which is not a thing to put on a command line.
pub(crate) const FEATURES: usize = 600;

/// The most keypoints an analysis may hold: what an index entry's 11 bits
/// number (`index::KP_BITS` is read from it). Here because `features_at`
/// caps a finer detail's count at it, so it is part of what an analysis is.
pub(crate) const MOST_KEYPOINTS: usize = 1 << 11;

/// One image's analysis.
///
/// The features and the thumbnail are shared rather than owned, because
/// byte-identical files share an analysis: the exact pass has already found
/// them, one member of each group is described and the rest point at it.
/// Copying instead meant a second megabyte-scale buffer per duplicate — three
/// hundred of them on this corpus — for bytes that are the same bytes.
#[derive(Clone, Default)]
pub(crate) struct Item {
    pub(crate) feats: std::sync::Arc<Features>,
    pub(crate) thumb: std::sync::Arc<Thumb>,
    /// The picture's own size, as shown; `feats` holds the working image's.
    pub(crate) dims: (u32, u32),
    /// What reading the file came to.
    pub(crate) read: Read,
    /// What the keep/delete suggestion reads beyond the analysis.
    pub(crate) traits: decode::Traits,
    /// The library outside the build that made the pixels
    /// (`decode::Decoded::made_by`), which a cached record carries.
    pub(crate) made_by: Option<&'static str>,
    /// The file's content hash, when the analysis was asked to take it from
    /// the bytes it read (`decode::Decoded::hash`).
    pub(crate) hash: Option<decode::Content>,
}

/// What reading a file came to: an analysis, whole or of a part, or none
/// and why. A refused or failed file has an empty analysis.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Read {
    /// Decoded and described. A blank picture is described to no features,
    /// and is still this.
    #[default]
    Whole,
    /// Decoded only in part, and described: what was wrong
    /// (`decode::Decoded::damaged`). Analysed again on every run rather than
    /// kept, so that every run says so.
    Damaged(String),
    /// Turned away for what it is (`decode::Refused`).
    Refused(decode::Refused),
    /// Would not open or decode, and why.
    Failed(String),
}

impl Read {
    /// Whether the file has an analysis.
    pub(crate) fn ok(&self) -> bool {
        matches!(self, Read::Whole | Read::Damaged(_))
    }

    /// What was wrong with a file read only in part.
    pub(crate) fn damaged(&self) -> Option<&str> {
        match self {
            Read::Damaged(why) => Some(why),
            _ => None,
        }
    }

    /// What the file was refused with, as a report says it.
    pub(crate) fn error(&self) -> Option<String> {
        match self {
            Read::Refused(r) => Some(r.to_string()),
            Read::Failed(e) => Some(e.clone()),
            _ => None,
        }
    }
}

impl From<cache::Record> for Item {
    fn from(rec: cache::Record) -> Item {
        Item { feats: rec.feats.into(), thumb: rec.thumb.into(), dims: rec.dims, read: Read::Whole, traits: rec.traits, made_by: None, hash: None }
    }
}

impl Item {
    /// A cached record as a run at working size `work` and detail `detail`
    /// reads it: its analysis up to the levels the picture has at `detail`,
    /// cut where the record holds more (a finer run made it), or `None`
    /// where it holds fewer, to be analysed again. A coarser detail's
    /// analysis is a finer one's leading levels to the bit
    /// (`a_finer_detail_holds_the_coarser_ones_analysis`), so what is read
    /// is what a run at `detail` makes; a picture with no finer level is the
    /// default's analysis at any detail.
    pub(crate) fn cached_at(rec: cache::Record, work: usize, detail: usize) -> Option<Item> {
        let need = finer_levels(rec.dims.0 as usize, rec.dims.1 as usize, work, detail).len();
        let have = rec.feats.ends.len().saturating_sub(1);
        if have < need {
            return None;
        }
        let mut it = Item::from(rec);
        if have > need {
            let f = &it.feats;
            let mut cut = f.prefix(f.ends[need] as usize);
            if need > 0 {
                cut.ends = f.ends[..=need].to_vec();
            }
            it.feats = std::sync::Arc::new(cut);
        }
        Some(it)
    }
}

/// What a cached analysis was made by, as the cache keys it: the build's
/// analysis (`IMGFP_ANALYSIS`, the hash `build.rs` makes of the sources that
/// compute it, the decoders' versions, the compiler and the target) and the
/// description's parameters as this run passes them. A record made under
/// another is never read: the build or a parameter that could make another
/// analysis says so itself, where a format number had to be raised by hand.
pub(crate) fn version(p: &sift::Params) -> [u8; 16] {
    let mut h = blake3::Hasher::new();
    h.update(env!("IMGFP_ANALYSIS").as_bytes());
    h.update(format!("{p:?}").as_bytes());
    h.finalize().as_bytes()[..16].try_into().expect("16 bytes")
}

/// Thread time spent decoding and describing, in microseconds, for the log.
pub(crate) static T_DECODE: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_SIFT: AtomicU64 = AtomicU64::new(0);

/// How far a small picture may be enlarged: up to `upsample_below`, but never
/// past the working size (512, or 256 under `--fast`), so that nothing is
/// analysed above it and it bounds what every picture costs. It used to
/// enlarge anything under 256 pixels to between 257 and 512 whatever the
/// working size said (a 120-pixel thumbnail cost what a 480-pixel photograph
/// costs at a working size of 140), and a picture the working size had just
/// shrunk cost what 512 costs from a fraction of the detail. A picture shrunk
/// to the working size is never enlarged under this limit, since one doubling
/// would take it past it. A working size of `0`, which shrinks nothing, keeps
/// the whole of `upsample_below`.
pub(crate) fn enlarge_below(work: usize, upsample_below: usize) -> usize {
    if work == 0 { upsample_below } else { upsample_below.min(work) }
}

/// Decode and describe one file at working size `work`, with the levels
/// finer than it up to `detail` (`finer_levels`); and how long the decode
/// took. At `detail` no finer than `work` (the default) it is the analysis
/// at `work` alone.
pub(crate) fn analyse(path: &Path, work: usize, detail: usize, p: &sift::Params, header: Option<&decode::Probe>, planes: &Planes, hash: bool) -> (Item, Duration) {
    let ups = header.map(|h| finer_levels(h.w as usize, h.h as usize, work, detail)).unwrap_or_default();
    if ups.is_empty() {
        return analyse_plain(path, work, p, header, planes, hash);
    }
    analyse_finer(path, work, &ups, p, header, planes, hash)
}

/// `analyse`, as plain.
pub(crate) fn analyse_plain(path: &Path, work: usize, p: &sift::Params, header: Option<&decode::Probe>, planes: &Planes, hash: bool) -> (Item, Duration) {
    let t0 = Instant::now();
    let r = decode::decode_guarded(|| decode::decode_with(path, work, header, hash));
    let decode = t0.elapsed();
    T_DECODE.fetch_add(decode.as_micros() as u64, Ordering::Relaxed);
    let item = match r {
        Ok(d) => {
            let t1 = Instant::now();
            let (feats, thumb) = planes.analysis(&d.work, || {
                let p = sift::Params { upsample_below: enlarge_below(work, p.upsample_below), ..*p };
                // The decode's claim is given back by now; this is the
                // description's (`decode::describing`).
                let up = sift::enlargement(d.work.w.max(d.work.h), p.upsample_below);
                let _room = decode::describing((d.work.w * d.work.h * up * up) as u64);
                let feats = sift::extract(&d.work, &p).spilled();
                let thumb = timed!(4, Thumb::build(&d.work, ((THUMB_LONG as f64 * decode::area_factor(d.work.w, d.work.h)).round() as usize).min(THUMB_LONG * THUMB_MAX_SIDES)));
                (feats.into(), thumb.spilled().into())
            });
            T_SIFT.fetch_add(t1.elapsed().as_micros() as u64, Ordering::Relaxed);
            let traits = decode::Traits { grid: grey_grid(&d.work), ..d.traits };
            let read = d.damaged.map_or(Read::Whole, Read::Damaged);
            Item { feats, thumb, dims: d.size, read, traits, made_by: d.made_by, hash: d.hash }
        }
        Err(e) => {
            let read = decode::refused(&e).map_or_else(|| Read::Failed(crate::why(&e)), Read::Refused);
            Item { read, ..Default::default() }
        }
    };
    (item, decode)
}

/// A level finer than the working size: the picture fitted to `size` as the
/// analysis at that working size fits it, or (`whole`) the picture itself,
/// where it is finer than the level below and coarser than `size`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Finer {
    pub size: usize,
    pub whole: bool,
}

/// The levels a picture shown `w` x `h` is described at beyond working size
/// `work`, up to `detail`: each doubling of `work` the picture reaches, and
/// the picture itself where it stops between two. They depend on the
/// picture and on `work` alone, so a run at a finer `detail` describes every
/// level a run at a coarser one does, and the same.
pub(crate) fn finer_levels(w: usize, h: usize, work: usize, detail: usize) -> Vec<Finer> {
    let af = decode::area_factor(w, h);
    let long = w.max(h);
    let side = |g: usize| (g as f64 * af).round() as usize;
    // The analysis at `work` holds the picture up to its working size.
    let mut below = long.min(side(work));
    let mut out = Vec::new();
    let mut g = work * 2;
    while work > 0 && g <= detail {
        if long >= side(g) {
            out.push(Finer { size: g, whole: false });
            below = side(g);
        } else {
            if long > below {
                out.push(Finer { size: g, whole: true });
            }
            break;
        }
        g *= 2;
    }
    out
}

/// The most features an analysis holds at `size`, `FEATURES` at `work`:
/// each finer level adds `FEATURES / 4` for every `work` the size grows by,
/// short of what an index entry can number. A finer level is one octave,
/// and the analysis at `work` spreads its `FEATURES` over four or five, so a
/// quarter keeps the finer octave about as dense as one of them. Measured on
/// IMGS-ALL at 1024 (`FEATURES` x 1 / 0.5 / 0.25 added): recall 96.56 / 96.55
/// / 96.48%, precision 98.24 / 98.27 / 98.72%, cross-family 20,193 / 19,778 /
/// 13,656; the further features found little and joined sibling pictures.
pub(crate) fn features_at(size: usize, work: usize) -> usize {
    let r = size as f64 / work.max(1) as f64;
    let n = FEATURES as f64 * (1.0 + (r - 1.0).max(0.0) / 4.0);
    (n.round() as usize).min(MOST_KEYPOINTS - 1)
}

/// `analyse` with finer levels: the analysis at `work` exactly as
/// `analyse_plain` makes it, and after its features each finer level's, only
/// keypoints finer than the level below can find, up to `features_at` its
/// size. The levels' ends are kept (`Features::ends`), so a run at a coarser
/// `detail` reads its own analysis as a prefix of this one.
fn analyse_finer(path: &Path, work: usize, ups: &[Finer], p: &sift::Params, header: Option<&decode::Probe>, twins: &Planes, hash: bool) -> (Item, Duration) {
    let t0 = Instant::now();
    let works: Vec<usize> = std::iter::once(work).chain(ups.iter().filter(|u| !u.whole).map(|u| u.size)).chain(ups.iter().any(|u| u.whole).then_some(0)).collect();
    let r = decode::decode_guarded(|| decode::decode_levels(path, &works, header, hash));
    let decode = t0.elapsed();
    T_DECODE.fetch_add(decode.as_micros() as u64, Ordering::Relaxed);
    let item = match r {
        Ok((planes, size, damaged, traits, made_by, hash)) => {
            let t1 = Instant::now();
            let base = &planes[0];
            let (feats, thumb, grid) = twins.levels(&planes, || {
                let pb = sift::Params { upsample_below: enlarge_below(work, p.upsample_below), ..*p };
                let up = sift::enlargement(base.w.max(base.h), pb.upsample_below);
                let finer_px: usize = planes[1..].iter().map(|g| g.w * g.h).sum();
                let _room = decode::describing((base.w * base.h * up * up + finer_px) as u64);
                let b = sift::extract(base, &pb);
                let thumb = timed!(4, Thumb::build(base, ((THUMB_LONG as f64 * decode::area_factor(base.w, base.h)).round() as usize).min(THUMB_LONG * THUMB_MAX_SIDES)));
                // Each finer level in the frame of the analysis at `work`,
                // keeping only what is finer than the level below: that
                // level's octave starts at scale 1.6 of its own pixels.
                let flong = b.w.max(b.h) as f32;
                let to_frame = |g: &decode::Gray| flong / g.w.max(g.h) as f32;
                let mut below = 1.0f32;
                let mut fit = planes[1..].iter();
                let whole = planes.last();
                let mut ins = Vec::with_capacity(ups.len());
                for (k, u) in ups.iter().enumerate() {
                    let g = if u.whole { whole.unwrap() } else { fit.next().unwrap() };
                    let s = to_frame(g);
                    ins.push(sift::LevelIn { plane: g, to_frame: s, tag: k as u32 + 1, cap: features_at(u.size, work).saturating_sub(b.len()), finer_than: 1.6 * below });
                    below = s;
                }
                let f = sift::extract_levels(&ins, b.w, b.h, p);
                let mut ends = vec![b.len() as u32];
                for k in 0..ups.len() {
                    let n = f.kps.iter().filter(|kp| sift::level_tag(kp) == k as u32 + 1).count();
                    ends.push(ends[k] + n as u32);
                }
                let kps: Vec<sift::Keypoint> = b.kps.iter().chain(f.kps.iter()).copied().collect();
                let desc: Vec<u8> = b.desc.iter().chain(f.desc.iter()).copied().collect();
                let feats = Features { w: b.w, h: b.h, kps: kps.into(), desc: desc.into(), ends }.spilled();
                (std::sync::Arc::new(feats), std::sync::Arc::new(thumb.spilled()), grey_grid(base))
            });
            T_SIFT.fetch_add(t1.elapsed().as_micros() as u64, Ordering::Relaxed);
            let traits = decode::Traits { grid, ..traits };
            let read = damaged.map_or(Read::Whole, Read::Damaged);
            Item { feats, thumb, dims: size, read, traits, made_by, hash }
        }
        Err(e) => {
            let read = decode::refused(&e).map_or_else(|| Read::Failed(crate::why(&e)), Read::Refused);
            Item { read, ..Default::default() }
        }
    };
    (item, decode)
}

/// The mean grey (0..=255) of each cell of a 4x4 grid over a working plane,
/// row by row: `Traits::grid`.
fn grey_grid(g: &decode::Gray) -> [f32; 16] {
    let mut sum = [0f64; 16];
    let mut n = [0u32; 16];
    for y in 0..g.h {
        let cy = (y * 4 / g.h.max(1)).min(3);
        for (x, &v) in g.px[y * g.w..(y + 1) * g.w].iter().enumerate() {
            let c = cy * 4 + (x * 4 / g.w.max(1)).min(3);
            sum[c] += v as f64;
            n[c] += 1;
        }
    }
    std::array::from_fn(|c| if n[c] > 0 { (sum[c] / n[c] as f64 * 255.0) as f32 } else { 0.0 })
}

/// The analyses this run has made, by the working plane they were made from,
/// so that files whose bytes differ and whose pictures do not are described
/// once.
///
/// The analysis — features and thumbnail — is a function of the working plane
/// and nothing else, so two files that decode to the same plane have the same
/// analysis, and the second can take the first's. They are common, and the
/// exact pass cannot see them: a photograph saved again losslessly as PNG,
/// TIFF or lossless WebP, or a JPEG whose metadata was stripped or edited.
/// On the four benchmark corpora together 1,113 of 26,886 files are one, 4.1%
/// of the analysis. Everything after the analysis treats them as the separate
/// files they are, so the output is the output without this, to the byte.
///
/// **The key is a hash of the plane, keyed at random for each run**
/// (SipHash-1-3 through `RandomState`), with the plane's size beside it, and
/// keeping every plane to compare it whole is not an option: they are most of
/// a megabyte each. A shared hash is not a comparison, and the exact pass
/// compares bytes for that reason — its hash was FNV, which has collisions
/// built in. This one is a keyed pseudo-random function: no file can be made
/// to collide with another without the key, which exists only inside the run,
/// and two planes collide by chance with probability 2^-64, which over a
/// million files is a pair in some fifty million runs. Hashing costs about
/// 0.2 ms a plane on one core, against some 60 ms of analysis saved per twin.
#[derive(Default)]
pub(crate) struct Planes {
    keys: std::collections::hash_map::RandomState,
    made: std::sync::Mutex<HashMap<(usize, usize, u64), Made>>,
    made_levels: std::sync::Mutex<HashMap<(usize, usize, u64), LevelsMade>>,
}

type Analysis = (std::sync::Arc<Features>, std::sync::Arc<Thumb>);
/// An analysis that one file is making and its twins may wait for.
type Made = std::sync::Arc<std::sync::OnceLock<Analysis>>;

/// An analysis with finer levels, and its grey grid.
type LevelsMade = std::sync::Arc<std::sync::OnceLock<(std::sync::Arc<Features>, std::sync::Arc<Thumb>, [f32; 16])>>;

impl Planes {
    /// `analysis`, for an analysis with finer levels (`analyse_finer`), keyed by
    /// every plane it is made from, the working size's first. Keyed by that
    /// one alone, two pictures alike at the working size and not finer (a
    /// picture and its copy enlarged by doubling each pixel reduce to one
    /// plane at 512) shared the first one's levels, and the second lost its
    /// own.
    fn levels(&self, planes: &[decode::Gray], make: impl FnOnce() -> (std::sync::Arc<Features>, std::sync::Arc<Thumb>, [f32; 16])) -> (std::sync::Arc<Features>, std::sync::Arc<Thumb>, [f32; 16]) {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut h = self.keys.build_hasher();
        for g in planes {
            // SAFETY: as in `analysis`.
            let bytes = unsafe { std::slice::from_raw_parts(g.px.as_ptr() as *const u8, std::mem::size_of_val(&g.px[..])) };
            (g.w, g.h).hash(&mut h);
            bytes.hash(&mut h);
        }
        let key = (planes[0].w, planes[0].h, h.finish());
        let cell = self.made_levels.lock().unwrap().entry(key).or_default().clone();
        cell.get_or_init(make).clone()
    }

    /// The analysis of `g`: one already made from the same plane, or `make`'s.
    /// A twin that arrives while the first is still being described waits for
    /// it rather than describing it again. `make` takes no lock, and its
    /// claim on the decode budget (`decode::describing`) waits only on
    /// claims of decodes and descriptions, none of which waits on a twin: a
    /// thread waiting here holds no claim.
    fn analysis(&self, g: &decode::Gray, make: impl FnOnce() -> Analysis) -> Analysis {
        use std::hash::BuildHasher;
        // SAFETY: an `f32` is four bytes with no padding, and every bit
        // pattern of them is a `u8`.
        let bytes = unsafe { std::slice::from_raw_parts(g.px.as_ptr() as *const u8, std::mem::size_of_val(&g.px[..])) };
        let key = (g.w, g.h, self.keys.hash_one(bytes));
        let cell = self.made.lock().unwrap().entry(key).or_default().clone();
        cell.get_or_init(make).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `build.rs` hashes the analysis's sources into `IMGFP_ANALYSIS`, under
    /// which every cached record is kept. A module those sources read and
    /// the list leaves out could change an analysis without changing the
    /// hash, and a cache would hand the old one back: so none of them may
    /// name a module of the crate outside the list, but these, which hold no
    /// value an analysis is made of: the timing macro and the profiler,
    /// failures' text (`why`), the scratch file the arrays live in, the
    /// progress bar's pace, and the cache's `Record`, the analysis as stored.
    #[test]
    fn the_analysis_reads_only_its_sources() {
        let sources: Vec<&str> = env!("IMGFP_ANALYSIS_SOURCES").split(':').collect();
        let module = |p: &str| -> String {
            let p = p.trim_start_matches("src/").trim_end_matches(".rs");
            p.split('/').next().unwrap().to_string()
        };
        let mut allowed: std::collections::HashSet<String> = sources.iter().filter(|s| s.starts_with("src/")).map(|s| module(s)).collect();
        for m in ["timed", "prof", "why", "spill", "progress", "cache"] {
            allowed.insert(m.to_string());
        }
        let root = env!("CARGO_MANIFEST_DIR");
        let mut outside = Vec::new();
        for s in sources.iter().filter(|s| s.ends_with(".rs") && s.starts_with("src/")) {
            let text = std::fs::read_to_string(format!("{root}/{s}")).unwrap();
            // A file's tests build no analysis.
            let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
            let mut rest = code;
            while let Some(at) = rest.find("crate::") {
                rest = &rest[at + "crate::".len()..];
                let names: Vec<&str> = if let Some(inner) = rest.strip_prefix('{') {
                    inner[..inner.find('}').unwrap()].split(',').map(|n| n.trim().split("::").next().unwrap()).collect()
                } else {
                    vec![&rest[..rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len())]]
                };
                for n in names.into_iter().filter(|n| !n.is_empty()) {
                    if !allowed.contains(n) {
                        outside.push(format!("{s}: crate::{n}"));
                    }
                }
            }
        }
        assert!(outside.is_empty(), "the analysis reads modules build.rs does not hash: {outside:?}");
        assert!(sources.iter().any(|s| s.ends_with("decode/jpeg.rs")), "a folder's files are listed");
        assert!(!sources.iter().any(|s| s.ends_with("decode/tests.rs")));
    }

    /// A grey PNG with texture at every scale, written to a fresh file.
    fn picture(name: &str, w: u32, h: u32) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("img-fp-analysis-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut out = Vec::new();
        {
            let mut e = ::png::Encoder::new(&mut out, w, h);
            e.set_color(::png::ColorType::Grayscale);
            e.set_depth(::png::BitDepth::Eight);
            let mut wr = e.write_header().unwrap();
            let px: Vec<u8> = (0..w * h)
                .map(|i| {
                    let (x, y) = ((i % w) as f32, (i / w) as f32);
                    let v = 128.0 + 50.0 * (x * 0.013).sin() * (y * 0.011).cos() + 40.0 * ((x * 0.09).sin() * (y * 0.07).sin()) + 25.0 * (((x as u32 / 23 + y as u32 / 17) % 3) as f32 - 1.0)
                        + 12.0 * ((x * 0.61 + y * 0.37).sin());
                    v.clamp(0.0, 255.0) as u8
                })
                .collect();
            wr.write_image_data(&px).unwrap();
        }
        std::fs::write(&path, out).unwrap();
        path
    }

    /// Each doubling of the working size the picture reaches, then the
    /// picture itself where it stops between two; nothing for a picture the
    /// working size already holds whole. A coarser detail's levels lead a
    /// finer one's.
    #[test]
    fn finer_levels_are_doublings_then_the_picture() {
        let fit = |size| Finer { size, whole: false };
        let whole = |size| Finer { size, whole: true };
        assert_eq!(finer_levels(4000, 3000, 512, 2048), vec![fit(1024), fit(2048)]);
        assert_eq!(finer_levels(1500, 1000, 512, 2048), vec![fit(1024), whole(2048)], "1,500 is past 1024 x 1.06, short of 2048 x 1.06");
        assert_eq!(finer_levels(700, 525, 512, 2048), vec![whole(1024)]);
        assert_eq!(finer_levels(400, 300, 512, 2048), vec![], "held whole at 512");
        assert_eq!(finer_levels(4000, 3000, 512, 512), vec![], "the default");
        assert_eq!(finer_levels(4000, 3000, 256, 256), vec![], "--fast");
        for (w, h) in [(4000, 3000), (1500, 1000), (700, 525), (5000, 900), (513, 600), (2048, 1536)] {
            let all = finer_levels(w, h, 512, 2048);
            assert!(all.starts_with(&finer_levels(w, h, 512, 1024)), "{w}x{h}");
        }
    }

    fn same_features(a: &Features, b: &Features) -> bool {
        let bits = |f: &Features| f.kps.iter().map(|k| [k.x, k.y, k.sigma, k.angle].map(f32::to_bits)).collect::<Vec<_>>();
        (a.w, a.h) == (b.w, b.h) && bits(a) == bits(b) && a.desc[..] == b.desc[..]
    }

    /// An analysis at a finer detail is the default's analysis, then its
    /// finer levels: the default's features, thumbnail and grid to the bit,
    /// and at 2048 the 1024 analysis as its prefix. So a run at a coarser
    /// detail, made apart, reads exactly what a finer one holds of it.
    #[test]
    fn a_finer_detail_holds_the_coarser_ones_analysis() {
        let path = picture("p.png", 2400, 1800);
        let p = sift::Params { max_features: FEATURES, ..Default::default() };
        let header = decode::probe(&path);
        let at = |detail| analyse(&path, 512, detail, &p, header.as_ref(), &Planes::default(), false).0;
        let (d512, d1024, d2048) = (at(512), at(1024), at(2048));
        assert!(d512.feats.ends.is_empty() && d512.feats.len() > 100);
        assert_eq!(d1024.feats.ends.len(), 2);
        assert_eq!(d2048.feats.ends.len(), 3);
        assert_eq!(d1024.feats.ends[0] as usize, d512.feats.len());
        assert!(same_features(&d1024.feats.prefix(d1024.feats.ends[0] as usize), &d512.feats));
        assert!(same_features(&d2048.feats.prefix(d2048.feats.ends[1] as usize), &d1024.feats));
        assert!(d2048.feats.len() > d1024.feats.len() && d1024.feats.len() > d512.feats.len(), "each level adds keypoints");
        for it in [&d1024, &d2048] {
            assert_eq!(it.thumb.px[..], d512.thumb.px[..]);
            assert_eq!(it.thumb.scale.to_bits(), d512.thumb.scale.to_bits());
            assert_eq!(it.traits.grid, d512.traits.grid);
            assert_eq!(it.dims, d512.dims);
        }
        // Every finer keypoint is finer than the default's finest octave,
        // which starts at scale 1.6 of its plane, the frame.
        assert!(d1024.feats.kps[d1024.feats.ends[0] as usize..].iter().all(|k| k.sigma < 1.6));
        // A picture the default holds whole has no finer level: the default's
        // analysis at any detail.
        let small = picture("s.png", 480, 360);
        let header = decode::probe(&small);
        let a = analyse(&small, 512, 512, &p, header.as_ref(), &Planes::default(), false).0;
        let b = analyse(&small, 512, 2048, &p, header.as_ref(), &Planes::default(), false).0;
        assert!(b.feats.ends.is_empty() && same_features(&a.feats, &b.feats));

        // A record read at another detail: cut to the levels the run's goes
        // to, which are what that run makes, or refused where it holds fewer.
        let rec = |it: &Item| cache::Record { feats: (*it.feats).clone(), thumb: (*it.thumb).clone(), dims: it.dims, traits: it.traits.clone() };
        let read = |it: &Item, detail| Item::cached_at(rec(it), 512, detail);
        let from2048 = read(&d2048, 512).unwrap();
        assert!(from2048.feats.ends.is_empty() && same_features(&from2048.feats, &d512.feats), "the default's analysis");
        let to1024 = read(&d2048, 1024).unwrap();
        assert_eq!(to1024.feats.ends, d1024.feats.ends);
        assert!(same_features(&to1024.feats, &d1024.feats), "the 1024 analysis");
        assert!(read(&d2048, 2048).is_some_and(|it| it.feats.ends == d2048.feats.ends && same_features(&it.feats, &d2048.feats)));
        assert!(read(&d512, 1024).is_none() && read(&d1024, 2048).is_none(), "fewer levels than the picture has: analysed again");
        assert!(read(&b, 512).is_some() && read(&a, 4096).is_some(), "a picture with no finer level is read at any detail");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
