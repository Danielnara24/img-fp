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

mod against;
mod cache;
mod claims;
mod decode;
mod extensions;
mod group;
mod heif;
mod index;
pub mod last;
pub mod outcome;
mod pipeline;
mod problems;
mod prof;
mod progress;
use progress::Stage;
mod report;
mod sift;
mod simd;
mod spill;
mod same;
mod suggest;
pub use outcome::{Evidence, Outcome, Relation};
pub use suggest::{Action, Lapse, Mode as SuggestMode};
pub use cache::file_name as cache_file_name;
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
use claims::{Claim, Edge, How};
use verify::{Thumb, Variant, Verdict};

#[derive(Parser, Debug)]
#[command(
    name = "img-fp",
    about = "Find duplicate and near-duplicate images.",
    version,
    after_help = "A few more options, for tuning and scripting, are in the man page (img-fp --man)."
)]
// The doc comments below are the `--help` text and the man page, so they say
// what a flag does and nothing about how its default was chosen. That is in
// CLAUDE.md, which the binary cannot point to.
struct Args {
    /// Folders or image files to scan. `-` reads a list of paths from stdin.
    ///
    /// A file named `-` is `./-`.
    #[arg(required_unless_present_any = ["completions", "man", "from_file", "last"], value_name = "PATH")]
    roots: Vec<PathBuf>,

    /// Read the paths to scan from a file, one per line (`-` = stdin).
    ///
    /// Entries may be folders or files and combine with any paths given as
    /// arguments. Blank lines are ignored.
    #[arg(long = "from-file", value_name = "FILE")]
    from_file: Option<PathBuf>,

    /// Paths in the list are NUL-separated, for `find -print0` or `fd -0`,
    /// and so are the paths `--list` prints.
    #[arg(short = '0', long = "null")]
    null: bool,

    /// Compare the images scanned with a library of images already kept: a
    /// folder, or an image file. Repeat for several.
    ///
    /// Instead of groups, the report then gives each image scanned a verdict:
    /// `repeat` (the library, or another image scanned, already shows all of
    /// it), `better` (the library has it, but this one shows more of it),
    /// `review` (a weak match to the library, close enough to be an edited
    /// copy) or `new`. The library's own duplicates are not looked for. It is
    /// walked with the same options as the paths to scan, so a library of
    /// subfolders needs `--recursive`. An image both reach counts as scanned.
    #[arg(long, value_name = "PATH")]
    against: Vec<PathBuf>,

    /// With `--against`, print only the paths of the images with this
    /// verdict, one a line, instead of the report: repeat, better, review or
    /// new. Repeat the option, or separate verdicts with commas, for several.
    ///
    /// The report is still written to `--output`, if that names a file.
    #[arg(long, value_enum, value_delimiter = ',', value_name = "VERDICT", requires = "against")]
    list: Vec<against::Verdict>,

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

    /// Also walk hidden folders, whose names start with a dot.
    ///
    /// They are left out by default: in a home folder they hold programs'
    /// icons and themes rather than photos. A hidden folder named as a path
    /// to scan is scanned either way.
    #[arg(long)]
    hidden: bool,

    /// Extensions a folder walk treats as images, comma-separated or repeated.
    ///
    /// `-x '*'` takes every file. An entry starting with `!` is an exception:
    /// `-x '!gif'` takes every file but GIFs. A file with no extension is
    /// taken only by those two forms, and a file named on the command line is
    /// scanned whatever its extension.
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
    /// be found. A picture much wider or taller than 4:3, such as a panorama
    /// or a full-page screenshot, gets a longer side, so that it is analysed
    /// with as many pixels as a 4:3 picture.
    ///
    /// Higher finds more, especially small images inside larger ones such as
    /// slides, screenshots and collages, but is slower and uses more memory.
    /// 640 is a good choice when that matters. `0` does not shrink images at
    /// all, which is much slower on large photos.
    #[arg(long, value_name = "PX", default_value_t = 512)]
    work_size: usize,

