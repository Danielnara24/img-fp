//! img-fp — find images that are the same picture.
//!
//! See `README.md` for what it does and `benchmark/BASELINE.md` for what it is
//! competing with. The short version of the design:
//!
//! ```text
//!   walk  ->  exact hash  ->  decode + local features  ->  vocabulary
//!                                                              |
//!            groups  <-  propagate  <-  verify  <-  inverted-file candidates
//! ```
//!
//! The claim a run makes is a *pair*: two files that are the same picture,
//! each backed by a geometric transform that was checked against the pixels.
//! A group is a *representative and everything that matched it* — so every
//! file in a group was checked against the file at its head, and a group
//! asserts only pairs the run really made. See `group.rs` for why that is
//! neither the transitive closure nor a clique, and why groups overlap.

mod cache;
mod decode;
mod group;
mod index;
mod prof;
mod sift;
mod verify;

use anyhow::Result;
use clap::Parser;
use index::{InvertedFile, Vocabulary, WordList};
use rayon::prelude::*;
use serde::Serialize;
use sift::{Features, DESC_LEN};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use verify::{Affine, Thumb, Variant, Verdict};

#[derive(Parser, Debug)]
#[command(name = "img-fp", about = "Find duplicate and near-duplicate images.", version)]
struct Args {
    /// Directories or files to scan.
    #[arg(required = true)]
    roots: Vec<PathBuf>,

    /// Write JSON results here instead of a summary on stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Worker threads (default: all cores).
    #[arg(short = 't', long, default_value_t = 0)]
    threads: usize,

    /// Long side the analysis runs at. Lower is faster and blinder.
    #[arg(long, default_value_t = 640)]
    work_size: usize,

    /// Candidates verified per image.
    #[arg(short = 'k', long, default_value_t = 150)]
    candidates: usize,

    /// Keypoint correspondences that must agree on one transform before a
    /// pair can be claimed — the inlier count, under a name that does not
    /// need RANSAC to read.
    ///
    /// One number, used by every tier. It was two: the claim-anchoring tier
    /// silently added 2, which is the sort of offset that looks principled and
    /// is really just a corpus talking.
    #[arg(long, default_value_t = 10)]
    min_aligned_points: u32,

    /// How much of one image's frame must lie inside the other, 0..1.
    ///
    /// Geometry alone: what the fitted transform claims, with no pixel read.
    /// Whether the claim is true is `--min-pixel-correlation`, which is a
    /// different question and not a tighter version of this one.
    #[arg(long, default_value_t = 0.85)]
    min_frame_overlap: f32,

    /// How well the pixels of that overlap must correlate, 0..1.
    ///
    /// The mean of |r| over the blocks of the overlap that carry detail. Half
    /// is the midpoint of what the statistic can report, not a value read off
    /// a corpus; the measured cliff, where wrong families start merging, is at
    /// 0.40.
    #[arg(long, default_value_t = 0.5)]
    min_pixel_correlation: f32,

    /// Write every verdict considered, accepted or not, to this CSV. For
    /// tuning the decision rule against a labelled corpus.
    #[arg(long)]
    dump: Option<PathBuf>,

    /// Reuse and update a cache of the per-image analysis.
    #[arg(long)]
    cache: Option<PathBuf>,

    /// Print timings per stage.
    #[arg(short, long)]
    verbose: bool,
}

// ---------------------------------------------------------------- walking

