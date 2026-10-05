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
//!
//! This is a library so that two binaries can share it: `img-fp`, the command
//! line (`cli_main`), and `img-fp-gui`, which runs the very same scan in a
//! child process of its own (`worker_main`) — see `src/gui/scan.rs` for why a
//! process and not a thread.

// Unix only, and said once here rather than pretended in a few places. A path
// is its bytes throughout — in the cache, the reports, the dump and the path
// lists — the cache is keyed on change times and the walk on inodes, and the
// interrupt ends the process with `_exit`. There used to be `cfg(not(unix))`
// branches in the walk and the interrupt as if another platform were a few
// lines away, beside a dozen unconditional `std::os::unix` uses that meant no
// such build could ever compile.
#[cfg(not(unix))]
compile_error!("img-fp builds for Unix-like systems only (Linux, the BSDs, macOS)");

mod cache;
mod decode;
mod extensions;
mod group;
mod index;
mod problems;
mod prof;
mod progress;
use progress::Stage;
mod report;
mod sift;
mod simd;
mod verify;
mod walk;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use index::{InvertedFile, Vocabulary, WordList};
use problems::{Log, Problems};
use rayon::prelude::*;
use sift::{Features, DESC_LEN};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use verify::{Affine, Thumb, Variant, Verdict};

#[derive(Parser, Debug)]
#[command(name = "img-fp", about = "Find duplicate and near-duplicate images.", version)]
// The doc comments below are the `--help` text and the man page, so they say
// what a flag does and nothing about how its default was chosen. That is in
// CLAUDE.md, which the binary cannot point to.
struct Args {
    /// Folders or image files to scan. `-` reads a list of paths from stdin.
    ///
    /// A file named `-` is `./-`.
    #[arg(required_unless_present_any = ["completions", "man", "from_file"], value_name = "PATH")]
    roots: Vec<PathBuf>,

    /// Read the paths to scan from a file, one per line (`-` = stdin).
    ///
    /// Entries may be folders or files and combine with any paths given as
    /// arguments. Blank lines are ignored.
    #[arg(long = "from-file", value_name = "FILE")]
    from_file: Option<PathBuf>,

    /// Paths in the list are NUL-separated, for `find -print0` or `fd -0`.
    #[arg(short = '0', long = "null")]
    null: bool,

    /// Include subfolders.
    #[arg(short, long)]
    recursive: bool,

    /// Leave out a folder or file. Repeat for several.
    ///
    /// A file reached through a symlink or a second root is left out too.
    #[arg(short = 'e', long = "exclude", value_name = "PATH")]
    exclude: Vec<PathBuf>,

    /// Follow symlinks met while walking a folder.
    ///
    /// A link named on the command line is followed either way. A file
    /// reached by several names is scanned once.
    #[arg(long)]
    follow_symlinks: bool,

    /// Extensions a folder walk treats as images, comma-separated or repeated.
    ///
    /// `-x '*'` takes every file, including ones with no extension. An entry
    /// starting with `!` is an exception: `-x '!gif'` takes every file but
    /// GIFs. A file named on the command line is scanned whatever its
    /// extension.
    #[arg(
        short = 'x',
        long = "extensions",
        value_delimiter = ',',
        value_name = "EXT",
        default_values_t = decode::EXTENSIONS.map(String::from)
    )]
    extensions: Vec<String>,

    /// Write the report to this file: `.txt`, `.csv` or `.json`. `-` is stdout.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Write the report as txt, csv or json, whatever `--output` is called.
    #[arg(long, value_enum, value_name = "FORMAT")]
    format: Option<report::Format>,

    /// Worker threads. `0` uses all cores.
    #[arg(short = 't', long, value_name = "N", default_value_t = 0)]
    threads: usize,

    /// Long side, in pixels, the images are analysed at. Larger images are
    /// shrunk to it; small ones are enlarged, up to it, so their details can
    /// be found.
    ///
    /// Higher finds more, especially small images inside larger ones such as
    /// slides, screenshots and collages, but is slower and uses more memory.
    /// 640 is a good choice when that matters. `0` does not shrink images at
    /// all, which is much slower on large photos.
    #[arg(long, value_name = "PX", default_value_t = 512)]
    work_size: usize,

    /// Candidate matches checked per image.
    #[arg(short = 'k', long, value_name = "N", default_value_t = 150, value_parser = parse_candidates)]
    candidates: usize,

    /// Matching points two images must share before they count as duplicates.
    ///
    /// Higher is stricter. Below 3 behaves as 3.
    #[arg(long, value_name = "N", default_value_t = 10)]
    min_aligned_points: u32,

    /// How much of one image must lie inside the other, from 0 to 1.
    ///
    /// Higher is stricter.
    #[arg(long, value_name = "F", default_value_t = 0.85, value_parser = parse_fraction)]
    min_frame_overlap: f32,

    /// How closely the pixels of that shared area must agree, from 0 to 1.
    ///
    /// Higher is stricter.
    // 0.6 is set for what a user wants grouped rather than for F1; see
    // CLAUDE.md for the sweep and the cliff.
    #[arg(long, value_name = "F", default_value_t = 0.6, value_parser = parse_fraction)]
    min_pixel_correlation: f32,

    /// Write every candidate pair considered, accepted or not, to this CSV.
    /// `-` is stdout.
    #[arg(long, value_name = "FILE")]
    dump: Option<PathBuf>,

    /// Use this cache file instead of the default one.
    ///
    /// The default is `$XDG_CACHE_HOME/img-fp/analysis.bin`, or
    /// `~/.cache/img-fp/analysis.bin`. A folder gets the default file name
    /// inside it.
    // Allowed beside `--no-cache` only to say which file `--clear-cache`
    // deletes; see `validate`.
    #[arg(long, value_name = "PATH")]
    cache: Option<PathBuf>,

    /// Don't read or write the cache.
    #[arg(long, conflicts_with = "prune_cache")]
    no_cache: bool,

    /// Delete the cache before running.
    ///
    /// With `--no-cache`, delete it and don't write a new one. `--cache`
    /// says which file, as it does without `--no-cache`.
    #[arg(long)]
    clear_cache: bool,

    /// Drop cached entries this scan did not use: images it did not find, and
    /// analyses made at another `--work-size`.
    ///
    /// Skipped when the scan could not read everything it was given.
    #[arg(long)]
    prune_cache: bool,

    /// Print timings for each stage.
    #[arg(short, long)]
    verbose: bool,

    /// Write every skipped file, problem and stage timing to this file.
    ///
    /// Truncated at the start of each run. `-` is stdout.
    #[arg(long, value_name = "PATH")]
    log_file: Option<PathBuf>,

    /// Print a completion script for bash, zsh, fish, elvish or powershell and exit.
    #[arg(long = "completions", value_name = "SHELL", exclusive = true)]
    completions: Option<Shell>,

    /// Print the man page (roff) and exit.
    #[arg(long = "man", exclusive = true)]
    man: bool,
}

// ---------------------------------------------------------------- arguments
//
// The thresholds are refused outside the range they are measured on. An
// overlap of 7, a correlation of NaN or no candidates at all is accepted by
// the types and finds nothing, and a run that finds nothing and exits 0 reads
// as a folder with no duplicates in it.
//
// `--work-size` has no floor, deliberately. Swept from 1 to 384 on both
// corpora, a working image under 8 pixels a side describes to no features at
// all, and the run says so — exit 2, every file listed as featureless — so it
// is never silent. Above that nothing is a threshold: 96 merged no families
// where 128 and 192 each merged one, and a floor anywhere would be a guess.

/// `--min-frame-overlap` and `--min-pixel-correlation`: both are fractions.
fn parse_fraction(s: &str) -> std::result::Result<f32, String> {
    let v: f32 = s.trim().parse().map_err(|e| format!("{e}"))?;
    // `contains` is false for NaN, which is the point.
    if !(0.0..=1.0).contains(&v) {
        return Err("must be a number from 0 to 1".into());
    }
    Ok(v)
}

/// The fewest aligned points a match can have: a transform is fitted through
/// three, so every verdict that has any carries at least that many. A
/// `--min-aligned-points` below it is taken as it, and the run says so — as
/// given, 1 and 2 behaved as 3 already, and 0 let a pair with no geometry at
/// all through once the other two bars were 0 as well.
const MIN_ALIGNED_POINTS: u32 = 3;

/// `--min-aligned-points` as the run applies it.
fn aligned_points(args: &Args) -> u32 {
    args.min_aligned_points.max(MIN_ALIGNED_POINTS)
}

/// `-k`: at least one, or nothing past the byte-identical pass is compared.
fn parse_candidates(s: &str) -> std::result::Result<usize, String> {
    match s.trim().parse::<usize>().map_err(|e| format!("{e}"))? {
        0 => Err("must be at least 1".into()),
        v => Ok(v),
    }
}

// ---------------------------------------------------------------- exact pass

/// FNV-1a over the file's bytes. Only files sharing a size are read, so on a
/// normal corpus this touches almost nothing.
///
/// Read a piece at a time rather than whole. Files that share a size are the
/// ones read here, and those include exactly the large ones — uncompressed
/// TIFFs or BMPs from one scanner are all one size, and so are two copies of
/// a disk image under `-x '*'` — so reading each whole, eight at once, held
/// eight of them in memory for a hash.
fn content_hash(path: &Path) -> Option<u128> {
    hash_reader(std::fs::File::open(path).ok()?, 1 << 20)
}