    /// Candidate matches checked per image.
    #[arg(short = 'k', long, value_name = "N", default_value_t = 150, value_parser = parse_candidates, hide = true)]
    candidates: usize,

    /// Matching points two images must share before they count as duplicates.
    ///
    /// Higher is stricter. Below 3 behaves as 3. An image that is the whole of
    /// the other, only resized, needs 3 if every part of it agrees: a plain
    /// picture such as an icon or a logo has few points to match.
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
    // 0.7 is set for what a user wants grouped rather than for F1; see
    // CLAUDE.md for the sweep and the cliff.
    #[arg(long, value_name = "F", default_value_t = 0.7, value_parser = parse_fraction)]
    min_pixel_correlation: f32,

    /// Don't search again for mirrored or inverted (negative) copies.
    ///
    /// Images that found fewer than two matches are normally searched for a
    /// second time, mirrored and inverted. In a library with few duplicates
    /// that is most images and most of a rescan's time. Skipping it is much
    /// faster there. It misses most mirrored and inverted copies, and where
    /// such a copy was what linked the rest of a group, the group splits.
    // Not a dominated trade, unlike the propagation switch that was removed:
    // a cached rescan of the found corpus goes 40 s -> 15 s for 16% of its
    // pairs, and the benchmark corpora lose 7-10 points of recall for 5-9%.
    // See CLAUDE.md, *What a cached run still has to do*.
    #[arg(long)]
    skip_mirrored: bool,

    /// How each file's KEEP, DELETE or REVIEW is decided.
    #[arg(long, value_enum, value_name = "RULE", default_value = "content")]
    suggest: suggest::Mode,

    /// Write every candidate pair considered, accepted or not, to this CSV.
    /// `-` is stdout.
    #[arg(long, value_name = "FILE", hide = true)]
    dump: Option<PathBuf>,

    /// Use this cache file instead of the default one.
    ///
    /// The default is `analysis-FORMAT.bin` in `$XDG_CACHE_HOME/img-fp`, or in
    /// `~/.cache/img-fp`, where FORMAT is the cache format this version
    /// writes, so each version keeps a default file of its own. A folder gets
    /// the default file name inside it.
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
    #[arg(long, hide = true)]
    prune_cache: bool,

    /// Write the last scan's report again, without scanning: in another
    /// format with `--output` or `--format`, or with another `--suggest` rule.
    ///
    /// The last scan is the last one run from the command line or the window,
    /// kept beside the cache. Files deleted since are left out, and a DELETE
    /// whose kept copy was one of them is REVIEW. A JSON report has the
    /// groups but no list of pairs.
    #[arg(
        long,
        conflicts_with_all = [
            "roots", "from_file", "null", "against", "list", "recursive", "exclude", "follow_symlinks",
            "hidden", "extensions", "work_size", "candidates", "min_aligned_points", "min_frame_overlap",
            "min_pixel_correlation", "skip_mirrored", "dump", "no_cache", "clear_cache", "prune_cache", "no_last",
        ]
    )]
    last: bool,

    /// Don't keep this scan for `--last` and the window.
    #[arg(long, hide = true)]
    no_last: bool,

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