fn walk(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            files.push(root.clone());
            continue;
        }
        for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.into_path();
            if decode::looks_like_image(&p) {
                files.push(p);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

// ---------------------------------------------------------------- exact pass

/// FNV-1a over the file's bytes. Only files sharing a size are read, so on a
/// normal corpus this touches almost nothing.
fn content_hash(path: &Path) -> Option<u128> {
    let data = std::fs::read(path).ok()?;
    let mut h: u128 = 0x6c62272e07bb0142_62b821756295c58d;
    for chunk in data.chunks(8) {
        let mut v = 0u64;
        for (i, &b) in chunk.iter().enumerate() {
            v |= (b as u64) << (i * 8);
        }
        h ^= v as u128;
        h = h.wrapping_mul(0x0000000001000000_000000000000013B);
    }
    h ^= data.len() as u128;
    Some(h)
}

fn exact_groups(files: &[PathBuf]) -> Vec<Vec<usize>> {
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if let Ok(md) = std::fs::metadata(f) {
            by_size.entry(md.len()).or_default().push(i);
        }
    }
    let candidates: Vec<Vec<usize>> = by_size.into_values().filter(|v| v.len() > 1).collect();
    let hashed: Vec<(u128, usize)> = candidates
        .par_iter()
        .flat_map(|group| {
            group
                .par_iter()
                .filter_map(|&i| content_hash(&files[i]).map(|h| (h, i)))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut by_hash: HashMap<u128, Vec<usize>> = HashMap::new();
    for (h, i) in hashed {
        by_hash.entry(h).or_default().push(i);
    }
    let mut out: Vec<Vec<usize>> = by_hash.into_values().filter(|v| v.len() > 1).collect();
    for g in out.iter_mut() {
        g.sort_unstable();
    }
    out.sort();
    out
}

// ---------------------------------------------------------------- per image

/// One image's analysis.
///
/// The features and the thumbnail are shared rather than owned, because
/// byte-identical files share an analysis: the exact pass has already found
/// them, one member of each group is described and the rest point at it.
/// Copying instead meant a second megabyte-scale buffer per duplicate — three
/// hundred of them on this corpus — for bytes that are the same bytes.
#[derive(Default)]
struct Item {
    feats: std::sync::Arc<Features>,
    thumb: std::sync::Arc<Thumb>,
    ok: bool,
    err: Option<String>,
}

const THUMB_LONG: usize = 128;

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
const FEATURES: usize = 600;

/// A ceiling, not a setting. Each round re-routes composed transforms through
/// the pairs the last one accepted, and the loop stops as soon as a round adds
/// nothing — which on every corpus tried has been the second or third. The
/// constant only exists so a pathological graph cannot spin forever.
const PROPAGATE_MAX_ROUNDS: usize = 8;

static T_DECODE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static T_SIFT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn analyse(path: &Path, work: usize, p: &sift::Params) -> Item {
    let t0 = Instant::now();
    let r = decode::decode(path, work);
    T_DECODE.fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
    match r {
        Ok(d) => {
            let t1 = Instant::now();
            let feats = sift::extract(&d.work, p);
            T_SIFT.fetch_add(t1.elapsed().as_micros() as u64, Ordering::Relaxed);
            let thumb = timed!(4, Thumb::build(&d.work, THUMB_LONG));
            Item { feats: feats.into(), thumb: thumb.into(), ok: true, err: None }
        }
        Err(e) => Item { err: Some(e.to_string()), ..Default::default() },
    }
}

/// Quantise every descriptor of an image into the sorted word list the
/// inverted file speaks.
fn quantise(vocab: &Vocabulary, f: &Features) -> WordList {
    let mut buf = Vec::with_capacity(4);
    let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(f.len() * 2);
    for i in 0..f.len() {
        vocab.quantise(f.d(i), &mut buf);
        for &w in buf.iter() {
            pairs.push((w, i as u32));
        }
    }
    pairs.sort_unstable();
    WordList { word: pairs.iter().map(|p| p.0).collect(), kp: pairs.iter().map(|p| p.1).collect() }
}

/// The same features as seen in a mirrored (and/or inverted) copy of the
/// image: descriptor bins permuted, keypoints moved. Building this costs a
/// permutation rather than a second extraction pass.
fn variant_features(f: &Features, var: Variant) -> Features {
    let mut out = Features { w: f.w, h: f.h, kps: f.kps.clone(), desc: vec![0u8; f.desc.len()] };
    let mut perm: [u8; DESC_LEN] = std::array::from_fn(|i| i as u8);
    if var.mirror {
        perm = compose_perm(&perm, &sift::mirror_perm());
    }
    if var.invert {
        perm = compose_perm(&perm, &sift::invert_perm());
    }
    for i in 0..f.len() {
        sift::permute(f.d(i), &perm, &mut out.desc[i * DESC_LEN..(i + 1) * DESC_LEN]);
    }
    let w = f.w as f32;
    for k in out.kps.iter_mut() {
        if var.mirror {
            k.x = w - 1.0 - k.x;
            k.angle = (180.0 - k.angle).rem_euclid(360.0);
        }
        if var.invert {
            k.angle = (k.angle + 180.0).rem_euclid(360.0);
        }
    }
    out
}

fn compose_perm(a: &[u8; DESC_LEN], b: &[u8; DESC_LEN]) -> [u8; DESC_LEN] {
    std::array::from_fn(|i| a[b[i] as usize])
}

// ---------------------------------------------------------------- union-find

struct Dsu(Vec<usize>);
impl Dsu {
    fn new(n: usize) -> Dsu {
        Dsu((0..n).collect())
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a] = b;
        }
    }
}

// ---------------------------------------------------------------- output

#[derive(Serialize)]
struct OutPair {
    a: String,
    b: String,
    aligned_points: u32,
    frame_overlap: f32,
    pixel_correlation: f32,
    scale: f32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    inverted: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    identical: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    propagated: bool,
}

/// One group: the file everything in it was verified against, and the files.
///
/// `files` includes the representative, and is the key `score.py` and the
/// other consumers read, so the group is still a plain list of paths to
/// anything that does not care which one is the head.
#[derive(Serialize)]
struct OutGroup {
    representative: String,
    files: Vec<String>,
}

#[derive(Serialize)]
struct Output {
    tool: &'static str,
    config: serde_json::Value,
    files_enumerated: usize,
    files_analysed: usize,
    failures: Vec<serde_json::Value>,
    runtime_seconds: f64,
    /// Largest first. Each is a representative and the files that matched it
    /// directly. These overlap: a file that is a duplicate of two files that
    /// are not duplicates of each other appears under both.
    groups: Vec<OutGroup>,
    pairs: Vec<OutPair>,
}

// ---------------------------------------------------------------- main