/// The hash of everything `r` yields, `piece` bytes at a time. `piece` is a
/// multiple of eight, so every full read ends on a word boundary and the
/// words are the words of the whole stream.
fn hash_reader(mut r: impl std::io::Read, piece: usize) -> Option<u128> {
    debug_assert!(piece % 8 == 0 && piece > 0);
    let mut h: u128 = 0x6c62272e07bb0142_62b821756295c58d;
    let mut buf = vec![0u8; piece];
    let mut total = 0u64;
    loop {
        // Fill the piece whole unless the file ends: a short read in the
        // middle would split a word across two reads.
        let mut got = 0;
        while got < piece {
            match r.read(&mut buf[got..]) {
                Ok(0) => break,
                Ok(k) => got += k,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        for chunk in buf[..got].chunks(8) {
            let mut v = 0u64;
            for (i, &b) in chunk.iter().enumerate() {
                v |= (b as u64) << (i * 8);
            }
            h ^= v as u128;
            h = h.wrapping_mul(0x0000000001000000_000000000000013B);
        }
        total += got as u64;
        if got < piece {
            break;
        }
    }
    h ^= total as u128;
    Some(h)
}

/// The name each file's cache record is kept under: its canonical path.
///
/// A walk spells a file the way its root was typed, and the cache used to be
/// keyed on that spelling. So one folder scanned as `photos`, as `./photos`
/// and as its absolute path was analysed three times and held three records
/// an image, and a relative record was carried or dropped according to
/// whatever folder the next run happened to start in — `carry_over` asks
/// whether the path exists, and a relative path exists relative to the
/// current directory. The window always names folders absolutely, so it and
/// `img-fp .` never shared a record either. A canonical path is one name per
/// file however it is reached; the report still names files as the run did.
///
/// The walk states most of them already (`known`, beside `files`), and the
/// filesystem is asked only for the rest; see `walk::walk`.
fn cache_names(files: &[PathBuf], known: Vec<Option<PathBuf>>) -> Vec<PathBuf> {
    files
        .par_iter()
        .zip(known)
        .map(|(f, k)| k.unwrap_or_else(|| std::fs::canonicalize(f).or_else(|_| std::path::absolute(f)).unwrap_or_else(|_| f.clone())))
        .collect()
}

fn exact_groups(files: &[PathBuf]) -> Vec<Vec<usize>> {
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if let Ok(md) = std::fs::metadata(f) {
            by_size.entry(md.len()).or_default().push(i);
        }
    }
    // Files that share a size are told apart first by a few pieces of each,
    // and only files that agree on those are read whole. Every same-size file
    // used to be hashed whole on every run, cached or not, and uncompressed
    // pictures from one scanner or camera are all one size: thirty 9 MB BMPs,
    // nothing changed, all of them in the cache, were 270 MB read to find that
    // no two were alike. A sample that differs is proof enough that the files
    // do, and almost every pair of different files differs in its first piece.
    let sampled: Vec<((u64, u128), usize)> = by_size
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .collect::<Vec<_>>()
        .par_iter()
        .flat_map_iter(|(len, group)| {
            group.iter().filter_map(move |&i| sample_hash(&files[i], *len).map(|h| ((*len, h), i))).collect::<Vec<_>>()
        })
        .collect();
    let mut by_sample: HashMap<(u64, u128), Vec<usize>> = HashMap::new();
    for (k, i) in sampled {
        by_sample.entry(k).or_default().push(i);
    }
    // A shared hash is a reason to compare, not a verdict. FNV is no defence
    // against a file made to collide — new bytes enter only the low half of
    // its state, so two files differing in a word and the word after it can
    // be made to agree, and two pictures differing in twelve thousand bytes
    // were reported `identical` — and "identical" is the one claim a person
    // acts on without looking. So the files of a group are compared byte for
    // byte, each against the first of every class found so far. A group of
    // two, which is nearly every group, is compared straight away, and a
    // larger one is hashed whole first so that the comparisons are between
    // files that are almost certainly the same.
    let mut out: Vec<Vec<usize>> = by_sample
        .into_values()
        .filter(|v| v.len() > 1)
        .collect::<Vec<_>>()
        .into_par_iter()
        .flat_map_iter(|mut group| {
            group.sort_unstable();
            let hashed: Vec<Vec<usize>> = if group.len() == 2 {
                vec![group]
            } else {
                let mut by_hash: HashMap<u128, Vec<usize>> = HashMap::new();
                for i in group {
                    if let Some(h) = content_hash(&files[i]) {
                        by_hash.entry(h).or_default().push(i);
                    }
                }
                by_hash.into_values().filter(|v| v.len() > 1).collect()
            };
            let mut found: Vec<Vec<usize>> = Vec::new();
            for mut group in hashed {
                group.sort_unstable();
                let mut classes: Vec<Vec<usize>> = Vec::new();
                for i in group {
                    match classes.iter_mut().find(|c| same_bytes(&files[c[0]], &files[i]) == Some(true)) {
                        Some(c) => c.push(i),
                        None => classes.push(vec![i]),
                    }
                }
                found.extend(classes.into_iter().filter(|c| c.len() > 1));
            }
            found
        })
        .collect();
    out.sort();
    out
}

/// Bytes read from each place `sample_hash` looks.
const SAMPLE_PIECE: u64 = 16 << 10;

/// A hash of four pieces of a file of length `len`: its start, its end, and
/// two places between. A file no longer than the four pieces is hashed whole.
///
/// Four rather than the start alone, because the files this is for are the
/// ones least likely to differ there: an uncompressed scan's first rows are its
/// header and its white margin, and so are its last.
fn sample_hash(path: &Path, len: u64) -> Option<u128> {
    use std::os::unix::fs::FileExt;
    let f = std::fs::File::open(path).ok()?;
    if len <= 4 * SAMPLE_PIECE {
        return hash_reader(f, 1 << 16);
    }
    let mut buf = vec![0u8; 4 * SAMPLE_PIECE as usize];
    for (k, at) in [0, len / 3, 2 * len / 3, len - SAMPLE_PIECE].into_iter().enumerate() {
        let piece = &mut buf[k * SAMPLE_PIECE as usize..(k + 1) * SAMPLE_PIECE as usize];
        f.read_exact_at(piece, at).ok()?;
    }
    hash_reader(&buf[..], 1 << 16)
}

/// Whether two files hold the same bytes, read a piece at a time; `None` if
/// either could not be read.
fn same_bytes(a: &Path, b: &Path) -> Option<bool> {
    same_stream(std::fs::File::open(a).ok()?, std::fs::File::open(b).ok()?, 1 << 20)
}

fn same_stream(mut a: impl std::io::Read, mut b: impl std::io::Read, piece: usize) -> Option<bool> {
    let fill = |r: &mut dyn std::io::Read, buf: &mut [u8]| -> Option<usize> {
        let mut got = 0;
        while got < buf.len() {
            match r.read(&mut buf[got..]) {
                Ok(0) => break,
                Ok(k) => got += k,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        Some(got)
    };
    let (mut x, mut y) = (vec![0u8; piece], vec![0u8; piece]);
    loop {
        let (n, m) = (fill(&mut a, &mut x)?, fill(&mut b, &mut y)?);
        if n != m || x[..n] != y[..m] {
            return Some(false);
        }
        if n < piece {
            return Some(true);
        }
    }
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
    /// The picture's own size, as shown; `feats` holds the working image's.
    dims: (u32, u32),
    ok: bool,
    err: Option<String>,
}

/// Exit code for a run that finished and reported everything it found, but
/// could not do all of it. `0` is a clean run, `1` is the fatal path — an
/// error that stopped the run, printed by `main` — and `130` is the shell's
/// convention for a Ctrl-C; see `interrupt`.
///
/// It exists because the results line reads exactly the same either way. A
/// script that pipes the JSON somewhere sees "4,581 pairs" whether every file
/// opened or a third of them refused, and this is the only part of the run it
/// can test. What counts towards it — and, just as importantly, what is a skip
/// and counts towards nothing — is in `problems.rs`, which also prints the
/// summary that names every one of them.
const EXIT_WITH_PROBLEMS: i32 = 2;

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

/// A verdict of the direct pass, between the files at two indices, the lower
/// first. See `direct_edge`.
type Direct = (u32, u32, Verdict);

/// The edge a direct verdict stands for. Its transform is the verdict's own,
/// and its inversion the verdict's variant's, which the direct pass never sets.
fn direct_edge(d: &Direct) -> (usize, usize, Affine, bool, Verdict) {
    (d.0 as usize, d.1 as usize, d.2.m, d.2.variant.invert, d.2)
}

/// Candidate pairs verified per slice; see the verification pass in `run`.
/// A few seconds of work across the workers, so the barrier between slices
/// costs nothing measurable, and a few megabytes of verdicts, so a slice's
/// own collect is no transient worth the name.
const VERIFY_SLICE: usize = 1 << 16;

static T_DECODE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static T_SIFT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
fn enlarge_below(work: usize, upsample_below: usize) -> usize {
    if work == 0 { upsample_below } else { upsample_below.min(work) }
}

fn analyse(path: &Path, work: usize, p: &sift::Params, header: Option<&decode::Probe>, planes: &Planes) -> Item {
    let t0 = Instant::now();
    let r = decode::decode_with(path, work, header);
    T_DECODE.fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
    match r {
        Ok(d) => {
            let t1 = Instant::now();
            let (feats, thumb) = planes.analysis(&d.work, || {
                let p = sift::Params { upsample_below: enlarge_below(work, p.upsample_below), ..*p };
                let feats = sift::extract(&d.work, &p);
                let thumb = timed!(4, Thumb::build(&d.work, THUMB_LONG));
                (feats.into(), thumb.into())
            });
            T_SIFT.fetch_add(t1.elapsed().as_micros() as u64, Ordering::Relaxed);
            Item { feats, thumb, dims: d.size, ok: true, err: None }
        }
        Err(e) => Item { err: Some(e.to_string()), ..Default::default() },
    }
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
struct Planes {
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

/// The `k` best-scoring candidates a query returned, best first, ties to the
/// lowest index.
///
/// The ordering is a total order — the scores may tie but the image indices
/// cannot — so the `k` that come out and the order they come out in are
/// settled by the comparator alone, and selecting before sorting gives the
/// same answer as sorting the lot. It is not the same cost: a query on a
/// corpus of any size touches most of it, so this was sorting some nine
/// thousand candidates to keep a hundred and fifty of them, once per query
/// and three times more for every image the second look re-asks. Measured on
/// a nine-thousand-image corpus it was 2.2 ms a query, which is more than the
/// retrieval it ranks.
///
/// **The selection runs on integers.** A positive finite float's bits order
/// the same way the float does, so `(!score_bits << 32) | index` is one `u64`
/// whose ascending order is exactly the comparator's — score descending, then
/// the lower index — and comparing two of them is one instruction where the
/// float comparator was a `partial_cmp`, an `unwrap` and a tie-break, each a
/// branch the selection mispredicts about half the time. Nearly every score a
/// query hands this is positive — a touched image shares a word, and only a
/// word in every image weighs nothing; anything else takes the comparator, as
/// before.
fn rank_best(scored: &mut Vec<(u32, f32)>, k: usize) {
    let cmp = |a: &(u32, f32), b: &(u32, f32)| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0));
    if k == 0 {
        scored.clear();
        return;
    }
    if scored.len() <= k || !scored.iter().all(|e| e.1 > 0.0 && e.1.is_finite()) {
        if scored.len() > k {
            scored.select_nth_unstable_by(k - 1, cmp);
            scored.truncate(k);
        }
        scored.sort_unstable_by(cmp);
        return;
    }
    RANK_KEYS.with(|keys| {
        let keys = &mut *keys.borrow_mut();
        keys.clear();
        keys.extend(scored.iter().map(|&(j, s)| (!s.to_bits() as u64) << 32 | j as u64));
        keys.select_nth_unstable(k - 1);
        keys.truncate(k);
        keys.sort_unstable();
        scored.clear();
        scored.extend(keys.iter().map(|&key| (key as u32, f32::from_bits(!((key >> 32) as u32)))));
    });
}

thread_local! {
    /// `rank_best`'s keys, kept per worker rather than allocated per query.
    static RANK_KEYS: std::cell::RefCell<Vec<u64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The three variants' candidate lists as one: each candidate once, under the
/// variant it scored highest for, and the best `k` of those in the order
/// `rank_best` uses — score, then the lower index.
///
/// Each list is already its variant's best `k`, and that is enough to rank
/// the merge exactly. A candidate outside its best variant's top `k` has `k`
/// others ahead of it there, and each of those scores at least as well in the
/// merge as it did in that list, so it could not have made the merged `k`
/// either.
fn merge_variants(merged: &mut Vec<(u32, f32, u8)>, k: usize) {
    merged.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)).then(a.2.cmp(&b.2)));
    merged.dedup_by_key(|e| e.0);
    merged.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    merged.truncate(k);
}

simd::dispatched! {
    /// Quantise every descriptor of an image into the sorted word list the
    /// inverted file speaks.
    fn quantise(vocab: &Vocabulary, f: &Features) -> WordList => quantise_any;
}

/// `quantise`, for whichever copy `dispatched!` chose.
#[cfg_attr(dispatch, inline(always))]
fn quantise_any(vocab: &Vocabulary, f: &Features) -> WordList {
    let mut buf = Vec::with_capacity(4);
    let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(f.len() * 2);
    for i in 0..f.len() {
        vocab.quantise(f.d(i), &mut buf);
        for &w in buf.iter() {
            pairs.push((w, i as u32));
        }
    }
    pairs.sort_unstable();
    // A keypoint index fits an entry's `KP_BITS`: `retain_best` holds an
    // image to `FEATURES`. `from_sorted` checks it, and the word's bound.
    WordList::from_sorted(&pairs)
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

use report::{OutGroup, OutPair, Output};

// ---------------------------------------------------------------- main

/// The command line: `img-fp`.
pub fn cli_main() -> Result<()> {
    check_cpu()?;
    let args = Args::parse();
    if let Some(shell) = args.completions {
        let mut cmd = Args::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        return Ok(());
    }
    if args.man {
        return print_man();
    }
    execute(&args, None)
}

/// Refuse to run on a CPU that lacks what this binary was compiled for.
///
/// A build for x86-64-v3 (AVX2, FMA, BMI2), as `target-cpu=native` makes on a
/// modern machine, has its fast kernels chosen at compile time. On an older
/// CPU the first of those instructions ends the process with SIGILL — "Illegal instruction" and nothing else,
/// which reads as a crash rather than as the wrong build. Asked first, it is a
/// sentence that says which build to use instead.
///
/// It checks exactly the features this binary was compiled with, so a build
/// for the machine it runs on (`target-cpu=native`) always passes, and a
/// portable one (`x86-64`) checks nothing.
pub fn check_cpu() -> Result<()> {
    check_cpu_for("img-fp", "cargo install img-fp --locked")
}

/// `check_cpu` for the window, whose build for this machine is not the
/// command line's: `cargo install img-fp` builds `img-fp` alone, and a person
/// told to run it was left with the same window that had just refused.
pub fn check_cpu_for_window() -> Result<()> {
    check_cpu_for("img-fp-gui", "cargo install img-fp --locked --features gui")
}

fn check_cpu_for(program: &str, install: &str) -> Result<()> {
    #[cfg(target_arch = "x86_64")]
    {
        let mut missing: Vec<&str> = Vec::new();
        macro_rules! need {
            ($($f:tt),*) => {$(
                if cfg!(target_feature = $f) && !std::arch::is_x86_feature_detected!($f) {
                    missing.push($f);
                }
            )*};
        }
        need!("sse4.2", "popcnt", "avx", "avx2", "fma", "bmi1", "bmi2", "lzcnt", "movbe", "f16c");
        if !missing.is_empty() {
            anyhow::bail!(
                "this build of {program} needs a CPU with {}, which this one does not have. \
                 Build it for this machine instead: {install}",
                missing.join(", ")
            );
        }
    }
    Ok(())
}

/// The scan `img-fp-gui` runs: the command line's own, in a child process, with
/// the progress line spoken as JSON on stdout (`progress::speak_json`) and the
/// report also written as JSON to `result`, which is what the window reads.
///
/// `argv` is an ordinary img-fp command line, program name first, so that a
/// scan from the window is exactly the scan the same flags would run from a
/// shell. Cancelling is the command line's Ctrl-C, sent as a signal: see
/// `interrupt`, which is why the scan is a process and not a thread.
pub fn worker_main<I, T>(result: &Path, argv: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = Args::try_parse_from(argv)?;
    progress::speak_json();
    execute(&args, Some(result))
}

/// Whether `argv` is a command line `worker_main` would accept, and clap's
/// message if not. The window checks before it starts anything.
pub fn check_args<I, T>(argv: I) -> std::result::Result<(), String>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = Args::try_parse_from(argv).map_err(|e| e.to_string())?;
    validate(&args)?;
    outputs_are_distinct(&args).map_err(|e| e.to_string())
}

/// The combinations of flags clap cannot express.
///
/// `--cache` with `--no-cache` names a file the run will neither read nor
/// write, which is a mistake — unless `--clear-cache` is there too, and then
/// it is the file to delete. It used to be refused outright, so there was no
/// way to delete a named cache without also using it, and the window, which
/// had a cache file named and "Use the cache" off, dropped `--cache` and
/// deleted the *default* cache instead of the one on screen.
fn validate(args: &Args) -> std::result::Result<(), String> {
    if args.cache.is_some() && args.no_cache && !args.clear_cache {
        return Err("--cache cannot be used with --no-cache, except with --clear-cache to delete that file".into());
    }
    Ok(())
}

/// The defaults of the options a window shows, read from `Args` itself so the
/// two can never disagree.
pub struct Defaults {
    pub work_size: usize,
    pub candidates: usize,
    pub min_aligned_points: u32,
    pub min_frame_overlap: f32,
    pub min_pixel_correlation: f32,
    pub extensions: Vec<String>,
}

pub fn defaults() -> Defaults {
    let a = Args::parse_from(["img-fp", "."]);
    Defaults {
        work_size: a.work_size,
        candidates: a.candidates,
        min_aligned_points: a.min_aligned_points,
        min_frame_overlap: a.min_frame_overlap,
        min_pixel_correlation: a.min_pixel_correlation,
        extensions: a.extensions,
    }
}

/// A preview of an image for a window: RGBA, long side at most `long`.
pub use decode::preview;

/// The only exit path.
///
/// Both binaries' run ends here. `gui` is where the window wants its copy of
/// the report; see `worker_main`.
///
/// `run` does the work and returns either an error that stopped it or nothing
/// at all; this decides what the shell is told. The order is deliberate: the
/// problem summary prints after the results, because it is the part to act on
/// and the results are the part to read, and it prints on the fatal path too —
/// a run that died writing its output has usually already found the images it
/// could not open, and that is still worth saying.
///
/// A fatal error takes precedence over problems for the obvious reason: `1`
/// says the run did not finish, which is a stronger statement than `2`, and
/// anyhow's own `main` handling prints the error and supplies the code.
fn execute(args: &Args, gui: Option<&Path>) -> Result<()> {
    validate(args).map_err(anyhow::Error::msg)?;
    stdout_has_one_reader(args, gui)?;
    outputs_are_distinct(args)?;
    // Opened before any work, so that a log file that cannot be written is an
    // ordinary fatal error at the top of the run rather than a discovery made
    // an hour into one.
    let log = Log::open(args.log_file.as_deref())?;
    // The same for the two files written at the end, which would otherwise
    // fail after the whole run — a directory named where a file was meant is
    // the usual way. Checked, not created: a run that dies should not leave an
    // empty results file, nor truncate the last one.
    let stdout = Path::new("-");
    for path in [&args.output, &args.dump].into_iter().flatten().filter(|p| *p != stdout) {
        report::check_writable(path)?;
    }
    interrupt()?;
    let mut problems = Problems::new(&log);
    let outcome = run(args, &log, &mut problems, gui);
    problems.print_summary();
    outcome?;
    if problems.any() {
        std::process::exit(EXIT_WITH_PROBLEMS);
    }
    Ok(())
}

/// Refuse two outputs that would both be written to stdout.
///
/// The report goes there unless `-o` names a file, and `--dump -` and
/// `--log-file -` go there too; any two of them would be one stream of
/// interleaved CSV, log lines and report that nothing can read back. Under the
/// window stdout is the progress channel, so neither may use it at all.
fn stdout_has_one_reader(args: &Args, gui: Option<&Path>) -> Result<()> {
    let dash = |p: &Option<PathBuf>| p.as_deref() == Some(stdout_path());
    let mut on_stdout: Vec<&str> = Vec::new();
    if gui.is_none() && args.output.as_deref().is_none_or(|p| p == stdout_path()) {
        on_stdout.push("the report");
    }
    if dash(&args.dump) {
        on_stdout.push("--dump -");
    }
    if dash(&args.log_file) {
        on_stdout.push("--log-file -");
    }
    if gui.is_some() && !on_stdout.is_empty() {
        anyhow::bail!("{} cannot be used here: stdout carries the scan's progress", on_stdout.join(" and "));
    }
    if on_stdout.len() > 1 {
        let fix = if on_stdout[0] == "the report" { "; send the report to a file with -o" } else { "" };
        anyhow::bail!("{} would both be written to stdout{fix}", on_stdout.join(" and "));
    }
    Ok(())
}

/// Refuse two of `-o`, `--dump`, `--log-file` and the cache naming one file.
///
/// The log is opened at the start and written a line at a time, and the
/// report and the dump are written at the end, so two of them on one file
/// overwrite each other: the report replaced the dump outright, and over the
/// log it was followed by a run of NUL bytes and the end of the log, written
/// at the offset the log had reached. Nothing said so, and the run exited 0.
///
/// The cache is one of them while the run uses it. `-o` naming it wrote the
/// report over the analysis of every folder the machine had scanned, with
/// exit 0, and the next run found "not a cache file" and started again; the
/// default cache is out of sight, so it is the easiest of the four to name by
/// mistake.
fn outputs_are_distinct(args: &Args) -> Result<()> {
    let cache = (!args.no_cache).then(|| cache::locate(args.cache.as_deref()));
    let named: Vec<(&str, &Path)> = [("-o", &args.output), ("--dump", &args.dump), ("--log-file", &args.log_file)]
        .into_iter()
        .filter_map(|(flag, p)| Some((flag, p.as_deref()?)))
        .filter(|(_, p)| *p != stdout_path())
        .chain(cache.as_deref().map(|p| ("the cache", p)))
        .collect();
    for (i, (fa, a)) in named.iter().enumerate() {
        for (fb, b) in &named[i + 1..] {
            if report::same_destination(a, b) {
                anyhow::bail!("{fa} and {fb} would both write {}; give each its own file", b.display());
            }
        }
    }
    Ok(())
}

/// `--man`: clap's page, plus the exit codes, which clap knows nothing about
/// and which are the part of a run a script can test. Same four as `vid-fp`.
fn print_man() -> Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    clap_mangen::Man::new(Args::command()).render(&mut buf)?;
    if !buf.ends_with(b"\n") {
        buf.push(b'\n');
    }
    buf.extend_from_slice(
        b".SH EXIT STATUS
.TP
.B 0
Ran clean.
.TP
.B 1
Fatal error; the run did not complete.
.TP
.B 2
Completed, but something failed, such as an image that would not decode. See
the Problems summary.
.TP
.B 130
Interrupted with Ctrl-C. Images analysed so far are kept in the cache.
",
    );
    std::io::stdout().write_all(&buf)?;
    Ok(())
}

