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
mod analysis;
mod cache;
mod claims;
mod decode;
mod extensions;
mod group;
mod heif;
mod index;
pub mod last;
mod matched;
pub mod outcome;
mod outputs;
mod pipeline;
mod problems;
mod prof;
mod progress;
use progress::Stage;
mod report;
mod retrieve;
mod sift;
mod simd;
mod spill;
mod same;
mod stated;
mod suggest;
mod table;
pub use outcome::{Evidence, Outcome, Relation};
pub use suggest::{Action, Lapse, Mode as SuggestMode};
pub use cache::file_name as cache_file_name;
pub use walk::desktop_dir;

/// The cache file a run uses when `--cache` does not say.
pub fn default_cache() -> PathBuf {
    cache::locate(None)
}
mod verify;
mod walk;

use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser};
use clap_complete::Shell;
use index::{Vocabulary, WordList};
use problems::{Log, Problems};
use rayon::prelude::*;
use sift::{Features, DESC_LEN};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use analysis::Item;
use claims::{Claim, Edge};
use verify::{Variant, Verdict};

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
    /// subfolders needs `--recursive`. An image both reach belongs to the
    /// nearer folder: an import folder inside the library is scanned, a
    /// library inside a folder scanned is the library. `--exclude` never
    /// leaves out a whole library.
    #[arg(long, value_name = "PATH")]
    against: Vec<PathBuf>,

    /// With `--against`, or `--last` after a scan against a library, print
    /// only the paths of the images with this verdict, one a line, instead of the report: repeat, better, review or
    /// new. Repeat the option, or separate verdicts with commas, for several.
    ///
    /// The report is still written to `--output`, if that names a file.
    #[arg(long, value_enum, value_delimiter = ',', value_name = "VERDICT")]
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
    /// After a scan against a library, its verdicts, or with `--list` their
    /// paths.
    ///
    /// The last scan is the last one run from the command line or the window,
    /// kept beside the cache. Files deleted since are left out, and a DELETE
    /// whose kept copy was one of them is REVIEW; against a library, a repeat
    /// of one is answered by the library alone. A JSON report has no list of
    /// pairs.
    // What it may be given with is `WITH_LAST`.
    #[arg(long)]
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
// all, and the run says so: every file is listed among the skips as an image
// with no detail to match. It exits 0, since a featureless file is a skip
// and not a problem (a folder holding one plain backdrop exited 2 on every
// run). Above that nothing is a threshold: 96 merged no families
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

/// The name each file's cache record is kept under: its canonical path.
///
/// A walk spells a file the way its root was typed, and the cache used to be
/// keyed on that spelling. So one folder scanned as `photos`, as `./photos`
/// and as its absolute path was analysed three times and held three records
/// an image, and a relative record was carried or dropped according to
/// whatever folder the next run happened to start in — `cache::carried` asks
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

// ---------------------------------------------------------------- per image

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

// ---------------------------------------------------------------- output

use stated::{Pair, Stated};
use same::{exact_groups, Same};

// ---------------------------------------------------------------- main

/// The command line: `img-fp`.
pub fn cli_main() -> Result<()> {
    check_cpu()?;
    let args = parse(std::env::args_os()).unwrap_or_else(|e| e.exit());
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
    let args = parse(argv)?;
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
    let args = parse(argv).map_err(|e| e.to_string())?;
    validate(&args)?;
    outputs::plan(&args, true).map(drop).map_err(|e| format!("{e:#}"))
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
    if !args.list.is_empty() && args.against.is_empty() && !args.last {
        return Err("--list needs --against, or --last to list a kept scan run against a library".into());
    }
    if !args.against.is_empty() && args.suggest != suggest::Mode::Content {
        return Err("--against decides with the content rule, and cannot be used with --suggest".into());
    }
    Ok(())
}

/// What `--last` may be given with: how to write the kept scan again, never
/// how to scan. A list of what it takes rather than of what it refuses, so a
/// scan option added later is refused without being named here. It was the
/// other way round, a list of eighteen options to refuse, which every new
/// one had to join.
const WITH_LAST: [&str; 10] = ["last", "null", "list", "output", "format", "threads", "suggest", "cache", "verbose", "log_file"];