fn main() -> Result<()> {
    let args = Args::parse();
    let t_start = Instant::now();
    few_arenas();
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new().num_threads(args.threads).build_global()?;
    }
    let verbose = args.verbose;
    macro_rules! stage {
        ($t:expr, $($arg:tt)*) => {
            if verbose { eprintln!("[{:6.1}s] {}", $t.elapsed().as_secs_f64(), format!($($arg)*)); }
        };
    }

    let files = walk(&args.roots);
    stage!(t_start, "{} files", files.len());
    if files.is_empty() {
        eprintln!("no image files found");
        return Ok(());
    }

    // Byte-identical copies, before anything is decoded.
    let exact = exact_groups(&files);
    stage!(t_start, "exact duplicates: {} groups", exact.len());

    // Decode and describe.
    let sp = sift::Params {
        max_features: FEATURES,
        ..Default::default()
    };
    let done = AtomicUsize::new(0);
    let n = files.len();
    let settings = cache::Settings {
        work_size: args.work_size as u32,
        features: FEATURES as u32,
        thumb: THUMB_LONG as u32,
    };
    let cached = match &args.cache {
        Some(p) => cache::load(p, settings),
        None => Default::default(),
    };
    if verbose && !cached.is_empty() {
        eprintln!("[{:6.1}s] cache: {} usable records", t_start.elapsed().as_secs_f64(), cached.len());
    }
    // The exact pass has already grouped the files whose bytes hash the same,
    // and the analysis depends on nothing but those bytes. Describing the
    // second copy of a file is not a cheaper way to reach the same answer, it
    // is the same work done twice: each group elects its first member and the
    // rest are copied from it. Those groups are already claimed as duplicates
    // in the output, so sharing one analysis between them asserts nothing the
    // run does not assert anyway.
    let mut twin_of: Vec<usize> = (0..n).collect();
    for g in exact.iter() {
        for &i in g[1..].iter() {
            twin_of[i] = g[0];
        }
    }
    let mut items: Vec<Item> = files
        .par_iter()
        .enumerate()
        .map(|(i, f)| {
            if twin_of[i] != i {
                return Item::default();
            }
            let key = cache::key_of(f);
            if let (Some(k), Some((ck, rec))) = (&key, cached.get(&f.display().to_string())) {
                if ck.len == k.len && ck.mtime == k.mtime {
                    return Item {
                        feats: rec.feats.clone().into(),
                        thumb: rec.thumb.clone().into(),
                        ok: true,
                        err: None,
                    };
                }
            }
            let it = analyse(f, args.work_size, &sp);
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if verbose && d % 250 == 0 {
                eprintln!("[{:6.1}s]   described {d}", t_start.elapsed().as_secs_f64());
            }
            it
        })
        .collect();
    for i in 0..n {
        let r = twin_of[i];
        if r == i {
            continue;
        }
        // A copy whose original could not be read is described on its own, so
        // that the failure is reported against the path that failed.
        items[i] = if items[r].ok {
            Item { feats: items[r].feats.clone(), thumb: items[r].thumb.clone(), ok: true, err: None }
        } else {
            analyse(&files[i], args.work_size, &sp)
        };
    }
    // Every usable record has been copied into `items` by now, and the map
    // holds a second copy of the analysis of every file that was cached.
    drop(cached);
    if let Some(p) = &args.cache {
        let names: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
        let entries: Vec<(&str, cache::Key, &Features, &Thumb)> = (0..n)
            .filter(|&i| items[i].ok)
            .filter_map(|i| cache::key_of(&files[i]).map(|k| (names[i].as_str(), k, &*items[i].feats, &*items[i].thumb)))
            .collect();
        if let Err(e) = cache::save(p, settings, &entries) {
            eprintln!("warning: could not write cache: {e}");
        }
    }
    if verbose {
        eprintln!(
            "[{:6.1}s] cpu: decode {:.0}s, features {:.0}s",
            t_start.elapsed().as_secs_f64(),
            T_DECODE.load(Ordering::Relaxed) as f64 / 1e6,
            T_SIFT.load(Ordering::Relaxed) as f64 / 1e6
        );
    }
    // Nothing after this point extracts features, so the workers' blur scratch
    // is dead weight from here on.
    rayon::broadcast(|_| sift::release_scratch());
    sift::release_scratch();
    // The analysis phase churns through buffers far larger than anything that
    // follows — a full-resolution decode, then a scale space per worker — and
    // the allocator keeps those pages against a demand that never comes,
    // because it has watched this process ask for them over and over. Handing
    // them back is worth several hundred megabytes for the rest of the run.
    // (This was measured once before and judged worthless, and it was: the
    // peak then stood in the middle of this very phase. With the decode
    // budget holding that down, what is left is the plateau this releases.)
    release_memory();
    let n_ok = items.iter().filter(|i| i.ok).count();
    let n_desc: usize = items.iter().map(|i| i.feats.len()).sum();
    stage!(t_start, "described {n_ok}/{n} images, {n_desc} descriptors");

    // Vocabulary from the corpus itself, at a depth the corpus chooses.
    let vp = index::VocabParams::for_corpus(n_desc);
    let mut pool: Vec<u8> = Vec::with_capacity(vp.sample.min(n_desc) * DESC_LEN);
    {
        // Even sampling across images, so one feature-rich image cannot own
        // the vocabulary.
        let per = (vp.sample / n_ok.max(1)).max(8);
        for it in items.iter() {
            let take = it.feats.len().min(per);
            let step = (it.feats.len() / take.max(1)).max(1);
            for i in (0..it.feats.len()).step_by(step).take(take) {
                pool.extend_from_slice(it.feats.d(i));
            }
        }
    }
    let vocab = timed!(12, Vocabulary::build(&pool, &vp));
    drop(pool);
    stage!(t_start, "vocabulary: {} words from {} samples", vocab.n_words(), vp.sample.min(n_desc));

    // Quantise. The word lists are built straight into the vector the inverted
    // file and every later stage read from: holding a second copy per image
    // costs as much again as the lists themselves.
    let lists: Vec<WordList> = items
        .par_iter()
        .map(|it| if it.ok { timed!(11, quantise(&vocab, &it.feats)) } else { WordList::default() })
        .collect();
    stage!(t_start, "quantised");

    // Inverted file. A word present in a fifth of the corpus says nothing.
    let max_posting = (n_ok / 5).max(32);
    let inv = timed!(13, InvertedFile::build(&lists, vocab.n_words(), max_posting));
    stage!(t_start, "inverted file");

    let policy = verify::Policy::new(args.min_aligned_points, args.min_frame_overlap, args.min_pixel_correlation);


    // ---- candidates, then verification
    //
    // Retrieval and verification are separate passes so that a pair proposed
    // from both sides is verified once. Containment scoring is asymmetric —
    // the crop finds the photograph much more readily than the reverse — so
    // both directions genuinely have to be asked.
    type Edge = (usize, usize, Affine, bool, Verdict);
    let mut cand_pairs: Vec<(u32, u32)> = (0..n)
        .into_par_iter()
        .filter(|&i| items[i].ok)
        .map_init(
            || (vec![0f32; n], Vec::new(), Vec::new()),
            |(acc, touched, scored), i| {
                timed!(14, inv.query(&lists[i], i as u32, acc, touched, scored));
                scored.retain(|&(j, s)| items[j as usize].ok && s > 0.0);
                scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
                scored.truncate(args.candidates);
                scored
                    .iter()
                    .map(|&(j, _)| if (j as usize) < i { (j, i as u32) } else { (i as u32, j) })
                    .collect::<Vec<_>>()
            },
        )
        .flatten()
        .collect();
    cand_pairs.par_sort_unstable();
    cand_pairs.dedup();
    stage!(t_start, "candidates: {} pairs", cand_pairs.len());

    let dumping = args.dump.is_some();
    // What a verdict must already have before its pixels are worth reading:
    // the weakest bar any tier applies, or everything a dump would record.
    let gate = if dumping {
        (3, 0.2)
    } else {
        (policy.corroborated.min_aligned_points, policy.corroborated.min_frame_overlap)
    };
    let all_direct: Vec<Edge> = cand_pairs
        .par_iter()
        .map_init(
            || (Vec::new(), Vec::new(), verify::Scratch::default()),
            |(cands, matches, scratch), &(i, j)| {
                let (i, j) = (i as usize, j as usize);
                timed!(15, index::shared(&lists[i], &lists[j], cands, 60_000));
                if cands.len() < 3 {
                    return None;
                }
                let p = verify::Pair {
                    fa: &items[i].feats,
                    fb: &items[j].feats,
                    ta: &items[i].thumb,
                    tb: &items[j].thumb,
                };
                let v = verify::verify(&p, cands, Variant::default(), gate, matches, scratch);
                (v.accepted(&policy.corroborated) || (dumping && v.n_in >= 3)).then_some((i, j, v.m, false, v))
            },
        )
        .flatten()
        .collect();
    // Anchors only: a pair believed on its own evidence. These are what decide
    // which files end up in one cluster.
    let edges: Vec<Edge> = all_direct.iter().filter(|(_, _, _, _, v)| v.accepted(&policy.anchor)).cloned().collect();
    stage!(t_start, "anchors: {} of {} verified pairs", edges.len(), all_direct.len());

    // ---- second look at images nothing matched: mirrored and inverted
    // A mirrored or inverted copy shares no visual words with its original —
    // the descriptor bins are permuted — so retrieval cannot find it, and the
    // query has to be re-asked with the permutation applied. Doing that for
    // every image would roughly double the retrieval cost for a handful of
    // pairs, so it is asked only where the first pass came up short: a file
    // with no matches, or with so few that it may be hanging off the edge of
    // its real cluster.
    let mut degree = vec![0u32; n];
    for &(a, b, _, _, _) in edges.iter() {
        degree[a] += 1;
        degree[b] += 1;
    }
    for g in exact.iter() {
        for &i in g {
            degree[i] += g.len() as u32 - 1;
        }
    }
    // Ask again, mirrored and inverted, for images that no *pair* of matches
    // has anchored yet. A file with one match is not safely found: that match
    // may be the wrong one, and a mirrored query is cheap next to being wrong.
    // Two is not a fitted threshold, it is "more than one" — the same minimal
    // constant the bridge test uses for "more than one file", and the only
    // place a count appears in either rule. Trying the tighter reading, "no
    // matches at all", costs a seed apiece on three of the held-out mirror
    // transforms.
    let lonely: Vec<usize> = (0..n).filter(|&i| items[i].ok && degree[i] < 2).collect();
    let variant_edges: Vec<Edge> = lonely
        .par_iter()
        .map_init(
            || (vec![0f32; n], Vec::new(), Vec::new(), Vec::new(), Vec::new(), verify::Scratch::default()),
            |(acc, touched, scored, cands, matches, scratch), &i| {
                let mut out: Vec<Edge> = Vec::new();
                for var in [
                    Variant { mirror: true, invert: false },
                    Variant { mirror: false, invert: true },
                    Variant { mirror: true, invert: true },
                ] {
                    let vf = variant_features(&items[i].feats, var);
                    let wl = quantise(&vocab, &vf);
                    inv.query(&wl, i as u32, acc, touched, scored);
                    scored.retain(|&(j, s)| items[j as usize].ok && s > 0.0);
                    scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
                    scored.truncate(args.candidates);
                    for &(j, _) in scored.iter() {
                        let j = j as usize;
                        index::shared(&wl, &lists[j], cands, 60_000);
                        if cands.len() < 3 {
                            continue;
                        }
                        let p = verify::Pair {
                            fa: &vf,
                            fb: &items[j].feats,
                            ta: &items[i].thumb,
                            tb: &items[j].thumb,
                        };
                        let v = verify::verify(&p, cands, var, gate, matches, scratch);
                        if v.accepted(&policy.anchor) {
                            let (lo, hi, mm) = if i < j {
                                (i, j, v.m)
                            } else {
                                match verify::invert_affine(&v.m) {
                                    Some(mi) => (j, i, mi),
                                    None => continue,
                                }
                            };
                            out.push((lo, hi, mm, var.invert, v));
                        }
                    }
                }
                out
            },
        )
        .flatten()
        .collect();
    stage!(t_start, "variants: {} more pairs from {} unmatched images", variant_edges.len(), lonely.len());

    // Nothing after this point matches a descriptor against another. What is
    // left — propagation, corroboration, assembly — works from thumbnails,
    // frame sizes and verdicts already taken, so the vocabulary, the inverted
    // file, the word lists and every descriptor in the corpus are dead weight
    // from here. Together they are the largest thing the run holds, and they
    // were being held to the end.
    drop(inv);
    drop(vocab);
    drop(lists);
    for it in items.iter_mut() {
        // The frame size stays: propagation and corroboration still ask how
        // large each picture was. Only the keypoints and descriptors go.
        it.feats = std::sync::Arc::new(Features { w: it.feats.w, h: it.feats.h, ..Default::default() });
    }
    release_memory();
    stage!(t_start, "released the index and the descriptors");

    let mut all: Vec<Edge> = edges;
    all.extend(variant_edges.iter().cloned());

    // Every anchor faces the bridge test, whichever pass produced it: a
    // mirrored match joining two clusters is exactly as consequential as a
    // direct one, and on this corpus an inverted match between a 225-pixel
    // photograph of the Earth and a beach scene was the single edge that
    // merged two whole families into 3,002 false pairs.
    let n_before = all.len();
    let all = drop_weak_bridges(all, n);
    stage!(t_start, "bridges: dropped {} lone links between clusters", n_before - all.len());

    // ---- propagate transforms inside each component
    // Propagation is iterated: each round's accepted pairs become edges the
    // next round's tree can route through, which shortens the path to files
    // that were only reachable the long way round. It converges in two or
    // three rounds.
    let mut propagated: Vec<Edge> = Vec::new();
    // Every hypothesis a round considered, kept only for `--dump`. A round
    // proposes every unmatched pair inside each component, which is quadratic
    // in the component and an order of magnitude more than it accepts, so the
    // rejected ones are dropped as soon as they have been judged unless
    // something is going to read them.
    let mut all_propagated: Vec<Edge> = Vec::new();
    let mut n_hypotheses = 0usize;
    // A composed pair is kept only if it clears the propagated tier's own
    // overlap floor; a dump wants every hypothesis the round considered.
    let prop_min_ov = if dumping { 0.2 } else { policy.propagated.min_frame_overlap };
    let mut pool: Vec<Edge> = all.clone();
    for round in 0..PROPAGATE_MAX_ROUNDS {
        let mut round_all = timed!(21, propagate(&items, &pool, n, prop_min_ov));
        let before = propagated.len();
        let seen: std::collections::HashSet<(usize, usize)> =
            pool.iter().map(|&(a, b, _, _, _)| (a, b)).collect();
        let fresh: Vec<Edge> = round_all
            .iter()
            .filter(|(a, b, _, _, v)| !seen.contains(&(*a, *b)) && v.accepted(&policy.propagated))
            .cloned()
            .collect();
        n_hypotheses += round_all.len();
        if dumping {
            all_propagated.append(&mut round_all);
        } else {
            drop(round_all);
        }
        propagated.extend(fresh.iter().cloned());
        pool.extend(fresh);
        stage!(t_start, "  propagation round {}: +{} pairs", round + 1, propagated.len() - before);
        if propagated.len() == before {
            break;
        }
    }

    stage!(t_start, "propagated: {} of {} composed hypotheses", propagated.len(), n_hypotheses);

    // Weaker matches, admitted only between files an anchor already put in the
    // same cluster. They cannot merge anything, so they cost recall to refuse
    // and risk nothing to accept.
    let corroborated: Vec<Edge> = {
        let mut dsu = Dsu::new(n);
        for (a, b, _, _, _) in all.iter().chain(propagated.iter()) {
            dsu.union(*a, *b);
        }
        let anchored: std::collections::HashSet<(usize, usize)> =
            all.iter().chain(propagated.iter()).map(|&(a, b, _, _, _)| (a, b)).collect();
        all_direct
            .iter()
            .filter(|(a, b, _, _, v)| {
                !anchored.contains(&(*a, *b))
                    && v.accepted(&policy.corroborated)
                    && dsu.find(*a) == dsu.find(*b)
            })
            .cloned()
            .collect()
    };
    stage!(t_start, "corroborated: {} more pairs inside existing clusters", corroborated.len());

    if let Some(path) = &args.dump {
        let f = std::fs::File::create(path)?;
        let mut w = std::io::BufWriter::new(f);
        writeln!(w, "a,b,kind,n_match,n_in,ov_a,ov_b,scale,rot,blk,blk_n,ncc,centred,inverted")?;
        for (kind, set) in [("direct", &all_direct), ("variant", &variant_edges), ("propagated", &all_propagated)] {
            for (a, b, _, iv, v) in set.iter() {
                writeln!(
                    w,
                    "{:?},{:?},{},{},{},{:.4},{:.4},{:.5},{:.1},{:.4},{},{:.4},{},{}",
                    files[*a].display().to_string(),
                    files[*b].display().to_string(),
                    kind, v.n_match, v.n_in, v.ov_a, v.ov_b, v.scale, v.rot_deg, v.blk, v.blk_n, v.ncc, v.centred as u8, *iv as u8
                )?;
            }
        }
        w.flush()?;
        eprintln!("dumped verdicts -> {}", path.display());
    }

    // ---- assemble
    //
    // The pairs are the claim. `graph` collects every one of them, and
    // `group::find` reduces it to representatives — a representative plus the
    // files that matched it directly, which is neither the closure nor the
    // maximal cliques; both were measured and both are worse. So a group
    // repeats claims this run actually made rather than implying new ones,
    // and it is not an all-pairs assertion either: two members that both
    // matched the representative were never compared with each other. The
    // argument and the numbers are in `group.rs`.
    let mut out_pairs: Vec<OutPair> = Vec::new();
    let mut graph: Vec<(usize, usize)> = Vec::new();
    let mut seen: std::collections::HashSet<(usize, usize)> = Default::default();
    for g in exact.iter() {
        for w in 0..g.len() {
            for x in w + 1..g.len() {
                let (a, b) = (g[w], g[x]);
                if seen.insert((a, b)) {
                    graph.push((a, b));
                    out_pairs.push(OutPair {
                        a: files[a].display().to_string(),
                        b: files[b].display().to_string(),
                        aligned_points: 0,
                        frame_overlap: 1.0,
                        pixel_correlation: 1.0,
                        scale: 1.0,
                        inverted: false,
                        identical: true,
                        propagated: false,
                    });
                }
            }
        }
    }
    for (a, b, _, inv_flag, v) in all.iter().chain(propagated.iter()).chain(corroborated.iter()) {
        let (a, b) = (*a, *b);
        if !seen.insert((a, b)) {
            continue;
        }
        graph.push((a, b));
        out_pairs.push(OutPair {
            a: files[a].display().to_string(),
            b: files[b].display().to_string(),
            aligned_points: v.n_in,
            frame_overlap: round3(v.ov_a.max(v.ov_b)),
            pixel_correlation: round3(v.blk),
            scale: round3(v.scale),
            inverted: *inv_flag,
            identical: false,
            propagated: v.n_match == 0,
        });
    }
    out_pairs.sort_by(|x, y| x.a.cmp(&y.a).then(x.b.cmp(&y.b)));

    let grouping = timed!(28, group::find(n, &graph));
    let membership = group::membership(&grouping);
    stage!(
        t_start,
        "groups: {} representatives over {} files, {} of them in more than one group",
        grouping.len(),
        membership.len(),
        membership.values().filter(|gs| gs.len() > 1).count()
    );
    // Largest first, ties by comparing member paths in order. Paths are
    // unique and no two representatives can hold the same members — the
    // second would have had nothing left to account for — so the order is
    // total and the output is reproducible.
    let mut groups: Vec<OutGroup> = grouping
        .iter()
        .map(|g| OutGroup {
            representative: files[g.representative].display().to_string(),
            files: g.members.iter().map(|&i| files[i].display().to_string()).collect(),
        })
        .collect();
    groups.sort_by(|a, b| {
        b.files.len().cmp(&a.files.len()).then_with(|| a.files.cmp(&b.files))
    });

    let failures: Vec<serde_json::Value> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            it.err.as_ref().map(|e| serde_json::json!({"path": files[i].display().to_string(), "error": e}))
        })
        .collect();

    let runtime = t_start.elapsed().as_secs_f64();
    let out = Output {
        tool: "img-fp",
        config: serde_json::json!({
            "work_size": args.work_size,
            "features": FEATURES,
            "candidates": args.candidates,
            "min_aligned_points": args.min_aligned_points,
            "min_frame_overlap": args.min_frame_overlap,
            "min_pixel_correlation": args.min_pixel_correlation,
            "stages": "anchor, propagate, corroborate",
            }),
        files_enumerated: files.len(),
        files_analysed: n_ok,
        failures,
        runtime_seconds: (runtime * 1000.0).round() / 1000.0,
        groups,
        pairs: out_pairs,
    };

    match args.output {
        Some(path) => {
            let f = std::fs::File::create(&path)?;
            serde_json::to_writer(std::io::BufWriter::new(f), &out)?;
            eprintln!(
                "{} groups, {} pairs over {} images in {:.1}s -> {}",
                out.groups.len(),
                out.pairs.len(),
                n_ok,
                runtime,
                path.display()
            );
        }
        None => {
            let stdout = std::io::stdout();
            let mut w = std::io::BufWriter::new(stdout.lock());
            // Representative first in each block: it is the file the rest
            // were compared against, and the one to keep.
            for g in out.groups.iter() {
                writeln!(w, "{}", g.representative)?;
                for f in g.files.iter().filter(|f| **f != g.representative) {
                    writeln!(w, "{f}")?;
                }
                writeln!(w)?;
            }
            w.flush()?;
            eprintln!("{} groups, {} pairs over {} images in {:.1}s", out.groups.len(), out.pairs.len(), n_ok, runtime);
        }
    }
    prof::report();
    Ok(())
}