/// Exit code for an interrupted run: the shell's own for a process ended by
/// SIGINT, and what `vid-fp` returns.
const EXIT_INTERRUPTED: i32 = 130;

/// Ctrl-C (and SIGTERM, SIGHUP) keeps what the analysis has finished, and
/// exits at once.
///
/// **At once is the requirement**, not a nicety: an interrupt that took a few
/// seconds to save would teach people to press Ctrl-C again, and the second
/// press would cost them what the first was saving. So the handler writes
/// nothing. Every finished analysis is appended to the cache the moment its
/// worker makes it (`cache::Store`), which means the work an interrupt keeps
/// is already in the file when the key is pressed, and all there is left to do
/// is wait for the one append that may be in flight — microseconds into the
/// page cache — and hold the lock so no other starts. Analyses still in
/// progress are dropped rather than waited for: one image per worker, and the
/// next run redoes them in the time this one would have spent finishing.
///
/// Nothing after the analysis is worth keeping, because nothing after it is a
/// property of one file — the vocabulary, the candidates and the verdicts all
/// depend on the corpus as a whole. So an interrupt in any other stage keeps
/// what the cache already holds and exits just as fast, and one during a
/// compaction removes the half-written copy and leaves the file it was copying.
///
/// **A second press is the default action**, which is to die on the signal:
/// the handler hands SIGINT back to the kernel before doing anything else. The
/// first press takes microseconds, so the second should never be needed; it
/// is there for a disk that has stopped answering. The worst it can leave is
/// half a record at the end of the cache, which the next run cuts off.
///
/// No summary: the run did not finish, and the problems it had found so far
/// are an account of a run nobody is going to read the results of.
fn interrupt() -> Result<()> {
    ctrlc::set_handler(|| {
        const SIG_DFL: usize = 0;
        unsafe extern "C" {
            fn signal(signum: i32, handler: usize) -> usize;
        }
        // SIGHUP, SIGINT, SIGTERM.
        for sig in [1, 2, 15] {
            unsafe {
                signal(sig, SIG_DFL);
            }
        }
        let (_held, kept) = cache::seal();
        progress::clear_for_exit();
        if kept > 0 {
            let (noun, verb) = if kept == 1 { ("description", "was") } else { ("descriptions", "were") };
            progress::to_stderr(&format!("Interrupted. {kept} new image {noun} {verb} saved to cache."));
        } else {
            progress::to_stderr("Interrupted.");
        }
        // `_exit`, not `exit`: the workers are still running, some of them
        // inside libheif, and `exit` would run the C++ static destructors out
        // from under them. Nothing is buffered that needs flushing — stderr
        // is not, the log is written a line at a time, and the cache is
        // written through the kernel.
        unsafe extern "C" {
            fn _exit(status: i32) -> !;
        }
        unsafe { _exit(EXIT_INTERRUPTED) }
    })
    .context("could not install the Ctrl-C handler")
}