/// `argv` as `Args`, refusing what clap's attributes cannot say: `--last`
/// beside an option not in `WITH_LAST`, in clap's words for a conflict.
fn parse<I, T>(argv: I) -> std::result::Result<Args, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut cmd = Args::command();
    let m = cmd.try_get_matches_from_mut(argv)?;
    if m.get_flag("last") {
        let given = |a: &&clap::Arg| m.value_source(a.get_id().as_str()) == Some(clap::parser::ValueSource::CommandLine);
        let refused = cmd.get_arguments().filter(given).find(|a| !WITH_LAST.contains(&a.get_id().as_str())).map(|a| a.to_string());
        if let Some(a) = refused {
            return Err(cmd.error(clap::error::ErrorKind::ArgumentConflict, format!("the argument '--last' cannot be used with '{a}'")));
        }
    }
    Args::from_arg_matches(&m)
}

/// One of the command line's options as a window passes it: its long flag,
/// and its default as the command line takes it (several values joined by
/// commas, a switch's `false`; `None` with no default). A window's defaults
/// are read from here, so the two can never disagree.
pub struct Flag {
    pub long: String,
    pub default: Option<String>,
}

/// The option whose clap id is `id`, if it has a long flag.
pub fn flag(id: &str) -> Option<Flag> {
    // Built, or a switch has no default yet.
    let mut cmd = Args::command();
    cmd.build();
    let a = cmd.get_arguments().find(|a| a.get_id() == id)?;
    let d: Vec<String> = a.get_default_values().iter().map(|v| v.to_string_lossy().into_owned()).collect();
    Some(Flag { long: format!("--{}", a.get_long()?), default: (!d.is_empty()).then(|| d.join(",")) })
}

/// Every option's clap id, for a window's test that it has decided about
/// each one.
pub fn option_ids() -> Vec<String> {
    Args::command().get_arguments().map(|a| a.get_id().to_string()).collect()
}

/// The options `argv` gives, by clap id, each with its values as typed (a
/// switch's is `true`), or clap's message if img-fp would refuse `argv`. A
/// window's tests read back the command lines it makes.
pub fn given<I, T>(argv: I) -> std::result::Result<std::collections::BTreeMap<String, Vec<String>>, String>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut cmd = Args::command();
    let m = cmd.try_get_matches_from_mut(argv).map_err(|e| e.to_string())?;
    Ok(cmd
        .get_arguments()
        .map(|a| a.get_id().as_str())
        .filter(|id| m.value_source(id) == Some(clap::parser::ValueSource::CommandLine))
        .map(|id| {
            let raw = m.get_raw(id).into_iter().flatten().map(|v| v.to_string_lossy().into_owned()).collect();
            (id.to_string(), raw)
        })
        .collect())
}

/// A preview of an image for a window: RGBA, long side at most `long`, or
/// made for a place of a given size.
pub use decode::{fit_preview, preview, preview_fit};