/// Keep the allocator's arenas few.
///
/// glibc gives a process up to eight arenas per core and lets each hold on to
/// what it has freed. That suits a program allocating small blocks in tight
/// loops on many threads; this one does the opposite, taking a handful of very
/// large buffers per image on one worker per core. Spread over sixty-four
/// arenas, a freed decode buffer or scale space is held apart from the next
/// worker that could have used it, and apart from the descriptors it was
/// interleaved with — some two hundred megabytes of holes on this corpus,
/// measured as the gap between what the run holds and what it is using. A
/// couple of arenas keep the same pages in circulation instead. The small,
/// frequent allocations that might contend for them are served out of each
/// thread's own cache without taking the arena lock at all.
fn few_arenas() {
    #[cfg(target_env = "gnu")]
    {
        const M_ARENA_MAX: i32 = -8;
        unsafe extern "C" {
            fn mallopt(param: i32, value: i32) -> i32;
        }
        unsafe {
            mallopt(M_ARENA_MAX, 2);
        }
    }
}

/// Return free heap pages to the operating system.
///
/// glibc raises its own mmap threshold as a program repeatedly allocates and
/// frees large blocks, so buffers that started as their own mappings — and
/// would have been unmapped on free — end up held in the heap instead. The
/// pages are free, but they are the process's, and `ru_maxrss` counts them.
/// This is called at the two points where the shape of the run changes and a
/// phase's worth of large buffers has just died.
fn release_memory() {
    #[cfg(target_env = "gnu")]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        unsafe {
            malloc_trim(0);
        }
    }
}

fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

/// Remove single matches that are the only thing joining two large clusters.
///
/// A wrong pair does not cost one pair. If it is the *only* edge between two
/// groups of files, everything on one side becomes a claimed duplicate of
/// everything on the other, and propagation will dutifully manufacture the
/// evidence: on this corpus one weak match between a dark galaxy photograph
/// and a beach photograph produced 62 false pairs downstream, and an earlier
/// one produced 354.
///
/// So an edge that is a *bridge* in the graph theory sense — its removal
/// disconnects the component — and which separates two non-trivial groups is
/// held to a much higher standard than an ordinary match, because it is
/// carrying all of them. A bridge to a single isolated file is left alone:
/// that is the ordinary case of a file matched exactly once, and it claims
/// nothing beyond itself.
fn drop_weak_bridges(edges: Vec<(usize, usize, Affine, bool, Verdict)>, n: usize) -> Vec<(usize, usize, Affine, bool, Verdict)> {
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n]; // (neighbour, edge index)
    for (e, (a, b, _, _, _)) in edges.iter().enumerate() {
        adj[*a].push((*b, e));
        adj[*b].push((*a, e));
    }
    // Iterative Tarjan: discovery time, low-link, and the subtree size needed
    // to know how much sits on the far side of a bridge.
    let mut disc = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut size = vec![1usize; n];
    let mut timer = 0usize;
    let mut bridges: Vec<(usize, usize)> = Vec::new(); // (edge index, far-side size)
    let mut stack: Vec<(usize, usize, usize)> = Vec::new(); // (node, parent edge, next neighbour)
    for s in 0..n {
        if disc[s] != usize::MAX || adj[s].is_empty() {
            continue;
        }
        let total = {
            // Component size, for deciding what counts as non-trivial.
            let mut seen = vec![s];
            let mut mark = std::collections::HashSet::from([s]);
            let mut i = 0;
            while i < seen.len() {
                let u = seen[i];
                i += 1;
                for &(v, _) in adj[u].iter() {
                    if mark.insert(v) {
                        seen.push(v);
                    }
                }
            }
            seen.len()
        };
        disc[s] = timer;
        low[s] = timer;
        timer += 1;
        stack.push((s, usize::MAX, 0));
        while let Some(&mut (u, pe, ref mut k)) = stack.last_mut() {
            if *k < adj[u].len() {
                let (v, e) = adj[u][*k];
                *k += 1;
                if e == pe {
                    continue;
                }
                if disc[v] == usize::MAX {
                    disc[v] = timer;
                    low[v] = timer;
                    timer += 1;
                    stack.push((v, e, 0));
                } else {
                    low[u] = low[u].min(disc[v]);
                }
            } else {
                stack.pop();
                if let Some(&mut (p, _, _)) = stack.last_mut() {
                    low[p] = low[p].min(low[u]);
                    size[p] += size[u];
                    if low[u] > disc[p] {
                        bridges.push((pe, size[u].min(total - size[u])));
                    }
                }
            }
        }
    }
    // A bridge whose far side is a *group* rather than a lone file is the sole
    // evidence for every pair the two sides imply, and no single match is
    // worth that much. It is dropped — not held to a higher bar, because
    // "higher" was three more numbers fitted to one corpus (three times the
    // inliers, fifteen points more agreement, under two octaves of gap) and a
    // match strong enough to pass them is still one match. The pairs are not
    // lost if the two sides really are one family: any second link between
    // them stops the edge being a bridge at all.
    let mut drop = vec![false; edges.len()];
    for (e, far) in bridges {
        if e != usize::MAX && far >= 2 {
            drop[e] = true;
        }
    }
    edges.into_iter().enumerate().filter(|(e, _)| !drop[*e]).map(|(_, x)| x).collect()
}