fn run(args: &Args, log: &Log, problems: &mut Problems, gui: Option<&Path>) -> Result<()> {
    // Before anything else: a `-x` that can match no file is a mistake to
    // stop on, not a walk to take.
    let (wanted, extensions_note) = extensions::normalize(&args.extensions)?;
    // So is a path list that cannot be read, and it is read before anything
    // has a side effect: `--clear-cache` with a mistyped `--from-file` must
    // not delete the cache and then stop.
    let (roots, lists) = walk::requested_roots(
        &walk::Sources {
            named: &args.roots,
            from_file: args.from_file.as_deref(),
            null_separated: args.null,
        },
    )?;
    prof::start();
    let t_start = Instant::now();
    few_arenas();
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new().num_threads(args.threads).build_global()?;
    }
    let verbose = args.verbose;
    let progress = progress::Progress::new();
    // Every line the run says about itself goes to the log file whether or not
    // the console asked for it: `-v` is about the terminal and `--log-file` is
    // about the file, and one flag must not quietly decide the other. (That
    // exact confusion is written up in `vid-fp`'s `verbosity`, where a
    // `-q --log-file` run wrote an empty log.)
    macro_rules! stage {
        ($t:expr, $($arg:tt)*) => {
            if verbose || log.active() {
                let line = format!("[{:6.1}s] {}", $t.elapsed().as_secs_f64(), format!($($arg)*));
                if verbose { progress.println(&line); }
                log.line(&line);
            }
        };
    }
    // Said on the console whatever the flags, and kept in the log with it.
    macro_rules! say {
        ($($arg:tt)*) => {{
            let line = format!($($arg)*);
            progress.println(&line);
            log.line(&line);
        }};
    }

    // Where the analysis is kept. Resolved even when the file does not exist
    // yet, because that is the first run and it is the run that creates it —
    // and even under `--no-cache`, if the run has been asked to delete it.
    let cache_path = if args.no_cache && !args.clear_cache {
        None
    } else {
        cache::resolve_path(args.cache.as_deref(), problems)
    };
    // The run's header, in `vid-fp`'s words: what it was asked to do, before
    // it does any of it, so a run's log says which settings produced it.
    say!(
        "Settings -> Work size: {}, Candidates: {}, Min aligned points: {}, Min frame overlap: {}, \
         Min pixel correlation: {}, Threads: {}, Recursive: {}, Follow symlinks: {}",
        args.work_size,
        args.candidates,
        args.min_aligned_points,
        args.min_frame_overlap,
        args.min_pixel_correlation,
        rayon::current_num_threads(),
        args.recursive,
        args.follow_symlinks
    );
    if let Some(note) = &extensions_note {
        say!("{note}");
    }
    if args.min_aligned_points < MIN_ALIGNED_POINTS {
        say!(
            "Note: --min-aligned-points {} behaves the same as {MIN_ALIGNED_POINTS}, the fewest points a match is fitted through.",
            args.min_aligned_points
        );
    }
    if args.clear_cache {
        if let Some(p) = &cache_path {
            say!("Clearing all cache at {}...", p.display());
            match std::fs::remove_file(p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                // Asked for and not done, and the next run will read the cache
                // this one meant to be rid of.
                Err(e) => problems.cache(format!("could not delete {}: {e}", p.display())),
            }
        }
    }
    let cache_path = cache_path.filter(|_| !args.no_cache);
    match &cache_path {
        Some(p) => say!("Cache: {}", p.display()),
        None if args.no_cache => say!("Cache: off (--no-cache)"),
        None => say!("Cache: off"),
    }
    let named: Vec<&PathBuf> = args.roots.iter().filter(|p| p.as_os_str() != "-").collect();
    if !named.is_empty() || lists.is_empty() {
        say!("Scanning: {:?}", named);
    }
    for (from, n) in &lists {
        say!("Scanning: {n} path(s) read from {from}");
    }
    if !args.exclude.is_empty() {
        say!("Excluding: {:?}", args.exclude);
    }
    say!("{}", wanted.describe());
    let (files, canonical) = walk::walk(
        &walk::Request {
            roots: &roots,
            exclude: &args.exclude,
            wanted: &wanted,
            recursive: args.recursive,
            follow_symlinks: args.follow_symlinks,
        },
        problems,
    );
    stage!(t_start, "{} files", files.len());
    if files.is_empty() {
        // Still a finished run, and its report says so. Returning without one
        // left whatever the last run wrote to `-o` in place, and a script
        // reading it read the last run's groups as this one's.
        progress.finish();
        let out = Output {
            tool: "img-fp",
            config: run_config(args),
            files_enumerated: 0,
            files_analysed: 0,
            failures: Vec::new(),
            runtime_seconds: 0.0,
            groups: Vec::new(),
            pairs: Vec::new(),
        };
        match write_reports(args, gui, &out, &files, &[])? {
            Some(path) => say!("No images found. -> {}", path.display()),
            None => say!("No images found."),
        }
        return Ok(());
    }

    // The bar's first estimate of the whole run, from nothing but the file
    // count; every stage below refines it as soon as it knows better.
    progress.forecast(|f| {
        f.files = files.len();
        f.cache_bytes = cache_path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
        f.cache_write = cache_path.is_some();
        f.candidates_per_query = args.candidates;
    });

    // Byte-identical copies, before anything is decoded.
    progress.begin(Stage::Identical);
    let mut exact = timed!(33, exact_groups(&files));
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
    // The cache is keyed on the paths themselves, not on their printable
    // form: two names that are not UTF-8 can print the same and be two files.
    // And on each file's canonical path, not the one this run spells; see
    // `cache_names`.
    let names: Vec<PathBuf> = if cache_path.is_some() { cache_names(&files, canonical) } else { Vec::new() };
    let (mut cached, mut store) = match &cache_path {
        Some(p) => {
            progress.begin(Stage::CacheRead);
            // Only this run's paths are unpacked; the rest of the machine's
            // cache is read for where it is, not for what it says. See
            // `cache::open`.
            let walked: std::collections::HashSet<&Path> = names.iter().map(|p| p.as_path()).collect();
            let (cached, store) = cache::open(p, settings, &|path| walked.contains(path), problems);
            (cached, Some(store))
        }
        None => Default::default(),
    };
    // Records in the file that the map does not hold, and that are not this
    // file's records at other settings: each superseded by a later one for
    // the same path. Something to compact away, if nothing else is.
    let superseded = store.as_ref().map_or(0, |s| s.records() - cached.len() - s.other_settings().len());
    if !cached.is_empty() {
        stage!(t_start, "cache: {} usable records", cached.len());
    }
    // Every walked file's record comes *out* of the map rather than being
    // copied from it. The analysis is the largest thing the run holds, and
    // cloning it here made it exist twice over — once in the map and once in
    // `items` — for the length of the phase that already sets the peak, and
    // then charged a second bill to free the originals. On the found corpus
    // that was 1.9 s of dropping a nine-thousand-record map, about as much
    // again cloning into it, and 164 MB of peak.
    //
    // What is left in the map afterwards is exactly the records for files this
    // run did not walk, which is what `carry_over` wants; the two counts taken
    // here are what tells the save below whether it has anything to write.
    let (mut in_cache, mut same_key) = (0usize, 0usize);
    let mine: Vec<Option<(cache::Record, cache::Span)>> = files
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let got = cached.remove(names.get(i)?)?;
            in_cache += 1;
            let k = cache::key_of(&files[i])?;
            if got.0 != k {
                return None;
            }
            same_key += 1;
            // A walked path's record is always unpacked.
            Some((got.1?, got.2))
        })
        .collect();
    // Where each file's record sits in the cache file, for the ones that have
    // one. A cached record's is known now; a new one's, when its worker has
    // appended it.
    let mut span_of: Vec<Option<cache::Span>> = mine.iter().map(|m| m.as_ref().map(|r| r.1)).collect();
    // The exact pass has already grouped the files whose bytes hash the same,
    // and the analysis depends on nothing but those bytes. Describing the
    // second copy of a file is not a cheaper way to reach the same answer, it
    // is the same work done twice: each group elects one member — one the
    // cache already knows, if any does, and otherwise its first — and the rest
    // are copied from it. Those groups are already claimed as duplicates in
    // the output, so sharing one analysis between them asserts nothing the
    // run does not assert anyway.
    //
    // The same goes for everything after the analysis; see `with_copies`.
    let mut twin_of: Vec<usize> = (0..n).collect();
    for g in exact.iter() {
        let original = g.iter().copied().find(|&i| mine[i].is_some()).unwrap_or(g[0]);
        for &i in g.iter() {
            twin_of[i] = original;
        }
    }
    // A copy keeps no record of its own: it is answered by its original on
    // every run, cached or not, so a record under its name is the same
    // analysis stored twice and never read. One written by an earlier build
    // is left out of the file at the next compaction.
    for i in 0..n {
        if twin_of[i] != i {
            span_of[i] = None;
        }
    }
    // What each file still to be analysed is expected to cost, for the bar
    // and nothing else. Read from headers, in parallel, and only for files the
    // cache and the exact pass have not already answered, so a cached run
    // opens nothing here.
    let todo = (0..n).filter(|&i| twin_of[i] == i && mine[i].is_none()).count();
    // Whether the save below will have anything to write, as far as it can be
    // told yet: something new to describe, or a record that no longer fitted.
    let rewrite = cache_path.is_some()
        && (superseded > 0 || same_key != in_cache || args.prune_cache || (0..n).any(|i| twin_of[i] != i && mine[i].is_some()));
    progress.forecast(|f| {
        f.to_describe = Some(todo);
        f.cache_write = rewrite;
    });
    let headers = progress.begin_counted(Stage::Headers, n as u64, n, "files");
    // The headers are kept as well: the decode claims its share of the
    // decode budget from them before it reads the file.
    let (weight, header): (Vec<u64>, Vec<Option<decode::Probe>>) = (0..n)
        .into_par_iter()
        .map(|i| {
            headers.tick();
            if twin_of[i] != i || mine[i].is_some() {
                return (0, None);
            }
            let bytes = std::fs::metadata(&files[i]).map(|m| m.len()).unwrap_or(0);
            let probe = decode::probe(&files[i]);
            (progress::analysis_cost(probe.as_ref(), bytes, args.work_size, sp.upsample_below), probe)
        })
        .unzip();
    stage!(t_start, "{todo} files to analyse");
    // A wildcard walk has not guessed at anything, so what it found is files;
    // calling them images is how a home directory reads as a photo library.
    let found = if wanted.is_a_guess_at_images() { ("image", "images") } else { ("file", "files") };
    // The three add up to `n`: a copy is answered by its original whether or
    // not the cache also knew it, so it is counted as a copy and only there.
    let copies = (0..n).filter(|&i| twin_of[i] != i).count();
    let from_cache = n - copies - todo;
    if from_cache > 0 || copies > 0 {
        say!(
            "Found {}; {from_cache} already cached, {}, {todo} to analyse.",
            count(n, found.0, found.1),
            count(copies, "identical copy", "identical copies")
        );
    } else {
        say!("Found {}. Analysing...", count(n, found.0, found.1));
    }
    let total_weight: u64 = weight.iter().sum();
    progress.forecast(|f| f.describe = Some(total_weight));
    let bar = progress.begin_counted(Stage::Describe, total_weight, todo, "images");
    // Each record goes into the cache file as soon as it is made, which is
    // what lets an interrupt keep the analysis without writing anything; see
    // `cache::Store`.
    let keep = |i: usize, key: Option<cache::Key>, it: &Item| -> Option<cache::Span> {
        let (s, key) = (store.as_ref()?, key?);
        if !it.ok {
            return None;
        }
        s.append(&names[i], key, it.dims, &it.feats, &it.thumb)
    };
    let planes = Planes::default();
    let (mut items, appended): (Vec<Item>, Vec<Option<cache::Span>>) = mine
        .into_par_iter()
        .enumerate()
        .map(|(i, rec)| {
            if twin_of[i] != i {
                return (Item::default(), None);
            }
            if let Some((rec, _)) = rec {
                let it = Item { feats: rec.feats.into(), thumb: rec.thumb.into(), dims: rec.dims, ok: true, err: None };
                return (it, None);
            }
            let f = &files[i];
            // Keyed before it is read, so that a file changing under the
            // analysis is described again next time rather than trusted.
            let key = store.as_ref().and_then(|_| cache::key_of(f));
            let it = analyse(f, args.work_size, &sp, header[i].as_ref(), &planes);
            let span = keep(i, key, &it);
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            bar.add(weight[i]);
            if (verbose || log.active()) && d % 250 == 0 {
                let line = format!("[{:6.1}s]   described {d}", t_start.elapsed().as_secs_f64());
                if verbose {
                    progress.println(&line);
                }
                log.line(&line);
            }
            (it, span)
        })
        .unzip();
    drop(weight);
    drop(header);
    drop(planes);
    for (s, a) in span_of.iter_mut().zip(appended) {
        if a.is_some() {
            *s = a;
        }
    }
    for i in 0..n {
        let r = twin_of[i];
        if r == i {
            continue;
        }
        // A copy of a file that would not decode fails as its original did, and
        // is reported under its own path by the loop over `items` below. It
        // used to be decoded again, here, on one thread, one copy after
        // another — the same bytes through the same decoder, for the same
        // error. (Both were read whole by the exact pass, so an original that
        // could not be *opened* is never in a group to begin with.)
        items[i] = Item {
            feats: items[r].feats.clone(),
            thumb: items[r].thumb.clone(),
            dims: items[r].dims,
            ok: items[r].ok,
            err: items[r].err.clone(),
        };
    }
    // `--prune-cache` keeps only what this scan found, and gives that up when
    // the scan is not a complete account of what is out there: a root that
    // would not resolve or a directory that would not open leaves files
    // unseen, and dropping their records would cost a re-analysis of images
    // nothing is wrong with. The run says so and exits 2 rather than pruning
    // anyway or pruning silently.
    let prune = args.prune_cache
        && (problems.walk_was_complete() || {
            say!("not pruning: the walk could not read everything it was pointed at");
            problems.cache("--prune-cache skipped: the walk was incomplete".into());
            false
        });
    progress.forecast(|f| {
        // What the matching stages will be handed: originals, not copies.
        f.images = Some((0..n).filter(|&i| items[i].ok && twin_of[i] == i).count());
        f.descriptors = Some((0..n).filter(|&i| twin_of[i] == i).map(|i| items[i].feats.len()).sum());
    });
    if let (Some(p), Some(store)) = (&cache_path, store.as_mut()) {
        progress.begin(Stage::CacheWrite);
        if let Some(e) = store.failure() {
            // Not fatal — the pairs this run reports are the same pairs — but
            // the next run will pay for this one's analysis all over again,
            // which is exactly what a problem is here.
            problems.cache(format!("could not write {}: {e}", p.display()));
        }
        let mut entries: Vec<(&Path, cache::Span)> =
            (0..n).filter(|&i| items[i].ok).filter_map(|i| Some((names[i].as_path(), span_of[i]?))).collect();
        // What the cache already knew about images this run never walked —
        // which, every walked file's record having been taken out of the map
        // above, is everything still in it. The cache is one file for the
        // machine rather than one per corpus, so scanning a second directory
        // must not cost the first one its analysis; see `cache::carry_over`.
        let kept = if prune { Vec::new() } else { cache::carry_over(&cached) };
        if !kept.is_empty() {
            stage!(t_start, "cache: {} records kept from other scans", kept.len());
        }
        entries.extend_from_slice(&kept);
        // And the analyses made at other settings, which this run did not
        // read: kept while their file is there, and dropped by `--prune-cache`,
        // which keeps only what this scan used. They used to be kept under it
        // too, for the files this scan found, so one run at another
        // `--work-size` doubled the cache for good: nothing short of
        // `--clear-cache`, which takes every record with it, could give the
        // copy back.
        let others: Vec<(PathBuf, cache::Span)> =
            store.other_settings().iter().filter(|(p, _)| !prune && p.exists()).cloned().collect();
        if !others.is_empty() {
            stage!(t_start, "cache: {} records kept from other working sizes", others.len());
        }
        let others_dropped = store.other_settings().len() - others.len();
        entries.extend(others.iter().map(|(p, s)| (p.as_path(), *s)));
        let dropped = cached.len() - kept.len() + others_dropped;
        if prune && dropped > 0 {
            say!("pruned {dropped} cached record(s) this scan did not use");
        }
        // Every record worth keeping is already in the file, since each went
        // in as it was made. The file needs rewriting only when it holds
        // something else as well — a record superseded, dropped or pruned —
        // and then the rewrite is a copy; see `cache::Store::compact`.
        //
        // Nothing to rewrite is worth noticing rather than rewriting anyway. A
        // threshold sweep is a dozen runs over one unchanged corpus, and every
        // one of them finds the file already says what it would write.
        if store.records() == entries.len() || !store.writable() {
            stage!(t_start, "cache: {} records, none to compact", store.records());
        } else if let Err(e) = store.compact(&mut entries) {
            problems.cache(format!("could not write {}: {e}", p.display()));
        }
    }
    drop(store);
    // What is left in the map is the analyses of files this run did not walk,
    // which the save above has just finished borrowing. Nothing reads them
    // again and they are megabytes apiece.
    drop(cached);
    stage!(
        t_start,
        "cpu: decode {:.0}s, features {:.0}s",
        T_DECODE.load(Ordering::Relaxed) as f64 / 1e6,
        T_SIFT.load(Ordering::Relaxed) as f64 / 1e6
    );
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
    timed!(37, release_memory());
    // Counted here rather than at the JSON `failures` below, which is built
    // from the same `items`: this is the last point every file has been
    // through, and a fatal error further down should not lose the account of
    // what would not open.
    let not_asked: Vec<bool> = (0..n)
        .map(|i| {
            !wanted.is_a_guess_at_images()
                && !extensions::names_an_image(&files[i])
                && items[i].err.as_deref() == Some(decode::NOT_AN_IMAGE)
        })
        .collect();
    for (i, it) in items.iter().enumerate() {
        match &it.err {
            // A file a wildcard walk reached that is no picture at all never
            // claimed to be one; see `extensions.rs`.
            Some(_) if not_asked[i] => problems.not_image_content(&files[i].display().to_string()),
            Some(e) => problems.unreadable(&files[i].display().to_string(), e),
            // Decoded, described, and described to nothing. Nothing failed —
            // a blank picture really has no local features — but the file is
            // in the corpus and the matcher has nothing to say about it, and
            // that is worth a line for the same reason a refusal is: the run
            // did not answer the question it was asked about this file.
            None if it.feats.len() == 0 => problems.featureless(&files[i].display().to_string()),
            None => {}
        }
    }
    // Two byte-identical files that are not pictures at all — a pair of empty
    // files, two copies of a README — are identical, and are not a duplicate
    // *image*, which is the only claim this tool makes. Only a wildcard walk
    // can bring such files in, and they leave the exact groups here, before
    // anything reads those groups as pairs. A byte-identical pair of broken
    // `.jpg`s is still reported: those did claim to be images.
    for g in exact.iter_mut() {
        g.retain(|&i| !not_asked[i]);
    }
    exact.retain(|g| g.len() > 1);
    let n_ok = items.iter().filter(|i| i.ok).count();
    let n_desc: usize = items.iter().map(|i| i.feats.len()).sum();
    stage!(t_start, "described {n_ok}/{n} images, {n_desc} descriptors");

    // Everything from here to the assembly works on distinct contents: each
    // set of byte-identical files is its original, and a copy is given its
    // original's pairs at the end (`with_copies`). A copy used to be indexed,
    // queried and verified like any other file — against its own original,
    // among others, to find the identity the exact pass had already found —
    // and it took a place in every candidate list its original was in.
    let matched: Vec<bool> = (0..n).map(|i| items[i].ok && twin_of[i] == i).collect();
    let n_match = matched.iter().filter(|&&m| m).count();
    let n_desc_match: usize = (0..n).filter(|&i| matched[i]).map(|i| items[i].feats.len()).sum();

    // Vocabulary from the corpus itself, at a depth the corpus chooses. A
    // copy's descriptors are its original's, so counting them would weigh
    // that picture twice in the sample and size the tree for words that are
    // not there.
    say!("Analysis complete. Matching {}...", count(n_ok, "image", "images"));
    let vp = index::VocabParams::for_corpus(n_desc_match);
    progress.forecast(|f| f.vocab_sample = Some(vp.sample.min(n_desc_match)));
    progress.begin(Stage::Vocabulary);
    let mut pool: Vec<&[u8; DESC_LEN]> = Vec::with_capacity(vp.sample.min(n_desc_match));
    timed!(34, {
        // Even sampling across images, so one feature-rich image cannot own
        // the vocabulary.
        let per = (vp.sample / n_match.max(1)).max(8);
        for it in (0..n).filter(|&i| matched[i]).map(|i| &items[i]) {
            let take = it.feats.len().min(per);
            let step = (it.feats.len() / take.max(1)).max(1);
            for i in (0..it.feats.len()).step_by(step).take(take) {
                pool.push(it.feats.d(i).try_into().unwrap());
            }
        }
    });
    let vocab = timed!(12, Vocabulary::build(pool, &vp));
    stage!(t_start, "vocabulary: {} live words of {} from {} samples", vocab.n_live_words(), vocab.n_words(), vp.sample.min(n_desc_match));

    // Quantise. The word lists are built straight into the vector the inverted
    // file and every later stage read from: holding a second copy per image
    // costs as much again as the lists themselves.
    progress.forecast(|f| f.live_words = Some(vocab.n_live_words()));
    let bar = progress.begin_counted(Stage::Quantise, n_desc_match as u64, n_match, "images");
    //
    // Files that share an analysis (see `Planes`) share their words too, so
    // each analysis is quantised once and its list copied to the rest.
    let mut first_of: HashMap<*const Features, usize> = HashMap::new();
    let same_as: Vec<usize> =
        (0..n).map(|i| if matched[i] { *first_of.entry(std::sync::Arc::as_ptr(&items[i].feats)).or_insert(i) } else { i }).collect();
    drop(first_of);
    let mut lists: Vec<WordList> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !matched[i] || same_as[i] != i {
                return WordList::default();
            }
            let it = &items[i];
            let wl = timed!(11, quantise(&vocab, &it.feats));
            bar.add(it.feats.len() as u64);
            wl
        })
        .collect();
    for i in (0..n).filter(|&i| same_as[i] != i) {
        lists[i] = lists[same_as[i]].clone();
        bar.add(items[i].feats.len() as u64);
    }
    drop(same_as);
    stage!(t_start, "quantised");

    // Inverted file, of every word; see `InvertedFile::build`.
    progress.begin(Stage::InvertedFile);
    let inv = timed!(13, InvertedFile::build(&lists, vocab.n_live_words()));
    stage!(t_start, "inverted file");

    let policy = verify::Policy::new(aligned_points(args), args.min_frame_overlap, args.min_pixel_correlation);


    // ---- candidates, then verification
    //
    // Retrieval and verification are separate passes so that a pair proposed
    // from both sides is verified once. Containment scoring is asymmetric —
    // the crop finds the photograph much more readily than the reverse — so
    // both directions genuinely have to be asked.
    type Edge = (usize, usize, Affine, bool, Verdict);
    let bar = progress.begin_counted(Stage::Candidates, n_desc_match as u64, n_match, "images");
    let mut cand_pairs: Vec<(u32, u32)> = (0..n)
        .into_par_iter()
        .filter(|&i| matched[i])
        .map_init(
            || (vec![0f32; n], Vec::new()),
            |(acc, scored), i| {
                timed!(14, inv.query(&lists[i], i as u32, acc, scored));
                bar.add(items[i].feats.len() as u64);
                timed!(41, {
                    // Every image the query touched, including one that scores
                    // nothing: that happens only when all it shares are words
                    // in every image, which only a small folder or one that is
                    // all one picture has (see `query_touched`) — and on a
                    // folder of two it is every word a duplicate shares. Dropping it
                    // there found no pair at all; the second look never did.
                    scored.retain(|&(j, _)| matched[j as usize]);
                    rank_best(scored, args.candidates);
                    scored
                        .iter()
                        .map(|&(j, _)| if (j as usize) < i { (j, i as u32) } else { (i as u32, j) })
                        .collect::<Vec<_>>()
                })
            },
        )
        .flatten()
        .collect();
    timed!(35, {
        cand_pairs.par_sort_unstable();
        cand_pairs.dedup();
    });
    stage!(t_start, "candidates: {} pairs", cand_pairs.len());
    // Verification reads the word lists and never the postings, and the
    // second look, which does, is asked of a few per cent of the files after
    // verification is done. So the postings are let go here and built again
    // from the same lists if the second look has anything to ask: on a corpus
    // of 27,000 pictures that is 94 MB off the run's peak, which is the end of
    // verification, for a rebuild of under two seconds on one thread. The
    // build is deterministic, so the second index is the first.
    drop(inv);

    let dumping = args.dump.is_some();
    // What a verdict must already have before its pixels are worth reading:
    // the weakest bar any tier applies, or everything a dump would record.
    //
    // A dump widens the gate and never narrows it: the bars the run's own
    // tiers use still decide, so a run with `--dump` finds the pairs the same
    // run without it finds. (It used to *set* the gate to (3, 0.2), which is
    // tighter than a `--min-frame-overlap` below 0.2 and so lost pairs.)
    let tier_gate = (policy.corroborated.min_aligned_points, policy.corroborated.min_frame_overlap);
    let gate = if dumping { (tier_gate.0.min(3), tier_gate.1.min(0.2)) } else { tier_gate };
    progress.forecast(|f| f.candidate_pairs = Some(cand_pairs.len()));
    let bar = progress.begin_counted(Stage::Verify, cand_pairs.len() as u64, cand_pairs.len(), "pairs");
    // Verified a slice of the candidates at a time, each slice's verdicts
    // appended to the one list as it finishes. A parallel `collect` of a
    // filtered stream gathers every worker's results into vectors of their
    // own, grown by doubling, and only then copies them into one: on a corpus
    // of 27,000 pictures that was some 300 MB alive for a moment at the end of
    // verification, which was the run's peak, and the holes it left in the
    // heap were dirtied again by the second look. The slices are taken in
    // order and each is collected in order, so the list is the one a single
    // collect gave, verdict for verdict.
    //
    // A verdict is kept as `Direct`, not as an `Edge`: an edge carries its
    // transform beside the verdict that already holds it, its inversion beside
    // the verdict's own variant, and two `usize`s for indices that fit a
    // `u32`. That is 112 bytes against 76, for the one list of edges held
    // through the peak — 923,601 of them on a corpus of 27,000 pictures.
    let mut all_direct: Vec<Direct> = Vec::new();
    for slice in cand_pairs.chunks(VERIFY_SLICE) {
        let part: Vec<Direct> = slice
            .par_iter()
            .map_init(
                || (Vec::new(), Vec::new(), verify::Scratch::default()),
                |(cands, matches, scratch), &(i, j)| timed!(42, {
                    bar.tick();
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
                    (v.accepted(&policy.corroborated) || (dumping && v.n_in >= 3)).then_some((i as u32, j as u32, v))
                }),
            )
            .flatten()
            .collect();
        all_direct.extend(part);
    }
    // What a round of propagation may spend: one composed hypothesis for each
    // candidate the direct pass verified. See `propagate`.
    let prop_budget = cand_pairs.len();
    // Every candidate has its verdict now, and the second look below is where
    // a found corpus's run peaks: a million pairs held through it for nothing.
    drop(cand_pairs);
    // Anchors only: a pair believed on its own evidence. These are what decide
    // which files end up in one cluster. They are read out of `all_direct`
    // where they lie rather than copied: they are nearly all of it, and a copy
    // was another hundred-odd megabytes held through the second look, which
    // is where a large corpus's run peaks.
    let edges = || all_direct.iter().filter(|(_, _, v)| v.accepted(&policy.anchor)).map(direct_edge);
    let n_edges = edges().count();
    stage!(t_start, "anchors: {} of {} verified pairs", n_edges, all_direct.len());

    // ---- second look at images nothing matched: mirrored and inverted
    // A mirrored or inverted copy shares no visual words with its original —
    // the descriptor bins are permuted — so retrieval cannot find it, and the
    // query has to be re-asked with the permutation applied. Doing that for
    // every image would roughly double the retrieval cost for a handful of
    // pairs, so it is asked only where the first pass came up short: a file
    // with no matches, or with so few that it may be hanging off the edge of
    // its real cluster.
    let mut degree = vec![0u32; n];
    for (a, b, _, _, _) in edges() {
        degree[a] += 1;
        degree[b] += 1;
    }
    // A byte-identical copy is not counted. It is the same picture, so it says
    // nothing about whether the picture's other versions were found — and
    // counted, it kept every file that has a copy from ever being asked
    // again: in a library where each photograph is there twice over, no file
    // was, and every mirrored and inverted version in it went unfound.
    // Ask again, mirrored and inverted, for images that no *pair* of matches
    // has anchored yet. A file with one match is not safely found: that match
    // may be the wrong one, and a mirrored query is cheap next to being wrong.
    // Two is not a fitted threshold, it is "more than one" — the same minimal
    // constant the bridge test uses for "more than one file", and the only
    // place a count appears in either rule. Trying the tighter reading, "no
    // matches at all", costs a seed apiece on three of the held-out mirror
    // transforms.
    let lonely: Vec<usize> = (0..n).filter(|&i| matched[i] && degree[i] < 2).collect();
    let variants = [
        Variant { mirror: true, invert: false },
        Variant { mirror: false, invert: true },
        Variant { mirror: true, invert: true },
    ];
    // Each variant is asked the same retrieval question it always was, and
    // then the three answers are spent as one: a candidate is verified once,
    // under the variant it scored highest for, and only the best `-k` of the
    // merged list are verified at all.
    //
    // It used to be the best `-k` of *each* variant, three verdicts' worth of
    // candidates per file, and a verdict is what the pass spends most of its
    // time rejecting — 3.94 M of them on a found corpus for 459 pairs. Almost
    // every file the pass is asked about has no mirrored or inverted twin, so
    // two of its three lists are nothing but the retrieval's best guesses at
    // a question with no answer, and a third list of those says nothing the
    // first two did not. What a true pair looks like is a candidate that
    // scores far above the rest under the one variant that relates the two
    // files; it keeps its place when the lists are merged, and the pairs
    // measure it: 459 of 459 anchors on the found corpus come from their
    // highest-scoring variant, 447 of them inside the merged best 150, and
    // the benchmark corpus's output is 217,376 pairs against 217,380 with
    // not one false pair more.
    // What each re-asked file will cost the bar: three descents and three
    // queries, which scale with its descriptors, and up to `-k` verdicts.
    let lonely_weight: Vec<u64> = {
        let fc = progress.forecast_now();
        lonely.iter().map(|&i| fc.variant_cost(items[i].feats.len())).collect()
    };
    let total_weight: u64 = lonely_weight.iter().sum();
    progress.forecast(|f| {
        f.variants = Some(total_weight);
        f.edges = Some(n_edges);
    });
    let bar = progress.begin_counted(Stage::Variants, total_weight, lonely.len(), "images");
    // The vocabulary is wanted here only to quantise the re-asked files'
    // mirrored and inverted descriptors, and the postings only to query them.
    // Where those files are few — a benchmark corpus, whose files nearly all
    // found their family — their word lists are quantised first and the
    // vocabulary let go before the postings are built again (see the drop
    // after the candidates), so the two are never held together: on 27,000
    // pictures that is 1,336 files' lists, some 20 MB, against a 181 MB
    // vocabulary. Where they are most of the corpus — a found one, where
    // nearly every file is re-asked — their lists would outweigh the
    // vocabulary several times over, and each file's are made and spent in
    // turn as before. The words are the same words either way.
    let n_live = vocab.n_live_words();
    let mut vocab = Some(vocab);
    let ahead_bytes = lonely.iter().map(|&i| lists[i].len()).sum::<usize>() * 3 * WordList::BYTES_PER_ENTRY;
    let ahead: Option<Vec<[WordList; 3]>> = (ahead_bytes < vocab.as_ref().unwrap().heap_bytes()).then(|| {
        let vocab = vocab.as_ref().unwrap();
        lonely
            .par_iter()
            .map(|&i| {
                std::array::from_fn(|v| {
                    let vf = timed!(40, variant_features(&items[i].feats, variants[v]));
                    timed!(39, quantise(vocab, &vf))
                })
            })
            .collect()
    });
    if ahead.is_some() {
        vocab = None;
    }
    let inv = (!lonely.is_empty()).then(|| timed!(13, InvertedFile::build(&lists, n_live)));
    let variant_all: Vec<(Edge, bool)> = lonely
        .par_iter()
        .enumerate()
        .zip(lonely_weight.par_iter())
        .map_init(
            || (vec![0f32; n], Vec::new(), Vec::new(), Vec::new(), verify::Scratch::default(), Vec::new()),
            |(acc, scored, cands, matches, scratch, merged), ((k, &i), &w)| timed!(38, {
                let mut out: Vec<(Edge, bool)> = Vec::new();
                let vf: [Features; 3] = timed!(40, std::array::from_fn(|v| variant_features(&items[i].feats, variants[v])));
                let made: [WordList; 3];
                let wl: &[WordList; 3] = match &ahead {
                    Some(a) => &a[k],
                    None => {
                        let vocab = vocab.as_ref().unwrap();
                        made = std::array::from_fn(|v| timed!(39, quantise(vocab, &vf[v])));
                        &made
                    }
                };
                merged.clear();
                for v in 0..3 {
                    timed!(14, inv.as_ref().unwrap().query(&wl[v], i as u32, acc, scored));
                    timed!(41, rank_best(scored, args.candidates));
                    merged.extend(scored.iter().map(|&(j, s)| (j, s, v as u8)));
                }
                timed!(41, merge_variants(merged, args.candidates));
                for &(j, _, v) in merged.iter() {
                    let (j, v) = (j as usize, v as usize);
                    timed!(15, index::shared(&wl[v], &lists[j], cands, 60_000));
                    if cands.len() < 3 {
                        continue;
                    }
                    let p = verify::Pair {
                        fa: &vf[v],
                        fb: &items[j].feats,
                        ta: &items[i].thumb,
                        tb: &items[j].thumb,
                    };
                    let verdict = verify::verify(&p, cands, variants[v], gate, matches, scratch);
                    // An anchor, or — for `--dump`, which records every
                    // verdict the geometry could fit, as the direct pass's
                    // does — anything with three aligned points.
                    let anchor = verdict.accepted(&policy.anchor);
                    if anchor || (dumping && verdict.n_in >= 3) {
                        // Every edge runs from the lower index to the higher,
                        // and its verdict with it: stored the other way round,
                        // the report's `scale` for the pair was the reciprocal
                        // of the scale between the files it names.
                        let (lo, hi, verdict) = if i < j {
                            (i, j, verdict)
                        } else {
                            match verdict.reversed() {
                                Some(r) => (j, i, r),
                                None => continue,
                            }
                        };
                        out.push(((lo, hi, verdict.m, variants[v].invert, verdict), anchor));
                    }
                }
                bar.add(w);
                out
            }),
        )
        .flatten()
        .collect();
    let variant_edges: Vec<Edge> = variant_all.iter().filter(|e| e.1).map(|e| e.0.clone()).collect();
    let variant_all: Vec<Edge> = if dumping { variant_all.into_iter().map(|e| e.0).collect() } else { Vec::new() };
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
    timed!(37, release_memory());
    stage!(t_start, "released the index and the descriptors");

    // One edge per pair of files. The mirrored and inverted pass can verify a
    // pair the direct pass already anchored, and two lonely files can each
    // find the other — and a pair counted twice is two links to the bridge
    // test, so a lone match between two clusters stopped being a bridge by
    // being found twice. The direct verdict is kept where there are two.
    let all: Vec<Edge> = unique_pairs(edges().chain(variant_edges.iter().cloned()));

    // Every anchor faces the bridge test, whichever pass produced it: a
    // mirrored match joining two clusters is exactly as consequential as a
    // direct one, and on this corpus an inverted match between a 225-pixel
    // photograph of the Earth and a beach scene was the single edge that
    // merged two whole families into 3,002 false pairs.
    progress.forecast(|f| f.edges = Some(all.len()));
    progress.begin(Stage::Propagate);
    // Clusters are made from clean anchors, and an anchor that is only an
    // anchor may add a file to one but never join two; see `admit_anchors`.
    let (clean, weak): (Vec<Edge>, Vec<Edge>) = all.into_iter().partition(|e| e.4.accepted(&policy.clean));
    let n_before = clean.len();
    let clean = drop_weak_bridges(clean, n);
    stage!(t_start, "bridges: dropped {} lone links between clusters", n_before - clean.len());
    let n_weak = weak.len();
    let (all, refused) = admit_anchors(clean, weak, n);
    stage!(t_start, "weak anchors: {} admitted, {refused} refused between clusters", n_weak - refused);

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
    //
    // A later round proposes again every pair an earlier one rejected, through
    // a tree that may have changed. Each pair is one hypothesis however many
    // rounds considered it, and the dump holds the last verdict on it: two
    // rows for one pair was a dump claiming more than the run had asked.
    let mut all_propagated: Vec<Edge> = Vec::new();
    let mut row_of: HashMap<(usize, usize), usize> = HashMap::new();
    let mut hypotheses: std::collections::HashSet<(usize, usize)> = Default::default();
    // A composed pair is kept only if it clears the propagated tier's own
    // overlap floor; a dump wants every hypothesis the round considered.
    let prop_min_ov = if dumping { policy.propagated.min_frame_overlap.min(0.2) } else { policy.propagated.min_frame_overlap };
    let mut pool: Vec<Edge> = all.clone();
    // Which files' components a round has to propose again: all of them in
    // the first round, and after that only those holding a pair the round
    // before accepted. A component that gained nothing has the same edges, so
    // the same tree and the same poses, and proposes exactly the hypotheses it
    // proposed last time — the accepted ones now known, the rest rejected
    // again. Components never merge here, since a composed pair joins two
    // files already in one.
    let mut dirty: Option<Vec<bool>> = None;
    let mut proposed_total = 0usize;
    // The clusters whose last proposal was a star, by lowest file, with their
    // sizes. A cluster starred in one round is the only one left to propose in
    // the next as often as not, and then fits the budget alone and is compared
    // pair by pair after all — so what to say is decided once the rounds are
    // done, from each cluster's last round, and not from its first. (A cluster
    // keeps its lowest file from round to round: components never merge here.)
    let mut still_starred: std::collections::BTreeMap<usize, usize> = Default::default();
    for round in 0..PROPAGATE_MAX_ROUNDS {
        let Propagation { found: round_all, starred, full, proposed } =
            timed!(21, propagate(&items, &pool, n, prop_min_ov, dirty.as_deref(), prop_budget));
        proposed_total += proposed;
        for first in full {
            still_starred.remove(&first);
        }
        still_starred.extend(starred);
        let before = propagated.len();
        let seen: std::collections::HashSet<(usize, usize)> =
            pool.iter().map(|&(a, b, _, _, _)| (a, b)).collect();
        let fresh: Vec<Edge> = round_all
            .iter()
            .filter(|(a, b, _, _, v)| !seen.contains(&(*a, *b)) && v.accepted(&policy.propagated))
            .cloned()
            .collect();
        hypotheses.extend(round_all.iter().map(|e| (e.0, e.1)));
        if dumping {
            for e in round_all {
                match row_of.get(&(e.0, e.1)) {
                    Some(&k) => all_propagated[k] = e,
                    None => {
                        row_of.insert((e.0, e.1), all_propagated.len());
                        all_propagated.push(e);
                    }
                }
            }
        } else {
            drop(round_all);
        }
        let mut next = vec![false; n];
        for e in fresh.iter() {
            next[e.0] = true;
            next[e.1] = true;
        }
        dirty = Some(next);
        propagated.extend(fresh.iter().cloned());
        pool.extend(fresh);
        stage!(t_start, "  propagation round {}: +{} pairs", round + 1, propagated.len() - before);
        if propagated.len() == before {
            break;
        }
    }

    stage!(
        t_start,
        "propagated: {} of {} composed hypotheses ({proposed_total} put to the pixels, budget {prop_budget} a round)",
        propagated.len(),
        hypotheses.len()
    );
    // Said on the console: pairs between two members of such a cluster that
    // direct matching missed are not looked for, and propagation is most of
    // the tool's recall.
    if !still_starred.is_empty() {
        let files: usize = still_starred.values().sum();
        say!(
            "Note: {} cluster(s) ({files} files in all) are too large to compare every pair inside; their files \
             were compared with the cluster's best-connected file.",
            still_starred.len()
        );
    }
    drop(hypotheses);
    drop(row_of);

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
            .filter(|(a, b, v)| {
                let (a, b) = (*a as usize, *b as usize);
                !anchored.contains(&(a, b)) && v.accepted(&policy.corroborated) && dsu.find(a) == dsu.find(b)
            })
            .map(direct_edge)
            .collect()
    };
    stage!(t_start, "corroborated: {} more pairs inside existing clusters", corroborated.len());

    if let Some(path) = &args.dump {
        // `-` is stdout, which `stdout_has_one_reader` has kept for it alone.
        let sink: Box<dyn Write> = if path == stdout_path() {
            Box::new(std::io::stdout().lock())
        } else {
            Box::new(std::fs::File::create(path).with_context(|| format!("could not create {}", path.display()))?)
        };
        let mut w = std::io::BufWriter::new(sink);
        writeln!(w, "a,b,kind,n_match,n_in,ov_a,ov_b,scale,rot,blk,blk_n,ncc,centred,inverted,blk_min")?;
        let all_direct: Vec<Edge> = all_direct.iter().map(direct_edge).collect();
        for (kind, set) in [("direct", &all_direct), ("variant", &variant_all), ("propagated", &all_propagated)] {
            for (a, b, _, iv, v) in set.iter() {
                // The paths as their own bytes, quoted the way CSV quotes:
                // written with `{:?}`, a name holding a quote or a comma split
                // into two columns, and a tab or a non-UTF-8 byte came out as
                // an escape that names no file.
                use std::os::unix::ffi::OsStrExt;
                let figures = [
                    kind.to_string(),
                    v.n_match.to_string(),
                    v.n_in.to_string(),
                    format!("{:.4}", v.ov_a),
                    format!("{:.4}", v.ov_b),
                    format!("{:.5}", v.scale),
                    format!("{:.1}", v.rot_deg),
                    format!("{:.4}", v.blk),
                    v.blk_n.to_string(),
                    format!("{:.4}", v.ncc),
                    (v.centred as u8).to_string(),
                    (*iv as u8).to_string(),
                    format!("{:.4}", v.blk_min),
                ];
                let mut row: Vec<&[u8]> = vec![files[*a].as_os_str().as_bytes(), files[*b].as_os_str().as_bytes()];
                row.extend(figures.iter().map(|f| f.as_bytes()));
                report::csv_row_with(&mut w, &row, b',')?;
            }
        }
        w.flush()?;
        drop(w);
        if path == stdout_path() {
            say!("dumped verdicts -> stdout");
        } else {
            say!("dumped verdicts -> {}", path.display());
        }
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
    //
    // Every pair so far is between originals. A copy is the same bytes as its
    // original, so each pair is now stated for each copy of either file, with
    // the verdict its original's pair was found on.
    let (all, propagated, corroborated) = (with_copies(&all, &twin_of), with_copies(&propagated, &twin_of), with_copies(&corroborated, &twin_of));
    // Which pairs are stated, and from what, in the order they are first
    // met — the order `graph` and every tie in the sort below keep. The
    // records themselves are a million strings on a large corpus, so they are
    // made afterwards on every thread rather than here on one.
    let mut graph: Vec<(usize, usize)> = Vec::new();
    // `None` for byte-identical files, or the edge and whether it was
    // corroborated.
    let mut stated: Vec<Option<(&Edge, bool)>> = Vec::new();
    let mut seen: std::collections::HashSet<(usize, usize)> = Default::default();
    for g in exact.iter() {
        for w in 0..g.len() {
            for x in w + 1..g.len() {
                let (a, b) = (g[w], g[x]);
                if seen.insert((a, b)) {
                    graph.push((a, b));
                    stated.push(None);
                }
            }
        }
    }
    let tiers = all.iter().map(|e| (e, false)).chain(propagated.iter().map(|e| (e, false)));
    for (e, corroborated) in tiers.chain(corroborated.iter().map(|e| (e, true))) {
        let (a, b) = (e.0, e.1);
        if !seen.insert((a, b)) {
            continue;
        }
        graph.push((a, b));
        stated.push(Some((e, corroborated)));
    }
    drop(seen);
    let names: Vec<String> = files.par_iter().map(|f| f.display().to_string()).collect();
    let mut out_pairs: Vec<OutPair> = graph
        .par_iter()
        .zip(stated.par_iter())
        .map(|(&(a, b), src)| match *src {
            None => OutPair {
                a: names[a].clone(),
                b: names[b].clone(),
                a_bytes: report::raw_bytes(&files[a]),
                b_bytes: report::raw_bytes(&files[b]),
                aligned_points: 0,
                frame_overlap: 1.0,
                pixel_correlation: 1.0,
                scale: 1.0,
                mirrored: false,
                inverted: false,
                identical: true,
                propagated: false,
                corroborated: false,
                ia: a,
                ib: b,
            },
            Some(((_, _, _, inv_flag, v), corroborated)) => OutPair {
                a: names[a].clone(),
                b: names[b].clone(),
                a_bytes: report::raw_bytes(&files[a]),
                b_bytes: report::raw_bytes(&files[b]),
                aligned_points: v.n_in,
                frame_overlap: round3(v.ov_a.max(v.ov_b)),
                pixel_correlation: round3(v.blk),
                scale: round3(file_scale(v.scale, &items[a], &items[b])),
                mirrored: v.m[0] * v.m[4] - v.m[1] * v.m[3] < 0.0,
                inverted: *inv_flag,
                identical: false,
                propagated: v.n_match == 0,
                corroborated,
                ia: a,
                ib: b,
            },
        })
        .collect();
    drop(stated);
    // Stable, as `sort_by` is, so equal names keep the order they were met in.
    out_pairs.par_sort_by(|x, y| x.a.cmp(&y.a).then(x.b.cmp(&y.b)));

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
            rep: g.representative,
            members: g.members.clone(),
        })
        .collect();
    groups.sort_by(|a, b| {
        b.files.len().cmp(&a.files.len()).then_with(|| a.files.cmp(&b.files))
    });

    let failures: Vec<serde_json::Value> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let e = it.err.as_ref().filter(|_| !not_asked[i])?;
            let mut f = serde_json::json!({"path": files[i].display().to_string(), "error": e});
            if let Some(b) = report::raw_bytes(&files[i]) {
                f["path_bytes"] = b.into();
            }
            Some(f)
        })
        .collect();

    // Cleared before anything is written to stdout, which shares the terminal.
    progress.finish();
    let runtime = t_start.elapsed().as_secs_f64();
    let out = Output {
        tool: "img-fp",
        config: run_config(args),
        // A file a wildcard walk reached and that turned out not to be a
        // picture is a skip, like one the extension list turned away, and is
        // not counted by either.
        files_enumerated: files.len() - not_asked.iter().filter(|&&x| x).count(),
        files_analysed: n_ok,
        failures,
        runtime_seconds: (runtime * 1000.0).round() / 1000.0,
        groups,
        pairs: out_pairs,
    };

    let summary = format!(
        "{}, {} over {} in {:.1}s",
        count(out.groups.len(), "group", "groups"),
        count(out.pairs.len(), "pair", "pairs"),
        count(n_ok, "image", "images"),
        runtime
    );
    let dims: Vec<(u32, u32)> = items.iter().map(|it| it.dims).collect();
    match timed!(36, write_reports(args, gui, &out, &files, &dims))? {
        Some(path) => say!("{summary} -> {}", path.display()),
        None => say!("{summary}"),
    }
    prof::report();
    Ok(())
}