/// Sets of byte-identical files, by index. `stats` is the walk's, for each
/// file's size; `comparable` says whether a file is worth reading at all, and
/// is asked only of files that share a size with another.
fn exact_groups(files: &[PathBuf], stats: &[walk::Stat], comparable: impl Fn(usize) -> bool + Sync) -> Vec<Vec<usize>> {
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, st) in stats.iter().enumerate() {
        by_size.entry(st.len).or_default().push(i);
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
            let comparable = &comparable;
            group
                .iter()
                .filter(move |&&i| comparable(i))
                .filter_map(move |&i| sample_hash(&files[i], *len).map(|h| ((*len, h), i)))
                .collect::<Vec<_>>()
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
#[derive(Clone, Default)]
struct Item {
    feats: std::sync::Arc<Features>,
    thumb: std::sync::Arc<Thumb>,
    /// The picture's own size, as shown; `feats` holds the working image's.
    dims: (u32, u32),
    ok: bool,
    err: Option<String>,
    /// Decoded only in part; see `decode::Decoded::damaged`.
    damaged: Option<String>,
    /// What the keep/delete suggestion reads beyond the analysis.
    traits: decode::Traits,
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
const FEATURES: usize = 600;

/// A ceiling, not a setting. Each round re-routes composed transforms through
/// the pairs the last one accepted, and the loop stops as soon as a round adds
/// nothing — which on every corpus tried has been the second or third. The
/// constant only exists so a pathological graph cannot spin forever.
const PROPAGATE_MAX_ROUNDS: usize = 8;

/// Candidate pairs verified per slice; see `pipeline::retrieve_and_verify`.
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
            Item { feats, thumb, dims: d.size, ok: true, err: None, damaged: d.damaged, traits }
        }
        Err(e) => Item { err: Some(e.to_string()), ..Default::default() },
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

/// A query's scores read both ways round: for each candidate, the larger of
/// how much of the query it holds and how much of it the query holds.
///
/// `query` scores containment of the query, which finds a crop's original
/// and not an original's crop. A scan of one folder asks both questions by
/// querying from both files, but against a library only the scanned files
/// ask — querying from every library file was the cost the mode exists to
/// avoid — so a photograph whose library copy is a crop would rank that crop
/// by how little of the photograph it holds. The intersection `query`
/// accumulates is the same from either side, so the other reading is the
/// score scaled by the two lists' idf masses (`InvertedFile::mass`), and
/// costs one multiply a candidate.
fn both_ways(scored: &mut [(u32, f32)], query_mass: f32, mass: &[f32]) {
    for (j, s) in scored.iter_mut() {
        let m = mass[*j as usize];
        if m > 0.0 {
            *s = s.max(*s * query_mass / m);
        }
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
    let mut kps = f.kps.to_vec();
    let mut desc = vec![0u8; f.desc.len()];
    let mut perm: [u8; DESC_LEN] = std::array::from_fn(|i| i as u8);
    if var.mirror {
        perm = compose_perm(&perm, &sift::mirror_perm());
    }
    if var.invert {
        perm = compose_perm(&perm, &sift::invert_perm());
    }
    for i in 0..f.len() {
        sift::permute(f.d(i), &perm, &mut desc[i * DESC_LEN..(i + 1) * DESC_LEN]);
    }
    let w = f.w as f32;
    for k in kps.iter_mut() {
        if var.mirror {
            k.x = w - 1.0 - k.x;
            k.angle = (180.0 - k.angle).rem_euclid(360.0);
        }
        if var.invert {
            k.angle = (k.angle + 180.0).rem_euclid(360.0);
        }
    }
    Features { w: f.w, h: f.h, kps: kps.into(), desc: desc.into() }
}

fn compose_perm(a: &[u8; DESC_LEN], b: &[u8; DESC_LEN]) -> [u8; DESC_LEN] {
    std::array::from_fn(|i| a[b[i] as usize])
}

// ---------------------------------------------------------------- output

use outcome::{Group, Head, Member};
use report::OutPair;
use same::Same;

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
/// portable one (`x86-64`) checks nothing. A refusal is said on stderr and
/// ends the process with exit 1 there and then (see `cpu`), so it is the
/// first thing either binary does.
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
        // Never set in a build that assumes nothing, hence the `allow`.
        #[allow(unused_mut)]
        let mut missing = false;
        // Each feature asked only of a build that has it.
        macro_rules! need {
            ($($f:tt),*) => {$(
                #[cfg(target_feature = $f)]
                if !cpu::has($f) {
                    cpu::say(if missing { ", " } else { "Error: this build of " });
                    if !missing {
                        cpu::say(program);
                        cpu::say(" needs a CPU with ");
                    }
                    cpu::say($f);
                    missing = true;
                }
            )*};
        }
        // Every instruction set rustc can be told to assume, not only the
        // x86-64-v3 ones. `target-cpu=native` on a machine with AVX-512 turns
        // it on, and the compiler then uses it everywhere.
        need!(
            "sse3", "ssse3", "sse4.1", "sse4.2", "sse4a", "popcnt", "cmpxchg16b", "avx", "avx2", "fma", "bmi1", "bmi2", "lzcnt",
            "movbe", "f16c", "adx", "aes", "pclmulqdq", "rdrand", "rdseed", "rtm", "sha", "sha512", "sm3", "sm4", "tbm", "gfni",
            "vaes", "vpclmulqdq", "kl", "widekl", "xsave", "xsavec", "xsaveopt", "xsaves", "avxifma", "avxneconvert", "avxvnni",
            "avxvnniint8", "avxvnniint16", "avx512f", "avx512cd", "avx512bw", "avx512dq", "avx512vl", "avx512ifma", "avx512vbmi",
            "avx512vbmi2", "avx512vnni", "avx512bitalg", "avx512vpopcntdq", "avx512bf16", "avx512fp16", "avx512vp2intersect"
        );
        if missing {
            cpu::say(", which this one does not have. Build it for this machine instead: ");
            cpu::say(install);
            cpu::say("\n");
            cpu::refuse();
        }
    }
    let _ = (program, install);
    Ok(())
}