/// Within each connected component, test the pairs direct matching missed by
/// composing transforms along the edges that were found.
///
/// This is not closure expansion. A composed transform is a *hypothesis* about
/// a pair nobody matched, and it is put through the same pixel test as any
/// other, at a stricter threshold because no features vouch for it. When A is
/// a crop of B and B is a crop of C, the A-C transform is known exactly and
/// the only question is whether the pixels agree — which is cheap to answer
/// and wrong to assume.
fn propagate(items: &[Item], edges: &[(usize, usize, Affine, bool, Verdict)], n: usize, min_ov: f32) -> Vec<(usize, usize, Affine, bool, Verdict)> {
    let mut adj: Vec<Vec<(usize, Affine, bool, u32)>> = vec![Vec::new(); n];
    for (a, b, m, inv, v) in edges.iter() {
        adj[*a].push((*b, *m, *inv, v.n_in));
        if let Some(mi) = verify::invert_affine(m) {
            adj[*b].push((*a, mi, *inv, v.n_in));
        }
    }
    let mut dsu = Dsu::new(n);
    for (a, b, _, _, _) in edges.iter() {
        dsu.union(*a, *b);
    }
    let mut comps: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        if !adj[i].is_empty() {
            comps.entry(dsu.find(i)).or_default().push(i);
        }
    }
    let known: std::collections::HashSet<(usize, usize)> =
        edges.iter().map(|&(a, b, _, _, _)| (a, b)).collect();

    let comps: Vec<Vec<usize>> = comps
        .into_values()
        .filter(|c| c.len() > 2 && c.len() <= 2000)
        .map(|mut c| {
            c.sort_unstable();
            c
        })
        .collect();

    comps
        .par_iter()
        .flat_map(|comp| {
            // Pose of every member relative to the component's root.
            //
            // Breadth-first, from the best-connected member. Every hop
            // composes another transform and carries its error into the
            // result, so the thing to minimise is the number of hops, not the
            // quality of each one: growing the tree best-edge-first was tried
            // and is measurably worse, because it trades short paths for
            // slightly better links and ends up composing more of them. The
            // root is the file most others matched, which is usually the
            // original or a clean re-encode of it.
            let root = *comp
                .iter()
                .max_by_key(|&&i| (adj[i].len(), std::cmp::Reverse(i)))
                .unwrap();
            let mut pose: HashMap<usize, (Affine, bool)> = HashMap::new();
            pose.insert(root, ([1.0, 0.0, 0.0, 0.0, 1.0, 0.0], false));
            let mut queue = std::collections::VecDeque::from([root]);
            while let Some(u) = queue.pop_front() {
                let (mu, iu) = pose[&u];
                for &(v, m, inv, _) in adj[u].iter() {
                    if pose.contains_key(&v) {
                        continue;
                    }
                    pose.insert(v, (verify::compose(&mu, &m), iu ^ inv));
                    queue.push_back(v);
                }
            }
            let mut out = Vec::new();
            for (ai, &a) in comp.iter().enumerate() {
                let Some(&(ma, ia)) = pose.get(&a) else { continue };
                let Some(inv_ma) = verify::invert_affine(&ma) else { continue };
                for &b in comp[ai + 1..].iter() {
                    if known.contains(&(a, b)) {
                        continue;
                    }
                    let Some(&(mb, ib)) = pose.get(&b) else { continue };
                    // a -> root -> b
                    let m = verify::compose(&inv_ma, &mb);
                    let var = Variant { mirror: false, invert: ia ^ ib };
                    let p = verify::Pair {
                        fa: &items[a].feats,
                        fb: &items[b].feats,
                        ta: &items[a].thumb,
                        tb: &items[b].thumb,
                    };
                    let v = verify::verify_transform(&p, &m, var, min_ov);
                    if v.ov_a.max(v.ov_b) > 0.5 {
                        out.push((a, b, m, var.invert, v));
                    }
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    /// The vocabulary has to scale with the corpus, and the reason is a bug
    /// that a single-corpus benchmark could never have shown: with a fixed
    /// 65,536 words, a folder of eight images put every descriptor in a word
    /// of its own and img-fp found one pair out of twenty-eight.
    ///
    /// Scaling the *depth* alone fixed that end and left a worse one in the
    /// middle. A tree of `16^depth` leaves can only be sized to within a
    /// factor of sixteen, so the occupancy the rule claims to hold constant
    /// swung from 2 descriptors per word just above a step to 32 just below
    /// one — and the coarse end merges families. It was reachable by being an
    /// ordinary size: 3,965 files landed at 26 descriptors per word and
    /// merged three of them. What the tree is sized by now is the branching
    /// as well, which is why these two assertions are about occupancy rather
    /// than about word counts.
    #[test]
    fn vocabulary_occupancy_holds_at_every_corpus_size() {
        use crate::index::VocabParams;
        let words = |n| {
            let p = VocabParams::for_corpus(n);
            p.branching.pow(p.depth as u32)
        };
        // The benchmark corpus keeps the tree every published number was
        // measured on: 2,404,926 descriptors into 16^5 words.
        assert_eq!(words(2_404_926), 1_048_576);
        // A handful of files still gets a tree small enough that twins share
        // a word, which is the failure the rule was written for.
        assert!(words(3_200) < 2_048);
        // From a folder to a drive, the tree lands near the target instead of
        // wherever the next power of sixteen falls. Three is the target; the
        // band is what the old rule could not hold.
        for n in [1_200usize, 3_200, 36_500, 542_660, 1_340_021, 1_701_218, 2_404_926, 8_000_000] {
            let occupancy = n as f64 / words(n) as f64;
            assert!(occupancy > 1.5 && occupancy <= 3.0, "n={n} gives {occupancy} per word");
        }
        // And the step itself is gone. These two corpora differ by 5% and
        // used to differ by a factor of sixteen in vocabulary size.
        assert!((words(2_100_000) as f64 / words(2_000_000) as f64) < 2.0);
    }

    use super::*;

    /// The mirrored-descriptor permutation must agree with actually mirroring
    /// the image and extracting again. If it does not, mirrored matches are
    /// found by luck rather than by construction.
    #[test]
    fn mirror_permutation_matches_reextraction() {
        let mut g = decode::Gray::new(200, 150);
        for y in 0..150 {
            for x in 0..200 {
                let v = ((x * 7 + y * 13) % 97) as f32 / 96.0;
                g.px[y * 200 + x] = v * 0.6 + ((x * y) % 31) as f32 / 120.0;
            }
        }
        let p = sift::Params::default();
        let f = sift::extract(&g, &p);
        let mut m = decode::Gray::new(g.w, g.h);
        for y in 0..g.h {
            for x in 0..g.w {
                m.px[y * g.w + x] = g.px[y * g.w + (g.w - 1 - x)];
            }
        }
        let fm = sift::extract(&m, &p);
        let vf = variant_features(&f, Variant { mirror: true, invert: false });
        assert_eq!(vf.len(), f.len());
        // Every derived keypoint should sit on a real one in the mirrored
        // extraction, with a close descriptor.
        let mut hits = 0;
        for i in 0..vf.len().min(200) {
            let k = &vf.kps[i];
            let best = (0..fm.len())
                .filter(|&j| (fm.kps[j].x - k.x).abs() < 1.5 && (fm.kps[j].y - k.y).abs() < 1.5)
                .min_by_key(|&j| {
                    vf.d(i).iter().zip(fm.d(j)).map(|(&a, &b)| (a as i32 - b as i32).pow(2)).sum::<i32>()
                });
            if let Some(j) = best {
                let d: i32 = vf.d(i).iter().zip(fm.d(j)).map(|(&a, &b)| (a as i32 - b as i32).pow(2)).sum();
                if d < 60_000 {
                    hits += 1;
                }
            }
        }
        assert!(hits * 10 >= vf.len().min(200) * 7, "only {hits} of {} mirrored descriptors agreed", vf.len().min(200));
    }

    /// Composing a transform with its own inverse must return the identity,
    /// because every propagated hypothesis is built that way.
    #[test]
    fn affine_round_trip() {
        let m: Affine = [0.8, -0.3, 12.0, 0.25, 0.9, -4.0];
        let mi = verify::invert_affine(&m).unwrap();
        let id = verify::compose(&m, &mi);
        for (a, b) in id.iter().zip([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]) {
            assert!((a - b).abs() < 1e-4, "{id:?}");
        }
    }
}