/// The settings a report records, which are the ones that decide its pairs.
fn run_config(args: &Args) -> serde_json::Value {
    serde_json::json!({
        "work_size": args.work_size,
        "features": FEATURES,
        "candidates": args.candidates,
        "min_aligned_points": aligned_points(args),
        "min_frame_overlap": as_typed(args.min_frame_overlap),
        "min_pixel_correlation": as_typed(args.min_pixel_correlation),
        "stages": "anchor, propagate, corroborate",
    })
}

/// A threshold as it was typed. The flags are `f32`, and JSON numbers are
/// `f64`: widened as they stand, `0.85` was written `0.8500000238418579`. The
/// shortest form that reads back as the same `f32` is the number the user
/// gave, and as an `f64` it prints as that.
fn as_typed(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(v as f64)
}

/// Write the report where `-o` says, and the window's copy of it; the file
/// the report went to, if it went to one.
///
/// Under the window, stdout is the progress channel and the report goes there
/// only if one was asked for by name.
///
/// The window's copy is written first. It is what the window reads to show
/// the scan at all, and a `-o` that failed — a disk full, a folder removed
/// mid-scan — used to end the run before it, so a finished scan showed as a
/// failed one with nothing to look at. The files' facts are read once for
/// both.
/// `dims` is each file's size as the analysis found it, indexed like `files`;
/// see `report::read_facts`.
fn write_reports(args: &Args, gui: Option<&Path>, out: &Output, files: &[PathBuf], dims: &[(u32, u32)]) -> Result<Option<PathBuf>> {
    let target = report::Target::of(args.output.as_deref(), args.format);
    let asked = gui.is_none() || args.output.as_deref().is_some_and(|p| p != stdout_path());
    let facts = report::read_facts(out, files, dims);
    if let Some(path) = gui {
        let target = report::Target { sink: report::Sink::File(path.to_path_buf()), format: report::Format::Json, pairs: false };
        report::write_with(&target, out, files, &facts)?;
    }
    let mut went = None;
    if asked {
        report::write_with(&target, out, files, &facts)?;
        if let report::Sink::File(path) = &target.sink {
            went = Some(path.clone());
        }
    }
    Ok(went)
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

/// `n` and the noun that goes with it: "1 image", "2 images".
fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn stdout_path() -> &'static Path {
    Path::new("-")
}