/// What the CPU says it has, asked of `cpuid` itself, and the refusal.
///
/// **The refusal is written a string at a time and ends the process at
/// once.** Every line of this binary is compiled for the features it assumes,
/// so the first allocation or format on the way to an ordinary error may be
/// one of the instructions the CPU lacks: a build for AVX-512 found what was
/// missing and died building the message (`anyhow::Error::construct`, its
/// first `vmovups` of a ZMM register). A `write` of a string the binary holds
/// and an `_exit` copy nothing.
///
/// **Not `is_x86_feature_detected!`**, which answers `true` without asking
/// for any feature the build was compiled with — `cfg!(target_feature)` is
/// the first half of its expansion — so in exactly the builds `check_cpu` is
/// for, it said every feature was there: a build for AVX-512 passed the check
/// on a Zen 2 and died with SIGILL in the argument parser.
#[cfg(target_arch = "x86_64")]
// `cpuid` is a safe call from Rust 1.89 and unsafe before it; and a build
// that assumes nothing past x86-64 asks it nothing outside the tests.
#[allow(unused_unsafe, dead_code)]
mod cpu {
    use std::arch::x86_64::{CpuidResult, __cpuid, __cpuid_count};

    fn leaf(l: u32, sub: u32) -> CpuidResult {
        // SAFETY: `cpuid` exists on every x86-64 CPU, and a leaf past the
        // highest one is asked only below once that one is known.
        unsafe { __cpuid_count(l, sub) }
    }

    /// The register bits the OS saves on a switch (XCR0), when it says.
    fn xcr0() -> u64 {
        if leaf(1, 0).ecx >> 27 & 1 == 0 {
            return 0;
        }
        #[target_feature(enable = "xsave")]
        unsafe fn read() -> u64 {
            unsafe { std::arch::x86_64::_xgetbv(0) }
        }
        // SAFETY: OSXSAVE is set, so `xgetbv` is there and enabled.
        unsafe { read() }
    }

    /// `s` on stderr, as it is.
    pub fn say(s: &str) {
        unsafe extern "C" {
            fn write(fd: i32, buf: *const u8, n: usize) -> isize;
        }
        // SAFETY: `s` is valid for its length; a failed write is dropped.
        unsafe {
            write(2, s.as_ptr(), s.len());
        }
    }