/// Why something failed, cause and all: `{e}` of a context says only the
/// context ("read photo.jpg", not that permission was denied). A cause the
/// text so far already ends with is left out, as the `image` crate's errors
/// say their own cause.
pub fn why(e: &anyhow::Error) -> String {
    let mut out = String::new();
    for cause in e.chain() {
        let t = cause.to_string();
        if out.ends_with(&t) {
            continue;
        }
        if !out.is_empty() {
            out.push_str(": ");
        }
        out.push_str(&t);
    }
    out
}

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
    let outputs = outputs::plan(args, gui.is_some())?;
    // Opened before any work, so that a log file that cannot be written is an
    // ordinary fatal error at the top of the run rather than a discovery made
    // an hour into one.
    let log = Log::open(outputs.log.as_ref())?;
    if args.last {
        let asked = last::Rewrite {
            cache: args.cache.as_deref(),
            report: outputs.report.as_ref(),
            suggest: args.suggest,
            list: &args.list,
            null: args.null,
        };
        for line in last::rewrite(&asked)? {
            progress::to_stderr(&line);
            log.line(&line);
        }
        return Ok(());
    }
    interrupt()?;
    let mut problems = Problems::new(&log);
    let outcome = pipeline::run(args, &outputs, &log, &mut problems, gui);
    problems.print_summary();
    outcome?;
    if problems.any() {
        std::process::exit(EXIT_WITH_PROBLEMS);
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

#[cfg(test)]
mod tests {
    /// A failure says its cause, once: an OS error under a context, and not
    /// the `image` crate's cause again after its own text.
    #[test]
    fn why_says_the_cause_once() {
        use anyhow::Context;
        let io: anyhow::Result<()> = Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)).context("read e.jpg");
        assert_eq!(super::why(&io.unwrap_err()), "read e.jpg: permission denied");
        let png = image::load_from_memory_with_format(b"\x89PNG\r\n\x1a\nnot a png", image::ImageFormat::Png).unwrap_err();
        let one = png.to_string();
        assert_eq!(super::why(&anyhow::Error::new(png)), one);
    }

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
    use verify::Thumb;

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
        let vf = f.seen_as(Variant::TURNED[0]);
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

    /// The window asks before it starts a scan what the scan would refuse:
    /// its worker's stdout is the progress channel.
    #[test]
    fn the_window_is_refused_what_its_worker_would_be() {
        assert_eq!(check_args(["img-fp", "."]), Ok(()));
        for given in [["--dump", "-"], ["--log-file", "-"], ["-o", "-"]] {
            let e = check_args(["img-fp"].into_iter().chain(given).chain(["."])).unwrap_err();
            assert!(e.contains("stdout carries the scan's progress"), "{given:?}: {e}");
        }
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

    /// `--last` rewrites a kept scan: it takes the options that say how,
    /// and refuses every one that says what or how to scan, as the list of
    /// eighteen it replaced did.
    #[test]
    fn last_takes_only_what_rewrites_a_report() {
        let ok = |extra: &[&str]| parse(["img-fp", "--last"].iter().chain(extra)).is_ok();
        assert!(ok(&[]));
        for good in [&["-0", "--list", "new"][..], &["-o", "r.csv"], &["--format", "json"], &["-t", "2"], &["--suggest", "correlation"],
            &["--cache", "/x.bin"], &["-v"], &["--log-file", "/l.txt"]] {
            assert!(ok(good), "{good:?}");
        }
        for bad in [&["."][..], &["--from-file", "f"], &["--against", "/a"], &["-r"], &["-e", "/x"], &["--follow-symlinks"], &["--hidden"],
            &["-x", "png"], &["--work-size", "640"], &["-k", "20"], &["--min-aligned-points", "5"], &["--min-frame-overlap", "0.5"],
            &["--min-pixel-correlation", "0.5"], &["--skip-mirrored"], &["--dump", "d.csv"], &["--no-cache"], &["--clear-cache"],
            &["--prune-cache"], &["--no-last"]] {
            let e = parse(["img-fp", "--last"].iter().chain(bad)).err().unwrap_or_else(|| panic!("{bad:?} accepted"));
            assert_eq!(e.kind(), clap::error::ErrorKind::ArgumentConflict, "{bad:?}");
            assert!(e.to_string().contains("'--last' cannot be used with"), "{e}");
        }
        assert!(parse(["img-fp", "--work-size", "640", "."]).is_ok(), "a default given is only refused beside --last");
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
            let lists = vec![retrieve::quantise(&vocab, &fa), retrieve::quantise(&vocab, &fb)];
            let inv = index::InvertedFile::build(&lists, vocab.n_live_words());
            let (mut acc, mut scored) = (vec![0f32; 2], Vec::new());
            inv.query(&lists[1], 1, &mut acc, &mut scored);
            let mut cands = Vec::new();
            index::shared(&lists[0], &lists[1], &mut cands, 60_000);
            let pair = verify::Pair { fa: &fa, fb: &fb, ta: &ta, tb: &tb };
            let v = verify::verify(&pair, &cands, Variant::SAME, (3, 0.2), &mut Vec::new(), &mut verify::Scratch::default());
            let t = verify::verify_transform(&pair, &v.m, Variant::SAME, 0.2);
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