/// A verdict's scale, which is the analysis images', as the files' own.
///
/// A transform is fitted between working images, and every picture is
/// analysed at about `--work-size` on its long side however large the file
/// is: a photograph and a copy shrunk to a third of it are both 512 pixels
/// there, and a crop of half the photograph is too. So the verdict's scale
/// said 0.99 for the copy a third the size and 0.5 for the crop that has the
/// photograph's own pixels. Measured in each file's pixels instead — each
/// working image's long side over its file's — it is what a person reading
/// "scale" expects: how many pixels of `b` one pixel of `a` spans.
fn file_scale(scale: f32, a: &Item, b: &Item) -> f32 {
    let shrink = |it: &Item| {
        let file = it.dims.0.max(it.dims.1);
        (file > 0).then(|| it.feats.w.max(it.feats.h) as f32 / file as f32)
    };
    match (shrink(a), shrink(b)) {
        (Some(sa), Some(sb)) if sb > 0.0 => scale * sa / sb,
        _ => scale,
    }
}

fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

/// `edges`, which run between originals, stated for every copy of either end
/// as well: an edge from `a` to `b` becomes one from each file sharing `a`'s
/// bytes to each file sharing `b`'s.
///
/// The verdict is the originals', which is the verdict the copies would have
/// been given, since their analysis is the originals' own. Every edge still
/// runs from the lower index to the higher, so a copy on the other side of
/// the other end takes the verdict reversed — its scale is then the scale
/// between the files it names.
fn with_copies(edges: &[(usize, usize, Affine, bool, Verdict)], twin_of: &[usize]) -> Vec<(usize, usize, Affine, bool, Verdict)> {
    let mut copies: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &o) in twin_of.iter().enumerate() {
        if o != i {
            copies.entry(o).or_insert_with(|| vec![o]).push(i);
        }
    }
    if copies.is_empty() {
        return edges.to_vec();
    }
    let mut out = Vec::with_capacity(edges.len());
    for (a, b, m, inv, v) in edges.iter() {
        let one = |x: &usize| std::slice::from_ref(x).to_vec();
        let (sa, sb) = (copies.get(a).cloned().unwrap_or_else(|| one(a)), copies.get(b).cloned().unwrap_or_else(|| one(b)));
        for &x in sa.iter() {
            for &y in sb.iter() {
                if (x < y) == (a < b) {
                    out.push((x, y, *m, *inv, v.clone()));
                } else {
                    match v.reversed() {
                        Some(r) => out.push((y, x, r.m, *inv, r)),
                        None => out.push((x, y, *m, *inv, v.clone())),
                    }
                }
            }
        }
    }
    out
}

/// The first edge for each pair of files, in the order given.
fn unique_pairs(edges: impl IntoIterator<Item = (usize, usize, Affine, bool, Verdict)>) -> Vec<(usize, usize, Affine, bool, Verdict)> {
    let mut seen = std::collections::HashSet::new();
    edges.into_iter().filter(|e| seen.insert((e.0.min(e.1), e.0.max(e.1)))).collect()
}

