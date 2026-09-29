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

    /// Long side, in pixels, the images are analysed at.
    ///
    /// Higher finds more, especially small images inside larger ones such as
    /// slides, screenshots and collages, but is slower and uses more memory.
    /// 640 is a good choice when that matters. `0` does not shrink images at
    /// all, which is much slower on large photos.
    #[arg(long, value_name = "PX", default_value_t = 384)]
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
    #[arg(long, value_name = "FILE")]
    dump: Option<PathBuf>,

    /// Use this cache file instead of the default one.
    ///
    /// The default is `$XDG_CACHE_HOME/img-fp/analysis.bin`, or
    /// `~/.cache/img-fp/analysis.bin`. A folder gets the default file name
    /// inside it.
    #[arg(long, value_name = "PATH", conflicts_with = "no_cache")]
    cache: Option<PathBuf>,

    /// Don't read or write the cache.
    #[arg(long, conflicts_with = "prune_cache")]
    no_cache: bool,

    /// Delete the cache before running.
    ///
    /// With `--no-cache`, delete it and don't write a new one.
    #[arg(long)]
    clear_cache: bool,

    /// Drop cached entries for images this scan did not find.
    ///
    /// Skipped when the scan could not read everything it was given.
    #[arg(long)]
    prune_cache: bool,

    /// Print timings for each stage.
    #[arg(short, long)]
    verbose: bool,

    /// Write every skipped file, problem and stage timing to this file.
    ///
    /// Truncated at the start of each run.
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