    /// End the process as a fatal error does: exit 1.
    pub fn refuse() -> ! {
        unsafe extern "C" {
            fn _exit(status: i32) -> !;
        }
        // SAFETY: nothing has started that needs to finish.
        unsafe { _exit(1) }
    }

    /// Whether the CPU, and for the vector extensions the OS, has `name`.
    pub fn has(name: &str) -> bool {
        // SAFETY: as in `leaf`.
        let max = unsafe { __cpuid(0) }.eax;
        let max_ext = unsafe { __cpuid(0x8000_0000) }.eax;
        let bit = |l: u32, sub: u32, reg: char, b: u32| -> bool {
            let ok = if l >= 0x8000_0000 { l <= max_ext } else { l <= max };
            if !ok {
                return false;
            }
            let r = leaf(l, sub);
            let v = match reg {
                'a' => r.eax,
                'b' => r.ebx,
                'c' => r.ecx,
                _ => r.edx,
            };
            v >> b & 1 == 1
        };
        let x = xcr0();
        // AVX state (SSE and YMM), and AVX-512's (opmask, ZMM halves).
        let avx_os = x & 0b110 == 0b110;
        let avx512_os = avx_os && x & 0b1110_0000 == 0b1110_0000;
        let (avx, avx512) = (|b: bool| b && avx_os, |b: bool| b && avx512_os);
        match name {
            "sse3" => bit(1, 0, 'c', 0),
            "pclmulqdq" => bit(1, 0, 'c', 1),
            "ssse3" => bit(1, 0, 'c', 9),
            "fma" => avx(bit(1, 0, 'c', 12)),
            "cmpxchg16b" => bit(1, 0, 'c', 13),
            "sse4.1" => bit(1, 0, 'c', 19),
            "sse4.2" => bit(1, 0, 'c', 20),
            "movbe" => bit(1, 0, 'c', 22),
            "popcnt" => bit(1, 0, 'c', 23),
            "aes" => bit(1, 0, 'c', 25),
            "xsave" => bit(1, 0, 'c', 26),
            "avx" => avx(bit(1, 0, 'c', 28)),
            "f16c" => avx(bit(1, 0, 'c', 29)),
            "rdrand" => bit(1, 0, 'c', 30),
            "bmi1" => bit(7, 0, 'b', 3),
            "avx2" => avx(bit(7, 0, 'b', 5)),
            "bmi2" => bit(7, 0, 'b', 8),
            "rtm" => bit(7, 0, 'b', 11),
            "avx512f" => avx512(bit(7, 0, 'b', 16)),
            "avx512dq" => avx512(bit(7, 0, 'b', 17)),
            "rdseed" => bit(7, 0, 'b', 18),
            "adx" => bit(7, 0, 'b', 19),
            "avx512ifma" => avx512(bit(7, 0, 'b', 21)),
            "avx512cd" => avx512(bit(7, 0, 'b', 28)),
            "sha" => bit(7, 0, 'b', 29),
            "avx512bw" => avx512(bit(7, 0, 'b', 30)),
            "avx512vl" => avx512(bit(7, 0, 'b', 31)),
            "avx512vbmi" => avx512(bit(7, 0, 'c', 1)),
            "avx512vbmi2" => avx512(bit(7, 0, 'c', 6)),
            "gfni" => bit(7, 0, 'c', 8),
            "vaes" => avx(bit(7, 0, 'c', 9)),
            "vpclmulqdq" => avx(bit(7, 0, 'c', 10)),
            "avx512vnni" => avx512(bit(7, 0, 'c', 11)),
            "avx512bitalg" => avx512(bit(7, 0, 'c', 12)),
            "avx512vpopcntdq" => avx512(bit(7, 0, 'c', 14)),
            "kl" => bit(7, 0, 'c', 23),
            "avx512vp2intersect" => avx512(bit(7, 0, 'd', 8)),
            "avx512fp16" => avx512(bit(7, 0, 'd', 23)),
            "sha512" => avx(bit(7, 1, 'a', 0)),
            "sm3" => avx(bit(7, 1, 'a', 1)),
            "sm4" => avx(bit(7, 1, 'a', 2)),
            "avxvnni" => avx(bit(7, 1, 'a', 4)),
            "avx512bf16" => avx512(bit(7, 1, 'a', 5)),
            "avxifma" => avx(bit(7, 1, 'a', 23)),
            "avxvnniint8" => avx(bit(7, 1, 'd', 4)),
            "avxneconvert" => avx(bit(7, 1, 'd', 5)),
            "avxvnniint16" => avx(bit(7, 1, 'd', 10)),
            "xsaveopt" => bit(0xD, 1, 'a', 0),
            "xsavec" => bit(0xD, 1, 'a', 1),
            "xsaves" => bit(0xD, 1, 'a', 3),
            "widekl" => bit(0x19, 0, 'b', 2),
            "lzcnt" => bit(0x8000_0001, 0, 'c', 5),
            "sse4a" => bit(0x8000_0001, 0, 'c', 6),
            "tbm" => bit(0x8000_0001, 0, 'c', 21),
            // A name not in this table is not one `check_cpu` asks about.
            _ => true,
        }
    }
}