/// Add the anchors that are not clean to the clusters the clean ones make.
/// Two clusters join on weak anchors only when **every file of the smaller one
/// has a weak anchor into the larger**; the weak anchors kept are the ones
/// inside a cluster once no more joins are possible. The edges kept, and how
/// many weak ones were refused.
///
/// **This is the bridge test's argument, carried from one edge to several.**
/// A family is held together by many clean matches — the re-encodes, the
/// resizes, the crops of one photograph agree with each other everywhere —
/// and two families of photographs of one scene, taken a moment or a step
/// apart, touch only through weak ones: a dozen aligned points and a patch of
/// the overlap that does not agree, where someone moved or the background
/// shifted with the viewpoint. Each such match is one of hundreds between the
/// two families' copies, so there are always several, none is a bridge, and
/// the bridge test kept them all. On IMGS2 four families merged that way at
/// the shipped settings, for 21,458 false pairs.
///
/// **Why every file, and not "never join two clusters".** The stricter rule
/// was tried first and cut families apart: two variants of one photograph
/// that match each other cleanly and the family only weakly — a perspective
/// warp and a photo of a screen, a tiny embed and a contact sheet — make a
/// cluster of two, and a low-contrast family breaks into many. It cost IMGS
/// four points of recall and 43 of 45 perfect transformations. A fragment of
/// a family is matched, file by file, by the family it came from; a family of
/// ninety files is not matched file by file by the photograph taken a moment
/// later. And a lone file is the case of one: its one file has an anchor, so
/// it joins, strongest first, the cluster it matches best — after which a
/// weak anchor into a second cluster is a join of two clusters like any other.
///
/// **And never two clusters that each close a cycle of clean anchors.** That
/// a family is not matched file by file by its sibling was an expectation,
/// and two photographs of one scene in Segovia broke it: eighty files
/// of one, every one with a weak anchor into the other's eighty-five, and
/// sixty-six of those pointing back. Coverage cannot tell that from a
/// fragment joining its family; what can is the fragment's own evidence. A
/// fragment's clean anchors are a pair, a chain or a star, each link the only
/// one; a family's reach a file by two paths, which is the corroboration the
/// bridge test asks of a single link. Two clusters that each have it are two
/// families, whatever the weak anchors between them say — unless **one file of
/// the larger has a weak anchor to every file of the smaller**. A greyscale, a
/// duotone and a halftone of one photograph agree with each other cleanly and
/// close a triangle, and the original matches all three; no photograph of
/// Segovia was matched by all eighty of the other's. It is the cover test
/// once more, asked of a single file: the smaller cluster is then renderings
/// of something the larger holds.
fn admit_anchors(
    clean: Vec<(usize, usize, Affine, bool, Verdict)>,
    mut weak: Vec<(usize, usize, Affine, bool, Verdict)>,
    n: usize,
) -> (Vec<(usize, usize, Affine, bool, Verdict)>, usize) {
    let mut dsu = Dsu::new(n);
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    // Whether a cluster's clean anchors close a cycle, by root.
    let mut cycle = vec![false; n];
    fn join(dsu: &mut Dsu, members: &mut [Vec<usize>], cycle: &mut [bool], a: usize, b: usize) {
        let (mut ra, mut rb) = (dsu.find(a), dsu.find(b));
        if ra == rb {
            return;
        }
        if members[ra].len() > members[rb].len() {
            std::mem::swap(&mut ra, &mut rb);
        }
        dsu.0[ra] = rb;
        let moved = std::mem::take(&mut members[ra]);
        members[rb].extend(moved);
        cycle[rb] |= cycle[ra];
    }
    for e in clean.iter() {
        let (ra, rb) = (dsu.find(e.0), dsu.find(e.1));
        if ra == rb {
            cycle[ra] = true;
        }
        join(&mut dsu, &mut members, &mut cycle, e.0, e.1);
    }
    weak.sort_by(|x, y| {
        y.4.n_in.cmp(&x.4.n_in).then(y.4.blk.total_cmp(&x.4.blk)).then((x.0, x.1).cmp(&(y.0, y.1)))
    });
    let mut near: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in weak.iter() {
        near[e.0].push(e.1);
        near[e.1].push(e.0);
    }
    // Clusters only grow, so a join refused now can be granted after another
    // has made one side larger; the pass repeats until nothing joins.
    // Cluster pairs already found wanting, forgotten at every join since a
    // join is what can change the answer.
    let mut refused_pairs: std::collections::HashSet<(usize, usize)> = Default::default();
    loop {
        let mut joined = false;
        for e in weak.iter() {
            let (ra, rb) = (dsu.find(e.0), dsu.find(e.1));
            if ra == rb {
                continue;
            }
            let (small, big) = if members[ra].len() <= members[rb].len() { (ra, rb) } else { (rb, ra) };
            if refused_pairs.contains(&(small, big)) {
                continue;
            }
            let covered = members[small].iter().all(|&x| near[x].iter().any(|&y| dsu.find(y) == big));
            // Two clusters with clean cycles join only when one file of the
            // larger has a weak anchor to every file of the smaller: the
            // smaller is then renderings of a photograph the larger holds.
            // A weak edge is listed once, so a count is a cover.
            let admitted = covered
                && (!(cycle[ra] && cycle[rb]) || {
                    let k = members[small].len();
                    members[big].iter().any(|&y| near[y].iter().filter(|&&z| dsu.find(z) == small).count() == k)
                });
            if admitted {
                join(&mut dsu, &mut members, &mut cycle, small, big);
                refused_pairs.clear();
                joined = true;
            } else {
                refused_pairs.insert((small, big));
            }
        }
        if !joined {
            break;
        }
    }
    let mut out = clean;
    let mut refused = 0;
    for e in weak {
        if dsu.find(e.0) == dsu.find(e.1) {
            out.push(e);
        } else {
            refused += 1;
        }
    }
    (out, refused)
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

/// What a round of propagation proposed, and how.
struct Propagation {
    /// Every composed hypothesis that cleared the overlap floor, verdict and all.
    found: Vec<(usize, usize, Affine, bool, Verdict)>,
    /// The components the round could not afford to compare pair by pair,
    /// and compared with their root alone, each as its lowest file and its
    /// size; see `propagate`.
    starred: Vec<(usize, usize)>,
    /// The lowest file of each component the round compared pair by pair.
    full: Vec<usize>,
    /// Composed hypotheses put to the pixels.
    proposed: usize,
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
///
/// **What a round may spend is `budget` hypotheses**, and the caller makes that
/// the number of candidate pairs the direct pass verified. Every pair inside a
/// component is quadratic in it, and everything else in a run is linear in
/// files times `-k`; a budget in those units keeps propagation the same order
/// of cost as the matching it extends, whatever one family's size. Components
/// are taken cheapest first, so small families — nearly all of them — are
/// never touched by it, and one that does not fit is not skipped but
/// **starred**: each of its files is compared with the root alone, which is
/// linear in the component. That is what a group is anyway — a
/// representative and what matched it, and the root is the best-connected
/// file, which is what `group::find` elects — so a starred family still comes
/// out as one group; what it gives up is the pairs between two non-root
/// members that direct matching missed.
///
/// It replaces a cap of two thousand files, in the first commit and never
/// derived, past which a component was not propagated at all. A family of
/// 2,100 variants of one photograph came out at a tenth of its pairs and as 26
/// overlapping groups, where 2,000 would have had every pair.
///
/// **The work is spread by row, not by component.** A component used to be one
/// task, so one large family ran on one thread while the rest sat idle — the
/// same 2,100 files took 76 CPU-seconds in 70 seconds of wall. A unit of work
/// is now one row of a component's pair triangle, or one pair of a star, and
/// the units come back in component order and row order, which is the order
/// the per-component loop produced them in.
///
/// `dirty`, when given, limits the work to the components holding a file it
/// marks; see the round loop in `run`.
fn propagate(
    items: &[Item],
    edges: &[(usize, usize, Affine, bool, Verdict)],
    n: usize,
    min_ov: f32,
    dirty: Option<&[bool]>,
    budget: usize,
) -> Propagation {
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
    // Pairs each component already holds, so that what a component would cost
    // is the pairs it would actually propose.
    let mut inside: HashMap<usize, usize> = HashMap::new();
    for &(a, _) in known.iter() {
        *inside.entry(dsu.find(a)).or_default() += 1;
    }

    // Cheapest first, ties to the lowest file, so the plan does not depend on
    // the order a hash map hands the components over in.
    let mut costed: Vec<(usize, Vec<usize>)> = comps
        .into_iter()
        .filter(|(_, c)| c.len() > 2)
        .filter(|(_, c)| dirty.is_none_or(|d| c.iter().any(|&i| d[i])))
        .map(|(r, mut c)| {
            c.sort_unstable();
            let k = c.len();
            (k * (k - 1) / 2 - inside.get(&r).copied().unwrap_or(0).min(k * (k - 1) / 2), c)
        })
        .collect();
    costed.sort_unstable_by(|x, y| x.0.cmp(&y.0).then(x.1[0].cmp(&y.1[0])));
    let mut spent = 0usize;
    let (mut starred, mut full) = (Vec::new(), Vec::new());
    let plan: Vec<(Vec<usize>, bool)> = costed
        .into_iter()
        .map(|(cost, c)| {
            let every_pair = spent + cost <= budget;
            if every_pair {
                spent += cost;
                full.push(c[0]);
            } else {
                starred.push((c[0], c.len()));
            }
            (c, every_pair)
        })
        .collect();

    // Pose of every member relative to its component's root, by position in
    // the component.
    //
    // Breadth-first, from the best-connected member. Every hop composes
    // another transform and carries its error into the result, so the thing
    // to minimise is the number of hops, not the quality of each one: growing
    // the tree best-edge-first was tried and is measurably worse, because it
    // trades short paths for slightly better links and ends up composing more
    // of them. The root is the file most others matched, which is usually the
    // original or a clean re-encode of it.
    let posed: Vec<(usize, Vec<Option<(Affine, bool)>>)> = plan
        .par_iter()
        .map(|(comp, _)| {
            let root = *comp.iter().max_by_key(|&&i| (adj[i].len(), std::cmp::Reverse(i))).unwrap();
            let at = |v: usize| comp.binary_search(&v).ok();
            let mut pose: Vec<Option<(Affine, bool)>> = vec![None; comp.len()];
            pose[at(root).unwrap()] = Some(([1.0, 0.0, 0.0, 0.0, 1.0, 0.0], false));
            let mut queue = std::collections::VecDeque::from([root]);
            while let Some(u) = queue.pop_front() {
                let (mu, iu) = pose[at(u).unwrap()].unwrap();
                for &(v, m, inv, _) in adj[u].iter() {
                    let Some(pv) = at(v) else { continue };
                    if pose[pv].is_some() {
                        continue;
                    }
                    pose[pv] = Some((verify::compose(&mu, &m), iu ^ inv));
                    queue.push_back(v);
                }
            }
            (root, pose)
        })
        .collect();

    // One unit per row of a component pair by pair, one per member of a star.
    let units: Vec<(u32, u32)> =
        plan.iter().enumerate().flat_map(|(ci, (c, _))| (0..c.len() as u32).map(move |ai| (ci as u32, ai))).collect();
    // `a -> root -> b`, put to the pixels, and kept against the floor the
    // caller asked for — the propagated tier's own, or a dump's lower one —
    // and no other. This used to be a fixed 0.5, which quietly overruled a
    // `--min-frame-overlap` below it for every composed pair, and threw away
    // the dump's hypotheses between 0.2 and 0.5.
    let test = |comp: &[usize], pose: &[Option<(Affine, bool)>], x: usize, y: usize| {
        let (a, b) = (comp[x], comp[y]);
        let ((ma, ia), (mb, ib)) = (pose[x]?, pose[y]?);
        let m = verify::compose(&verify::invert_affine(&ma)?, &mb);
        let var = Variant { mirror: false, invert: ia ^ ib };
        let p = verify::Pair { fa: &items[a].feats, fb: &items[b].feats, ta: &items[a].thumb, tb: &items[b].thumb };
        let v = verify::verify_transform(&p, &m, var, min_ov);
        (v.ov_a.max(v.ov_b) >= min_ov).then_some((a, b, m, var.invert, v))
    };
    let proposed = AtomicUsize::new(0);
    let found = units
        .par_iter()
        .flat_map_iter(|&(ci, x)| {
            let ((comp, every_pair), (root, pose)) = (&plan[ci as usize], &posed[ci as usize]);
            let x = x as usize;
            let mut out = Vec::new();
            let mut asked = 0;
            if *every_pair {
                for y in x + 1..comp.len() {
                    if !known.contains(&(comp[x], comp[y])) {
                        asked += 1;
                        out.extend(test(comp, pose, x, y));
                    }
                }
            } else if comp[x] != *root {
                let r = comp.binary_search(root).unwrap();
                let (lo, hi) = (x.min(r), x.max(r));
                if !known.contains(&(comp[lo], comp[hi])) {
                    asked += 1;
                    out.extend(test(comp, pose, lo, hi));
                }
            }
            proposed.fetch_add(asked, Ordering::Relaxed);
            out
        })
        .collect();
    Propagation { found, starred, full, proposed: proposed.into_inner() }
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

    /// The words a corpus really gets are bounded by the sample the tree is
    /// trained on, so the sample follows the corpus: a tenth of it, up to the
    /// cap. A fixed sample held the live words constant and let descriptors
    /// per word climb with the library towards the size that merges families.
    #[test]
    fn the_vocabulary_sample_follows_the_corpus() {
        use crate::index::VocabParams;
        let sample = |n| VocabParams::for_corpus(n).sample;
        assert_eq!(sample(1_447_607), 144_760, "IMGS at the default work size: about the old 160,000");
        assert_eq!(sample(5_367_534), 536_753);
        assert_eq!(sample(90_000_000), 1_280_000, "capped");
        assert_eq!(sample(7), 1);
        // Descriptors per sample are held from a folder to the cap.
        for n in [20_000usize, 1_447_607, 5_367_534, 12_000_000] {
            assert_eq!(n / sample(n), 10, "n={n}");
        }
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

    /// `rank_best` selects on integer keys when it can; it has to choose the
    /// same candidates in the same order as the float comparator, ties and
    /// all, and fall back to the comparator for anything a key cannot carry.
    #[test]
    fn rank_best_keys_order_as_the_comparator_does() {
        let cmp = |a: &(u32, f32), b: &(u32, f32)| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0));
        let mut x = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for n in [0usize, 1, 5, 149, 150, 151, 400, 3000] {
            for levels in [3u64, 50, 1 << 20] {
                let mut scored: Vec<(u32, f32)> = (0..n as u32).map(|j| (j, 1e-3 + (next() % levels) as f32 * 0.37)).collect();
                // Shuffled, since a query now hands them over in index order.
                for i in (1..scored.len()).rev() {
                    scored.swap(i, (next() % (i as u64 + 1)) as usize);
                }
                for k in [0usize, 1, 7, 150] {
                    let mut want = scored.clone();
                    want.sort_by(cmp);
                    want.truncate(k);
                    let mut got = scored.clone();
                    super::rank_best(&mut got, k);
                    assert_eq!(got.iter().map(|e| (e.0, e.1.to_bits())).collect::<Vec<_>>(), want.iter().map(|e| (e.0, e.1.to_bits())).collect::<Vec<_>>(), "n {n} levels {levels} k {k}");
                }
            }
        }
        // A score no key can carry goes to the comparator.
        let mut got = vec![(3u32, 0.0f32), (1, 2.0), (2, 2.0)];
        super::rank_best(&mut got, 2);
        assert_eq!(got, vec![(1, 2.0), (2, 2.0)]);
    }

    /// The hash read a piece at a time is the hash of the whole: every length
    /// around a piece boundary, a word boundary, and none at all.
    #[test]
    fn a_hash_read_in_pieces_is_the_hash_of_the_whole() {
        fn whole(data: &[u8]) -> u128 {
            let mut h: u128 = 0x6c62272e07bb0142_62b821756295c58d;
            for chunk in data.chunks(8) {
                let mut v = 0u64;
                for (i, &b) in chunk.iter().enumerate() {
                    v |= (b as u64) << (i * 8);
                }
                h ^= v as u128;
                h = h.wrapping_mul(0x0000000001000000_000000000000013B);
            }
            h ^ data.len() as u128
        }
        let data: Vec<u8> = (0..300u32).map(|i| (i * 131 % 251) as u8).collect();
        for len in [0usize, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 100, 300] {
            for piece in [8usize, 16, 24, 64, 1 << 20] {
                assert_eq!(hash_reader(&data[..len], piece), Some(whole(&data[..len])), "len {len} piece {piece}");
            }
        }
        // A reader that hands over a byte at a time still fills each piece.
        struct Trickle<'a>(&'a [u8]);
        impl std::io::Read for Trickle<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let Some((&b, rest)) = self.0.split_first() else { return Ok(0) };
                if buf.is_empty() {
                    return Ok(0);
                }
                buf[0] = b;
                self.0 = rest;
                Ok(1)
            }
        }
        assert_eq!(hash_reader(Trickle(&data), 16), Some(whole(&data)));
    }

    /// Files sharing a hash are grouped only when their bytes agree. The two
    /// streams here are built to collide under the hash — a change in bits
    /// 40 to 55 of one word, cancelled by the next — and are not identical.
    #[test]
    fn a_hash_collision_is_not_an_identical_pair() {
        let a: Vec<u8> = (0..64u32).map(|i| (i * 37 % 251) as u8).collect();
        let mut b = a.clone();
        // The state before word 2, by the same arithmetic as `hash_reader`.
        let mut h: u128 = 0x6c62272e07bb0142_62b821756295c58d;
        let p: u128 = 0x0000000001000000_000000000000013B;
        let word = |d: &[u8], k: usize| u64::from_le_bytes(d[k * 8..k * 8 + 8].try_into().unwrap());
        for k in 0..2 {
            h = (h ^ word(&a, k) as u128).wrapping_mul(p);
        }
        let wa = word(&a, 2);
        let wb = wa ^ (0x1234u64 << 40);
        let (ha, hb) = ((h ^ wa as u128).wrapping_mul(p), (h ^ wb as u128).wrapping_mul(p));
        assert_eq!((ha ^ hb) >> 64, 0, "the difference stays in the low half");
        b[16..24].copy_from_slice(&wb.to_le_bytes());
        b[24..32].copy_from_slice(&(word(&a, 3) ^ (ha ^ hb) as u64).to_le_bytes());
        assert_ne!(a, b);
        assert_eq!(hash_reader(&a[..], 8), hash_reader(&b[..], 8), "a collision, as built");
        assert_eq!(same_stream(&a[..], &b[..], 16), Some(false));
        assert_eq!(same_stream(&a[..], &a[..], 16), Some(true));
        assert_eq!(same_stream(&a[..], &a[..63], 16), Some(false));
        assert_eq!(same_stream(&a[..48], &a[..48], 16), Some(true), "a length that ends on a piece");
    }

    /// Same-size files are grouped exactly when their bytes agree, whatever the
    /// sample said: files that differ only between the sampled pieces, three
    /// copies of one file, a pair, and files short enough to be hashed whole.
    #[test]
    fn identical_files_are_found_whatever_the_sample_says() {
        let dir = std::env::temp_dir().join(format!("img-fp-exact-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big: Vec<u8> = (0..300_000u32).map(|i| (i * 31 % 251) as u8).collect();
        let mut middle = big.clone();
        // Between the start and the first third, where no piece is read.
        middle[50_000] ^= 1;
        let small = b"short file".to_vec();
        let mut small_other = small.clone();
        small_other[3] = b'X';
        let named: Vec<(&str, &[u8])> = vec![
            ("a", &big), ("a_copy1", &big), ("a_copy2", &big), ("a_middle", &middle),
            ("s", &small), ("s_copy", &small), ("s_other", &small_other),
        ];
        let files: Vec<PathBuf> = named
            .iter()
            .map(|(n, b)| {
                let p = dir.join(n);
                std::fs::write(&p, b).unwrap();
                p
            })
            .collect();
        assert_eq!(exact_groups(&files), vec![vec![0, 1, 2], vec![4, 5]]);
        // A pair alone is compared straight away.
        assert_eq!(exact_groups(&[files[0].clone(), files[3].clone()]), Vec::<Vec<usize>>::new());
        assert_eq!(exact_groups(&[files[0].clone(), files[1].clone()]), vec![vec![0, 1]]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two outputs never share stdout, and under the window neither the dump
    /// nor the log may use it at all.
    #[test]
    fn stdout_carries_one_output() {
        let check = |extra: &[&str], gui: bool| {
            let args = Args::try_parse_from(["img-fp"].iter().chain(extra).chain(&["."])).unwrap();
            stdout_has_one_reader(&args, gui.then_some(Path::new("/r.json"))).is_ok()
        };
        assert!(check(&[], false));
        assert!(check(&["-o", "r.json", "--dump", "-"], false));
        assert!(check(&["-o", "r.json", "--log-file", "-"], false));
        assert!(!check(&["--dump", "-"], false), "the report is on stdout already");
        assert!(!check(&["-o", "-", "--log-file", "-"], false));
        assert!(!check(&["-o", "r.json", "--dump", "-", "--log-file", "-"], false));
        assert!(check(&["--dump", "./-"], false), "a file named - is ./-");
        assert!(check(&[], true), "the window's report is a file");
        assert!(!check(&["--dump", "-"], true));
    }

    /// A named cache beside `--no-cache` is the file `--clear-cache` deletes,
    /// and without `--clear-cache` it is refused.
    #[test]
    fn a_named_cache_beside_no_cache_is_only_for_clearing() {
        let args = |extra: &[&str]| Args::try_parse_from(["img-fp"].iter().chain(extra).chain(&["."])).unwrap();
        assert!(validate(&args(&["--cache", "/x.bin"])).is_ok());
        assert!(validate(&args(&["--cache", "/x.bin", "--no-cache"])).is_err());
        assert!(validate(&args(&["--cache", "/x.bin", "--no-cache", "--clear-cache"])).is_ok());
        assert!(check_args(["img-fp", "--cache", "/x.bin", "--no-cache", "."]).is_err());
    }

    /// Two outputs on one file are refused before anything is written.
    #[test]
    fn two_outputs_on_one_file_are_refused() {
        let args = |extra: &[&str]| Args::try_parse_from(["img-fp"].iter().chain(extra).chain(&["."])).unwrap();
        let dir = std::env::temp_dir().join(format!("img-fp-outputs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (r, l) = (dir.join("r.txt"), dir.join("l.txt"));
        let (r, l) = (r.to_str().unwrap(), l.to_str().unwrap());
        assert!(outputs_are_distinct(&args(&["-o", r, "--log-file", l])).is_ok());
        assert!(outputs_are_distinct(&args(&["-o", r, "--log-file", r])).is_err());
        assert!(outputs_are_distinct(&args(&["-o", r, "--dump", r])).is_err());
        assert!(outputs_are_distinct(&args(&["--dump", r, "--log-file", r, "-o", l])).is_err());
        assert!(outputs_are_distinct(&args(&["-o", "-", "--log-file", r])).is_ok());
        // Nor the cache, while the run uses it.
        assert!(outputs_are_distinct(&args(&["-o", r, "--cache", r])).is_err());
        assert!(outputs_are_distinct(&args(&["--log-file", l, "--cache", l])).is_err());
        assert!(outputs_are_distinct(&args(&["-o", r, "--cache", l])).is_ok());
        assert!(outputs_are_distinct(&args(&["-o", r, "--cache", r, "--no-cache", "--clear-cache"])).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A lone link between two clusters is a bridge however many times the
    /// run found it. Two passes verifying the same pair used to be two links,
    /// and the bridge test then kept a match it exists to drop.
    #[test]
    fn a_pair_found_twice_is_still_one_bridge() {
        let id: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let direct = Verdict { n_in: 40, ..Default::default() };
        let mirrored = Verdict { n_in: 12, ..Default::default() };
        // Two triangles {0,1,2} and {3,4,5}, joined only by 2-3: once from
        // the direct pass and once more, the other way round, from the second
        // look.
        let mut edges: Vec<(usize, usize, Affine, bool, Verdict)> =
            [(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5), (2, 3)].iter().map(|&(a, b)| (a, b, id, false, direct.clone())).collect();
        edges.push((3, 2, id, false, mirrored));
        let unique = unique_pairs(edges);
        assert_eq!(unique.len(), 7);
        assert_eq!(unique[6].4.n_in, 40, "the first verdict for a pair is the one kept");
        let kept = drop_weak_bridges(unique, 6);
        assert!(!kept.iter().any(|e| (e.0, e.1) == (2, 3)), "the lone link is dropped");
        assert_eq!(kept.len(), 6);
    }

    /// A pair between originals is stated for every copy of either, each
    /// running from the lower index to the higher with the verdict turned to
    /// match — and a run with no copies is left exactly as it was.
    #[test]
    fn a_pair_is_stated_for_every_copy_of_either_file() {
        let m: Affine = [2.0, 0.0, 5.0, 0.0, 2.0, 7.0];
        let v = Verdict { m, n_in: 30, ov_a: 0.25, ov_b: 1.0, scale: 2.0, ..Default::default() };
        let edges = vec![(1usize, 3usize, m, false, v)];
        // 0 and 4 are copies of 1; 2 is a copy of 3.
        let twin_of = vec![1, 1, 3, 3, 1];
        let out = with_copies(&edges, &twin_of);
        let mut names: Vec<(usize, usize)> = out.iter().map(|e| (e.0, e.1)).collect();
        names.sort_unstable();
        assert_eq!(names, [(0, 2), (0, 3), (1, 2), (1, 3), (2, 4), (3, 4)]);
        for (a, b, em, _, ev) in out.iter() {
            assert!(a < b);
            // Files sharing 1's bytes are the small side: scale 2 from them.
            let from_one = twin_of[*a] == 1;
            assert_eq!(ev.scale, if from_one { 2.0 } else { 0.5 }, "{a}-{b}");
            assert_eq!(*em, ev.m);
            assert_eq!((ev.ov_a, ev.ov_b), if from_one { (0.25, 1.0) } else { (1.0, 0.25) });
        }
        let alone = with_copies(&edges, &[0, 1, 2, 3]);
        assert_eq!(alone.len(), 1);
        assert_eq!((alone[0].0, alone[0].1), (1, 3));
    }

    /// Weak anchors join a cluster only when every file of the smaller side has
    /// one into the larger: a lone file and a family's fragment join, a group
    /// with a file that does not vouch for the join does not.
    #[test]
    fn weak_anchors_join_clusters_only_when_every_file_vouches() {
        let id: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let e = |a: usize, b: usize, n_in: u32| (a, b, id, false, Verdict { n_in, ..Default::default() });
        // Two clean clusters, {0,1,2,7} and {3,4,5}.
        let clean = vec![e(0, 1, 50), e(1, 2, 50), e(2, 7, 50), e(3, 4, 50), e(4, 5, 50)];
        // Weak links between them from three files of each, a lone file 6
        // matching both (the stronger into the second), a weak link inside
        // the first, and a fragment {8,9} of the second family, clean between
        // its two files and weakly matched by the family from both.
        let weak = vec![e(2, 3, 12), e(1, 4, 11), e(0, 6, 11), e(3, 6, 15), e(0, 2, 10), e(3, 8, 11), e(4, 9, 11)];
        let clean = [clean, vec![e(8, 9, 50)]].concat();
        let (kept, refused) = admit_anchors(clean, weak, 10);
        let mut pairs: Vec<(usize, usize)> = kept.iter().map(|x| (x.0, x.1)).collect();
        pairs.sort_unstable();
        // 7 has no weak anchor into the second cluster, so the first cluster
        // never joins it however many of its other files do.
        assert_eq!(pairs, [(0, 1), (0, 2), (1, 2), (2, 7), (3, 4), (3, 6), (3, 8), (4, 5), (4, 9), (8, 9)]);
        assert_eq!(refused, 3);
    }

    /// Two clusters whose clean anchors each close a cycle are two families,
    /// and weak anchors do not join them even file for file in both
    /// directions — unless one file vouches for the whole of the smaller; a
    /// chain matched the same way still joins.
    #[test]
    fn weak_anchors_join_two_clusters_with_clean_cycles_only_through_one_file() {
        let id: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let e = |a: usize, b: usize, n_in: u32| (a, b, id, false, Verdict { n_in, ..Default::default() });
        // Triangles {0,1,2} and {3,4,5}; a chain {6,7,8}.
        let clean = vec![e(0, 1, 50), e(1, 2, 50), e(0, 2, 50), e(3, 4, 50), e(4, 5, 50), e(3, 5, 50), e(6, 7, 50), e(7, 8, 50)];
        // Every file of each triangle has a weak anchor into the other, and
        // every file of the chain one into the first triangle.
        let weak = vec![e(0, 3, 12), e(1, 4, 12), e(2, 5, 12), e(6, 0, 11), e(7, 1, 11), e(8, 2, 11)];
        let (kept, refused) = admit_anchors(clean, weak, 9);
        let mut pairs: Vec<(usize, usize)> = kept.iter().map(|x| (x.0, x.1)).collect();
        pairs.sort_unstable();
        assert_eq!(pairs, [(0, 1), (0, 2), (1, 2), (3, 4), (3, 5), (4, 5), (6, 0), (6, 7), (7, 1), (7, 8), (8, 2)]);
        assert_eq!(refused, 3);
        // One file of the first triangle matching every file of the second,
        // as an original matches its renderings, does join them.
        // The first is given a fourth file so that it is the larger.
        let clean = vec![e(0, 1, 50), e(1, 2, 50), e(0, 2, 50), e(2, 6, 50), e(3, 4, 50), e(4, 5, 50), e(3, 5, 50)];
        let weak = vec![e(0, 3, 12), e(0, 4, 12), e(0, 5, 12)];
        let (kept, refused) = admit_anchors(clean, weak, 7);
        assert_eq!((kept.len(), refused), (10, 0));
    }

    /// A round proposes every unmatched pair of a component it can afford,
    /// cheapest component first, and compares the files of one it cannot with
    /// the root alone — it never passes one over.
    #[test]
    fn a_component_over_the_budget_is_starred_not_skipped() {
        let id: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        // A chain over `files`, which leaves every pair but the links to ask.
        let chain = |files: std::ops::Range<usize>| {
            files.clone().skip(1).map(|b| (b - 1, b, id, false, Verdict::default())).collect::<Vec<_>>()
        };
        let n = 30;
        let items: Vec<Item> = (0..n).map(|_| Item::default()).collect();
        // {0..10}: 45 pairs, 9 known, 36 to ask. {10..30}: 190 pairs, 19 known, 171.
        let edges = [chain(0..10), chain(10..30)].concat();
        let p = propagate(&items, &edges, n, 0.0, None, 1_000);
        assert_eq!((p.proposed, p.found.len(), p.starred.len()), (36 + 171, 36 + 171, 0));
        // A budget for the small one alone: the large one is starred — its
        // root is the second file of the chain, the first of the best
        // connected, and it is asked about the 18 files it has no link to.
        let p = propagate(&items, &edges, n, 0.0, None, 100);
        assert_eq!(p.starred, vec![(10, 20)]);
        assert_eq!(p.full, vec![0]);
        assert_eq!(p.proposed, 36 + 17);
        assert!(p.found.iter().filter(|e| e.0 >= 10).all(|e| e.0 == 11 || e.1 == 11), "a star is all through its root");
        // No budget at all: both are starred, and still asked.
        let p = propagate(&items, &edges, n, 0.0, None, 0);
        assert_eq!((p.starred.len(), p.proposed), (2, 7 + 17));
        // Starred in one round, and alone in the next, it fits the budget and
        // is compared pair by pair: which is why the run's note is decided by
        // a cluster's last round and not its first.
        let p1 = propagate(&items, &edges, n, 0.0, None, 180);
        assert_eq!(p1.starred, vec![(10, 20)]);
        let mut dirty = vec![false; n];
        for e in &p1.found {
            dirty[e.0] = true;
            dirty[e.1] = true;
        }
        let pool = [edges.clone(), p1.found.clone()].concat();
        let p2 = propagate(&items, &pool, n, 0.0, Some(&dirty), 180);
        assert!(p2.starred.is_empty() && p2.full.contains(&10), "{:?} {:?}", p2.starred, p2.full);
        // The order is the plan's, whatever order the work finished in.
        let again = propagate(&items, &edges, n, 0.0, None, 1_000);
        let pairs = |p: &Propagation| p.found.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>();
        assert_eq!(pairs(&again), pairs(&propagate(&items, &edges, n, 0.0, None, 1_000)));
        let mut sorted = pairs(&again);
        sorted.sort_unstable();
        assert_eq!(pairs(&again), sorted, "component by component, row by row");
    }

    /// Values the thresholds are not measured on are refused by the parser,
    /// not run and reported as a clean folder.
    #[test]
    fn thresholds_outside_their_range_are_refused() {
        let ok = |extra: &[&str]| Args::try_parse_from(["img-fp"].iter().chain(extra).chain(&["."])).is_ok();
        assert!(ok(&[]));
        for good in [&["--min-frame-overlap", "0"][..], &["--min-frame-overlap", "1"], &["--min-pixel-correlation", "0.5"],
            &["-k", "1"], &["--min-aligned-points", "0"], &["--min-aligned-points", "2"], &["--work-size", "0"], &["--work-size", "1"], &["--work-size", "4000"]] {
            assert!(ok(good), "{good:?}");
        }
        for bad in [&["--min-frame-overlap", "7"][..], &["--min-frame-overlap", "NaN"], &["--min-pixel-correlation=-3"],
            &["--min-pixel-correlation", "1.01"], &["-k", "0"]] {
            assert!(!ok(bad), "{bad:?}");
        }
    }

    /// In a build that cannot assume AVX2, the two copies of every dispatched
    /// stage — the one compiled for x86-64-v3 and the one any x86-64 runs —
    /// give the same answer to the bit: the features, the vocabulary's words,
    /// the query's scores, the word-list intersection and the verdict. Only
    /// such a build has two copies, the release among them; one built for
    /// x86-64-v3 has one.
    #[cfg(all(dispatch, not(target_feature = "avx2")))]
    #[test]
    fn the_plain_and_the_v3_copies_agree() {
        // Random blobs at two scales, and a crop of them: a real match.
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / (1u64 << 24) as f32
        };
        let (w, h) = (320usize, 240usize);
        let coarse: Vec<f32> = (0..41 * 31).map(|_| rnd()).collect();
        let fine: Vec<f32> = (0..161 * 121).map(|_| rnd()).collect();
        let bilinear = |grid: &[f32], gw: usize, step: f32, x: usize, y: usize| {
            let (fx, fy) = (x as f32 / step, y as f32 / step);
            let (x0, y0) = (fx as usize, fy as usize);
            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
            let at = |i: usize, j: usize| grid[j * gw + i];
            (at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx) * (1.0 - ty) + (at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx) * ty
        };
        let mut g = decode::Gray::new(w, h);
        for y in 0..h {
            for x in 0..w {
                g.px[y * w + x] = 0.7 * bilinear(&coarse, 41, 8.0, x, y) + 0.3 * bilinear(&fine, 161, 2.0, x, y);
            }
        }
        let mut c = decode::Gray::new(200, 150);
        for y in 0..150 {
            for x in 0..200 {
                c.px[y * 200 + x] = g.px[(30 + y) * w + 40 + x];
            }
        }
        let run = || {
            let p = sift::Params { max_features: FEATURES, ..Default::default() };
            let (fa, fb) = (sift::extract(&g, &p), sift::extract(&c, &p));
            let (ta, tb) = (Thumb::build(&g, THUMB_LONG), Thumb::build(&c, THUMB_LONG));
            let pool: Vec<&[u8; DESC_LEN]> = (0..fa.len()).map(|i| fa.d(i)).chain((0..fb.len()).map(|i| fb.d(i))).map(|d| d.try_into().unwrap()).collect();
            let vp = index::VocabParams::for_corpus(pool.len());
            let vocab = Vocabulary::build(pool, &vp);
            let lists = vec![quantise(&vocab, &fa), quantise(&vocab, &fb)];
            let inv = InvertedFile::build(&lists, vocab.n_live_words());
            let (mut acc, mut scored) = (vec![0f32; 2], Vec::new());
            inv.query(&lists[1], 1, &mut acc, &mut scored);
            let mut cands = Vec::new();
            index::shared(&lists[0], &lists[1], &mut cands, 60_000);
            let pair = verify::Pair { fa: &fa, fb: &fb, ta: &ta, tb: &tb };
            let v = verify::verify(&pair, &cands, Variant::default(), (3, 0.2), &mut Vec::new(), &mut verify::Scratch::default());
            let t = verify::verify_transform(&pair, &v.m, Variant::default(), 0.2);
            let kps = |f: &Features| f.kps.iter().map(|k| [k.x, k.y, k.sigma, k.angle].map(f32::to_bits)).collect::<Vec<_>>();
            let verdict = |v: &Verdict| (v.n_match, v.n_in, v.m.map(f32::to_bits), [v.ov_a, v.ov_b, v.blk, v.ncc, v.blk_min].map(f32::to_bits), v.blk_n);
            (
                (kps(&fa), fa.desc.clone(), kps(&fb), fb.desc.clone(), ta.px.clone()),
                (lists.iter().map(|l| (0..l.len()).map(|i| l.word(i)).collect::<Vec<_>>()).collect::<Vec<_>>(), scored.iter().map(|e| (e.0, e.1.to_bits())).collect::<Vec<_>>()),
                (cands.clone(), verdict(&v), verdict(&t)),
            )
        };
        if !simd::v3() {
            eprintln!("skipped: a CPU without AVX2 has only the one copy to run");
            return;
        }
        let v3 = run();
        simd::force_plain(true);
        let plain = run();
        simd::force_plain(false);
        assert!(v3.0.0.len() > 50 && v3.2.1.1 >= 10, "the pictures match: {} keypoints, {} aligned points", v3.0.0.len(), v3.2.1.1);
        assert_eq!(v3.0, plain.0, "the features");
        assert_eq!(v3.1, plain.1, "the words and the query");
        assert_eq!(v3.2, plain.2, "the intersection and the verdicts");
    }

    /// A binary built for the machine it runs on passes its own check.
    #[test]
    fn this_machine_has_what_this_build_needs() {
        assert!(check_cpu().is_ok());
    }
}
