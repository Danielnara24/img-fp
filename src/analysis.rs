//! One file's analysis, and what reading the file came to.
//!
//! A file is decoded to its working plane and described (`analyse`), or its
//! record is read back from the cache (`Item::from`): either way an `Item`,
//! whose `read` says whether there is an analysis and, when there is none,
//! why. Everything after the analysis reads that one field; what it means to
//! the run (a skip, a problem, a record worth keeping) is asked of it.

use crate::{cache, decode, sift, timed, THUMB_LONG, THUMB_MAX_SIDES};
use crate::sift::Features;
use crate::verify::Thumb;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

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
        Item { feats: rec.feats.into(), thumb: rec.thumb.into(), dims: rec.dims, read: Read::Whole, traits: rec.traits }
    }
}

/// Thread time spent decoding and describing, in microseconds, for the log.
pub(crate) static T_DECODE: AtomicU64 = AtomicU64::new(0);
pub(crate) static T_SIFT: AtomicU64 = AtomicU64::new(0);

/// How far a small picture may be enlarged: up to `upsample_below`, but never
/// past the working size, so that nothing is analysed above `--work-size` and
/// the option bounds what every picture costs. It used to enlarge anything
/// under 256 pixels to between 257 and 512 whatever the working size said,
/// which made a 120-pixel thumbnail cost what a 480-pixel photograph costs at
/// `--work-size 140`, and made a picture the working size had just shrunk —
/// at `--work-size 128`, every photograph — cost what 512 costs from a
/// sixteenth of the detail. A picture shrunk to the working size is never
/// enlarged under this limit, since one doubling would take it past it. `0`,
/// which shrinks nothing, keeps the whole of `upsample_below`.
pub(crate) fn enlarge_below(work: usize, upsample_below: usize) -> usize {
    if work == 0 { upsample_below } else { upsample_below.min(work) }
}

/// Decode and describe one file.
pub(crate) fn analyse(path: &Path, work: usize, p: &sift::Params, header: Option<&decode::Probe>, planes: &Planes) -> Item {
    let t0 = Instant::now();
    let r = decode::decode_guarded(|| decode::decode_with(path, work, header));
    T_DECODE.fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
    match r {
        Ok(d) => {
            let t1 = Instant::now();
            let (feats, thumb) = planes.analysis(&d.work, || {
                let p = sift::Params { upsample_below: enlarge_below(work, p.upsample_below), ..*p };
                let feats = sift::extract(&d.work, &p).spilled();
                let thumb = timed!(4, Thumb::build(&d.work, ((THUMB_LONG as f64 * decode::area_factor(d.work.w, d.work.h)).round() as usize).min(THUMB_LONG * THUMB_MAX_SIDES)));
                (feats.into(), thumb.spilled().into())
            });
            T_SIFT.fetch_add(t1.elapsed().as_micros() as u64, Ordering::Relaxed);
            let traits = decode::Traits { grid: grey_grid(&d.work), ..d.traits };
            let read = d.damaged.map_or(Read::Whole, Read::Damaged);
            Item { feats, thumb, dims: d.size, read, traits }
        }
        Err(e) => {
            let read = decode::refused(&e).map_or_else(|| Read::Failed(e.to_string()), Read::Refused);
            Item { read, ..Default::default() }
        }
    }
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
}

type Analysis = (std::sync::Arc<Features>, std::sync::Arc<Thumb>);
/// An analysis that one file is making and its twins may wait for.
type Made = std::sync::Arc<std::sync::OnceLock<Analysis>>;

impl Planes {
    /// The analysis of `g`: one already made from the same plane, or `make`'s.
    /// A twin that arrives while the first is still being described waits for
    /// it rather than describing it again; `make` takes no lock and no part of
    /// the decode budget, so nothing it waits on can be waiting on it.
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