/// The scan `img-fp-gui` runs: the command line's own, in a child process, with
/// the progress line spoken as JSON on stdout (`progress::speak_json`) and the
/// scan's `Outcome` saved to `result`, which is what the window reads.
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
    outputs_are_not_pictures(&args).map_err(|e| e.to_string())?;
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
    // The verdicts are the content rule's, with the library kept; the other
    // two rules read groups, which a run against a library does not make.
    if !args.against.is_empty() && args.suggest != suggest::Mode::Content {
        return Err("--against decides with the content rule, and cannot be used with --suggest".into());
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
    pub skip_mirrored: bool,
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
        skip_mirrored: a.skip_mirrored,
        extensions: a.extensions,
    }
}

/// A preview of an image for a window: RGBA, long side at most `long`, or
/// made for a place of a given size.
pub use decode::{fit_preview, preview, preview_fit};

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
    outputs_are_not_pictures(args)?;
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
    if args.last {
        let asked = last::Rewrite { cache: args.cache.as_deref(), output: args.output.as_deref(), format: args.format, suggest: args.suggest };
        for line in last::rewrite(&asked)? {
            progress::to_stderr(&line);
            log.line(&line);
        }
        return Ok(());
    }
    interrupt()?;
    let mut problems = Problems::new(&log);
    let outcome = pipeline::run(args, &log, &mut problems, gui);
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
    if gui.is_some() && !args.against.is_empty() {
        anyhow::bail!("--against cannot be used in the window");
    }
    if gui.is_some() && args.last {
        anyhow::bail!("--last cannot be used in the window");
    }
    let dash = |p: &Option<PathBuf>| p.as_deref() == Some(stdout_path());
    let mut on_stdout: Vec<&str> = Vec::new();
    // `--list` takes stdout's place for the report, which then goes only to
    // a file `-o` names.
    let report_there = match args.output.as_deref() {
        None => args.list.is_empty(),
        Some(p) => p == stdout_path(),
    };
    if gui.is_none() && report_there {
        on_stdout.push("the report");
    }
    if !args.list.is_empty() {
        on_stdout.push("--list");
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

/// Refuse an `-o`, `--dump` or `--log-file` that names a picture.
///
/// Each of the three replaces whatever file it names, and nothing asked what
/// that file was: `img-fp photos -o photos/a.jpg` wrote the report over
/// `a.jpg`, exit 0, and the report it wrote named `a.jpg` as its group's
/// representative. The plausible way to get there is a glob — `img-fp -o
/// *.jpg` is `-o` the first JPEG and a scan of the rest — and the log is the
/// worst of the three, truncated before anything else has run. A picture is
/// what this tool is asked to look after, so it never writes over one; a file
/// that is not a picture, such as last run's report, is written over as
/// before. Asked before the log is opened, so before anything is written.
fn outputs_are_not_pictures(args: &Args) -> Result<()> {
    for (flag, p) in [("-o", &args.output), ("--dump", &args.dump), ("--log-file", &args.log_file)] {
        let Some(p) = p.as_deref().filter(|p| *p != stdout_path()) else { continue };
        if decode::is_a_picture(p) {
            anyhow::bail!("{flag} {} is a picture, and img-fp will not write over one; name another file", p.display());
        }
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
///
/// The page lists every option, the ones `--help` hides included: those are
/// for tuning and scripting, kept out of the short list rather than secret,
/// and `clap_mangen` would leave them out too.
fn print_man() -> Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let cmd = Args::command().mut_args(|a| a.hide(false)).after_help(None::<&str>);
    clap_mangen::Man::new(cmd).render(&mut buf)?;
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
    const SIG_DFL: usize = 0;
    const SIG_IGN: usize = 1;
    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
        fn sigaction(signum: i32, act: *const u8, old: *mut u8) -> i32;
    }
    // SIGHUP, SIGINT, SIGTERM.
    const SIGNALS: [i32; 3] = [1, 2, 15];
    // **A signal the run was started ignoring stays ignored.** That is how
    // `nohup` keeps a run going after its terminal closes (SIGHUP), and how
    // a shell keeps Ctrl-C from reaching a job a script put in the
    // background (SIGINT), and installing the handler over it undid both: a
    // `nohup img-fp` an hour into a library ended with "Interrupted." the
    // moment its terminal went. Asked with `sigaction`, which changes
    // nothing: setting a disposition to find out the old one leaves a moment
    // when the signal is lost.
    let ignored: Vec<i32> = SIGNALS
        .into_iter()
        .filter(|&sig| {
            // Room for any platform's `struct sigaction`, whose first field
            // is the handler.
            let mut old = [0usize; 32];
            // SAFETY: a null action only reads the disposition into `old`.
            unsafe { sigaction(sig, std::ptr::null(), old.as_mut_ptr() as *mut u8) == 0 && old[0] == SIG_IGN }
        })
        .collect();
    let leave = ignored.clone();
    ctrlc::set_handler(move || {
        for sig in SIGNALS.into_iter().filter(|s| !leave.contains(s)) {
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
    .context("could not install the Ctrl-C handler")?;
    for sig in ignored {
        // SAFETY: setting a disposition the process was started with.
        unsafe {
            signal(sig, SIG_IGN);
        }
    }
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

/// What a pair in the report is stated from, and what every reader of the
/// report's pairs (the report, `--against`, the keep rule) reads off it.
#[derive(Clone, Copy)]
enum Stated<'a> {
    /// The same bytes; see `exact_groups`.
    Identical,
    /// Not the same bytes, the same picture; see `same::Kind`.
    SamePixels,
    /// A verified pair.
    Claim(&'a Claim),
}

impl<'a> Stated<'a> {
    fn verdict(&self) -> Option<&'a Verdict> {
        match self {
            Stated::Claim(c) => Some(&c.e.v),
            _ => None,
        }
    }

    /// What the pair says, as the report, `--last`, the window and
    /// `--against` read it.
    fn evidence(&self) -> Evidence {
        let relation = match self {
            Stated::Identical => Relation::Identical,
            Stated::SamePixels => Relation::SamePixels,
            Stated::Claim(c) => match c.how {
                How::Direct => Relation::Direct,
                How::Propagated => Relation::Propagated,
                How::Corroborated => Relation::Corroborated,
            },
        };
        let v = self.verdict();
        Evidence {
            relation,
            // None for a pair no keypoint was matched for: the same file, or
            // a composed transform.
            aligned_points: match self {
                Stated::Claim(c) if c.how != How::Propagated => Some(c.e.v.n_in),
                _ => None,
            },
            frame_overlap: v.map_or(1.0, |v| round3(v.ov_a.max(v.ov_b))),
            pixel_correlation: v.map_or(1.0, |v| round3(v.blk)),
            mirrored: v.is_some_and(|v| v.m[0] * v.m[4] - v.m[1] * v.m[3] < 0.0),
            inverted: matches!(self, Stated::Claim(c) if c.e.inverted()),
        }
    }

    /// No block of the overlap disagrees (`suggest::CLEAN`); the same file
    /// always is.
    fn clean(&self) -> bool {
        self.verdict().is_none_or(|v| v.blk_min >= suggest::CLEAN)
    }

    fn link(&self) -> suggest::Link<'a> {
        match self {
            Stated::Claim(c) => suggest::Link::Edge(&c.e.v, c.e.v.m, c.e.inverted()),
            _ => suggest::Link::Exact,
        }
    }
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
        let m: verify::Affine = [0.8, -0.3, 12.0, 0.25, 0.9, -4.0];
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
        let groups = |files: &[PathBuf]| {
            let stats: Vec<walk::Stat> =
                files.iter().map(|f| walk::Stat { len: std::fs::metadata(f).unwrap().len(), key: None }).collect();
            exact_groups(files, &stats, |_| true)
        };
        assert_eq!(groups(&files), vec![vec![0, 1, 2], vec![4, 5]]);
        // A pair alone is compared straight away.
        assert_eq!(groups(&[files[0].clone(), files[3].clone()]), Vec::<Vec<usize>>::new());
        assert_eq!(groups(&[files[0].clone(), files[1].clone()]), vec![vec![0, 1]]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Under a wildcard, two copies of a file that is no picture are not read
    /// to be compared, and so are no group; two copies of one that names an
    /// image are compared as before, whatever is in them.
    #[test]
    fn a_wildcard_walk_compares_only_what_may_be_a_picture() {
        let dir = std::env::temp_dir().join(format!("img-fp-exact-head-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = vec![7u8; 100_000];
        let names = ["a.mkv", "b.mkv", "c.jpg", "d.jpg"];
        let files: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
        for f in &files {
            std::fs::write(f, &video).unwrap();
        }
        let stats: Vec<walk::Stat> = files.iter().map(|_| walk::Stat { len: video.len() as u64, key: None }).collect();
        let comparable = |i: usize| extensions::names_an_image(&files[i]) || decode::may_be_image(&files[i]);
        assert_eq!(exact_groups(&files, &stats, comparable), vec![vec![2, 3]]);
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

    /// The check's own reading of `cpuid` says what the standard library's
    /// detector says, for every feature the detector really asks about in
    /// this build: one the build does not assume.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn the_cpu_check_reads_cpuid_as_std_does() {
        macro_rules! agree {
            ($($f:tt),*) => {$(
                if !cfg!(target_feature = $f) {
                    assert_eq!(cpu::has($f), std::arch::is_x86_feature_detected!($f), $f);
                }
            )*};
        }
        agree!(
            "sse3", "ssse3", "sse4.1", "sse4.2", "sse4a", "popcnt", "cmpxchg16b", "avx", "avx2", "fma", "bmi1", "bmi2", "lzcnt",
            "movbe", "f16c", "adx", "aes", "pclmulqdq", "rdrand", "rdseed", "rtm", "sha", "tbm", "gfni", "vaes", "vpclmulqdq",
            "xsave", "xsavec", "xsaveopt", "xsaves", "avx512f", "avx512cd", "avx512bw", "avx512dq", "avx512vl", "avx512ifma",
            "avx512vbmi", "avx512vbmi2", "avx512vnni", "avx512bitalg", "avx512vpopcntdq", "avx512bf16", "avx512fp16",
            "avx512vp2intersect"
        );
    }
}