/// The largest component propagation works inside. A round tests every
/// unmatched pair in a component, which is quadratic in it: two thousand files
/// are two million pixel checks a round, about a minute of CPU, and
/// the cost goes up fourfold with every doubling after that.
const PROPAGATE_MAX_COMPONENT: usize = 2000;

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
/// branch the selection mispredicts about half the time. Every score a query
/// hands this is positive (a touched image has at least one shared word, of
/// idf at least ln 5); anything else takes the comparator, as before.
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
    // A keypoint index fits the list's sixteen bits: `retain_best` holds an
    // image to `FEATURES`.
    debug_assert!(f.len() <= u16::MAX as usize + 1);
    WordList { word: pairs.iter().map(|p| p.0).collect(), kp: pairs.iter().map(|p| p.1 as u16).collect() }
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
/// The release is built for x86-64-v3 (AVX2, FMA, BMI2), and the fast kernels
/// are chosen at compile time. On an older CPU the first of those instructions
/// ends the process with SIGILL — "Illegal instruction" and nothing else,
/// which reads as a crash rather than as the wrong build. Asked first, it is a
/// sentence that says which build to use instead.
///
/// It checks exactly the features this binary was compiled with, so a build
/// for the machine it runs on (`target-cpu=native`) always passes, and a
/// portable one (`x86-64`) checks nothing.
pub fn check_cpu() -> Result<()> {
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
                "this build of img-fp needs a CPU with {}, which this one does not have. \
                 Build it for this machine instead: cargo install img-fp --locked",
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
    Args::try_parse_from(argv).map(|_| ()).map_err(|e| e.to_string())
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
        #[cfg(unix)]
        {
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
        }
        let (_held, kept) = cache::seal();
        progress::clear_for_exit();
        if kept > 0 {
            let (noun, verb) = if kept == 1 { ("description", "was") } else { ("descriptions", "were") };
            eprintln!("Interrupted. {kept} new image {noun} {verb} saved to cache.");
        } else {
            eprintln!("Interrupted.");
        }
        // `_exit`, not `exit`: the workers are still running, some of them
        // inside libheif, and `exit` would run the C++ static destructors out
        // from under them. Nothing is buffered that needs flushing — stderr
        // is not, the log is written a line at a time, and the cache is
        // written through the kernel.
        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn _exit(status: i32) -> !;
            }
            unsafe { _exit(EXIT_INTERRUPTED) }
        }
        #[cfg(not(unix))]
        std::process::exit(EXIT_INTERRUPTED);
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
        problems,
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
    let files = walk::walk(
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
        match write_reports(args, gui, &out, &files)? {
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
    let (mut cached, mut store) = match &cache_path {
        Some(p) => {
            progress.begin(Stage::CacheRead);
            // Only this run's paths are unpacked; the rest of the machine's
            // cache is read for where it is, not for what it says. See
            // `cache::open`.
            let walked: std::collections::HashSet<&Path> = files.iter().map(|p| p.as_path()).collect();
            let (cached, store) = cache::open(p, settings, &|path| walked.contains(path), problems);
            (cached, Some(store))
        }
        None => Default::default(),
    };
    // Records in the file that the map does not hold: each superseded by a
    // later one for the same path. Something to compact away, if nothing else is.
    let superseded = store.as_ref().map_or(0, |s| s.records() - cached.len());
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
        .map(|(i, name)| {
            let got = cached.remove(name)?;
            in_cache += 1;
            let k = cache::key_of(&files[i])?;
            if got.0.len != k.len || got.0.mtime != k.mtime {
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
    let weight: Vec<u64> = (0..n)
        .into_par_iter()
        .map(|i| {
            headers.tick();
            if twin_of[i] != i || mine[i].is_some() {
                return 0;
            }
            let bytes = std::fs::metadata(&files[i]).map(|m| m.len()).unwrap_or(0);
            let probe = decode::probe(&files[i]);
            progress::analysis_cost(probe.as_ref(), bytes, args.work_size, sp.upsample_below)
        })
        .collect();
    stage!(t_start, "{todo} files to analyse");
    // A wildcard walk has not guessed at anything, so what it found is files;
    // calling them images is how a home directory reads as a photo library.
    let found = if wanted.is_a_guess_at_images() { "images" } else { "files" };
    // The three add up to `n`: a copy is answered by its original whether or
    // not the cache also knew it, so it is counted as a copy and only there.
    let copies = (0..n).filter(|&i| twin_of[i] != i).count();
    let from_cache = n - copies - todo;
    if from_cache > 0 || copies > 0 {
        say!("Found {n} {found}; {from_cache} already cached, {copies} identical copies, {todo} to analyse.");
    } else {
        say!("Found {n} {found}. Analysing...");
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
        s.append(&files[i], key, &it.feats, &it.thumb)
    };
    let (mut items, appended): (Vec<Item>, Vec<Option<cache::Span>>) = mine
        .into_par_iter()
        .enumerate()
        .map(|(i, rec)| {
            if twin_of[i] != i {
                return (Item::default(), None);
            }
            if let Some((rec, _)) = rec {
                let it = Item { feats: rec.feats.into(), thumb: rec.thumb.into(), ok: true, err: None };
                return (it, None);
            }
            let f = &files[i];
            // Keyed before it is read, so that a file changing under the
            // analysis is described again next time rather than trusted.
            let key = store.as_ref().and_then(|_| cache::key_of(f));
            let it = analyse(f, args.work_size, &sp);
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
            (0..n).filter(|&i| items[i].ok).filter_map(|i| Some((files[i].as_path(), span_of[i]?))).collect();
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
        let dropped = cached.len() - kept.len();
        if prune && dropped > 0 {
            say!("pruned {dropped} cached record(s) this scan did not find");
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
        } else if let Err(e) = store.compact(settings, &mut entries) {
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
    say!("Analysis complete. Matching {n_ok} images...");
    let vp = index::VocabParams::for_corpus(n_desc_match);
    progress.forecast(|f| f.vocab_sample = Some(vp.sample.min(n_desc_match)));
    progress.begin(Stage::Vocabulary);
    let mut pool: Vec<u8> = Vec::with_capacity(vp.sample.min(n_desc_match) * DESC_LEN);
    timed!(34, {
        // Even sampling across images, so one feature-rich image cannot own
        // the vocabulary.
        let per = (vp.sample / n_match.max(1)).max(8);
        for it in (0..n).filter(|&i| matched[i]).map(|i| &items[i]) {
            let take = it.feats.len().min(per);
            let step = (it.feats.len() / take.max(1)).max(1);
            for i in (0..it.feats.len()).step_by(step).take(take) {
                pool.extend_from_slice(it.feats.d(i));
            }
        }
    });
    let vocab = timed!(12, Vocabulary::build(&pool, &vp));
    drop(pool);
    stage!(t_start, "vocabulary: {} live words of {} from {} samples", vocab.n_live_words(), vocab.n_words(), vp.sample.min(n_desc_match));

    // Quantise. The word lists are built straight into the vector the inverted
    // file and every later stage read from: holding a second copy per image
    // costs as much again as the lists themselves.
    progress.forecast(|f| f.live_words = Some(vocab.n_live_words()));
    let bar = progress.begin_counted(Stage::Quantise, n_desc_match as u64, n_match, "images");
    let lists: Vec<WordList> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !matched[i] {
                return WordList::default();
            }
            let it = &items[i];
            let wl = timed!(11, quantise(&vocab, &it.feats));
            bar.add(it.feats.len() as u64);
            wl
        })
        .collect();
    stage!(t_start, "quantised");

    // Inverted file. A word present in a fifth of the corpus says nothing.
    progress.begin(Stage::InvertedFile);
    let max_posting = (n_match / 5).max(32);
    let inv = timed!(13, InvertedFile::build(&lists, vocab.n_live_words(), max_posting));
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
                    // in every image, which an index can hold only on a folder
                    // of 32 or fewer (see `query_touched`) — and on a folder
                    // of two it is every word a duplicate shares. Dropping it
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
    let all_direct: Vec<Edge> = cand_pairs
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
                (v.accepted(&policy.corroborated) || (dumping && v.n_in >= 3)).then_some((i, j, v.m, false, v))
            }),
        )
        .flatten()
        .collect();
    // Every candidate has its verdict now, and the second look below is where
    // a found corpus's run peaks: a million pairs held through it for nothing.
    drop(cand_pairs);
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
        f.edges = Some(edges.len());
    });
    let bar = progress.begin_counted(Stage::Variants, total_weight, lonely.len(), "images");
    let variant_all: Vec<(Edge, bool)> = lonely
        .par_iter()
        .zip(lonely_weight.par_iter())
        .map_init(
            || (vec![0f32; n], Vec::new(), Vec::new(), Vec::new(), verify::Scratch::default(), Vec::new()),
            |(acc, scored, cands, matches, scratch, merged), (&i, &w)| timed!(38, {
                let mut out: Vec<(Edge, bool)> = Vec::new();
                let vf: [Features; 3] = timed!(40, std::array::from_fn(|v| variant_features(&items[i].feats, variants[v])));
                let wl: [WordList; 3] = std::array::from_fn(|v| timed!(39, quantise(&vocab, &vf[v])));
                merged.clear();
                for v in 0..3 {
                    timed!(14, inv.query(&wl[v], i as u32, acc, scored));
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
    let all: Vec<Edge> = unique_pairs(edges.into_iter().chain(variant_edges.iter().cloned()));

    // Every anchor faces the bridge test, whichever pass produced it: a
    // mirrored match joining two clusters is exactly as consequential as a
    // direct one, and on this corpus an inverted match between a 225-pixel
    // photograph of the Earth and a beach scene was the single edge that
    // merged two whole families into 3,002 false pairs.
    progress.forecast(|f| f.edges = Some(all.len()));
    progress.begin(Stage::Propagate);
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
    for round in 0..PROPAGATE_MAX_ROUNDS {
        let (round_all, too_large) = timed!(21, propagate(&items, &pool, n, prop_min_ov, dirty.as_deref()));
        // Said once, and on the console: pairs inside such a component rest on
        // direct matches alone, and propagation is most of the tool's recall.
        if round == 0 && !too_large.is_empty() {
            let files: usize = too_large.iter().sum();
            say!(
                "Note: {} cluster(s) of more than {PROPAGATE_MAX_COMPONENT} files ({files} files in all) are too large to \
                 propagate matches inside; pairs within them come from direct matches only.",
                too_large.len()
            );
        }
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

    stage!(t_start, "propagated: {} of {} composed hypotheses", propagated.len(), hypotheses.len());
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
        let f = std::fs::File::create(path).with_context(|| format!("could not create {}", path.display()))?;
        let mut w = std::io::BufWriter::new(f);
        writeln!(w, "a,b,kind,n_match,n_in,ov_a,ov_b,scale,rot,blk,blk_n,ncc,centred,inverted")?;
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
                ];
                let mut row: Vec<&[u8]> = vec![files[*a].as_os_str().as_bytes(), files[*b].as_os_str().as_bytes()];
                row.extend(figures.iter().map(|f| f.as_bytes()));
                report::csv_row_with(&mut w, &row, b',')?;
            }
        }
        w.flush()?;
        say!("dumped verdicts -> {}", path.display());
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
                    });
                }
            }
        }
    }
    let tiers = all.iter().map(|e| (e, false)).chain(propagated.iter().map(|e| (e, false)));
    for ((a, b, _, inv_flag, v), corroborated) in tiers.chain(corroborated.iter().map(|e| (e, true))) {
        let (a, b) = (*a, *b);
        if !seen.insert((a, b)) {
            continue;
        }
        graph.push((a, b));
        out_pairs.push(OutPair {
            a: files[a].display().to_string(),
            b: files[b].display().to_string(),
            a_bytes: report::raw_bytes(&files[a]),
            b_bytes: report::raw_bytes(&files[b]),
            aligned_points: v.n_in,
            frame_overlap: round3(v.ov_a.max(v.ov_b)),
            pixel_correlation: round3(v.blk),
            scale: round3(v.scale),
            mirrored: v.m[0] * v.m[4] - v.m[1] * v.m[3] < 0.0,
            inverted: *inv_flag,
            identical: false,
            propagated: v.n_match == 0,
            corroborated,
            ia: a,
            ib: b,
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

    let summary = format!("{} groups, {} pairs over {} images in {:.1}s", out.groups.len(), out.pairs.len(), n_ok, runtime);
    match timed!(36, write_reports(args, gui, &out, &files))? {
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
        "min_frame_overlap": args.min_frame_overlap,
        "min_pixel_correlation": args.min_pixel_correlation,
        "stages": "anchor, propagate, corroborate",
    })
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
fn write_reports(args: &Args, gui: Option<&Path>, out: &Output, files: &[PathBuf]) -> Result<Option<PathBuf>> {
    let target = report::Target::of(args.output.as_deref(), args.format);
    let asked = gui.is_none() || args.output.as_deref().is_some_and(|p| p != stdout_path());
    let facts = report::read_facts(out, files);
    if let Some(path) = gui {
        let target = report::Target { sink: report::Sink::File(path.to_path_buf()), format: report::Format::Json };
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

fn stdout_path() -> &'static Path {
    Path::new("-")
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
///
/// Also returns the sizes of the components skipped for being larger than
/// `PROPAGATE_MAX_COMPONENT`, so that the run can say so.
#[allow(clippy::type_complexity)]
///
/// `dirty`, when given, limits the work to the components holding a file it
/// marks; see the round loop in `run`.
fn propagate(
    items: &[Item],
    edges: &[(usize, usize, Affine, bool, Verdict)],
    n: usize,
    min_ov: f32,
    dirty: Option<&[bool]>,
) -> (Vec<(usize, usize, Affine, bool, Verdict)>, Vec<usize>) {
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

    let too_large: Vec<usize> = comps.values().map(|c| c.len()).filter(|&l| l > PROPAGATE_MAX_COMPONENT).collect();
    let comps: Vec<Vec<usize>> = comps
        .into_values()
        .filter(|c| c.len() > 2 && c.len() <= PROPAGATE_MAX_COMPONENT)
        .filter(|c| dirty.is_none_or(|d| c.iter().any(|&i| d[i])))
        .map(|mut c| {
            c.sort_unstable();
            c
        })
        .collect();

    let found = comps
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
                    // Kept against the floor the caller asked for — the
                    // propagated tier's own, or a dump's lower one — and no
                    // other. This used to be a fixed 0.5, which quietly
                    // overruled a `--min-frame-overlap` below it for every
                    // composed pair, and threw away the dump's hypotheses
                    // between 0.2 and 0.5.
                    if v.ov_a.max(v.ov_b) >= min_ov {
                        out.push((a, b, m, var.invert, v));
                    }
                }
            }
            out
        })
        .collect();
    (found, too_large)
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

    /// A component propagation cannot afford is named, not passed over in
    /// silence; one it can is not.
    #[test]
    fn a_component_too_large_to_propagate_is_reported() {
        let id: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let star = |k: usize| (1..k).map(|b| (0, b, id, false, Verdict::default())).collect::<Vec<_>>();
        let n = PROPAGATE_MAX_COMPONENT + 1;
        let items: Vec<Item> = (0..n).map(|_| Item::default()).collect();
        let (found, too_large) = propagate(&items, &star(n), n, 0.85, None);
        assert!(found.is_empty());
        assert_eq!(too_large, vec![n]);
        let (_, too_large) = propagate(&items, &star(3), n, 0.85, None);
        assert!(too_large.is_empty());
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

    /// A binary built for the machine it runs on passes its own check.
    #[test]
    fn this_machine_has_what_this_build_needs() {
        assert!(check_cpu().is_ok());
    }
}
