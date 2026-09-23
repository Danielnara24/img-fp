//! Where the CPU seconds go, stage by stage. Compiled out unless
//! `--features prof`.
//!
//! Casual timing on this machine is worthless — see *How to measure any of
//! this* in `CLAUDE.md` — but *proportions* within one run are not, because
//! every stage of that run faced the same clock. This is what says which loop
//! to look at; the single-threaded benchmarks in each module's `bench` tests
//! say whether a change to that loop helped.
//!
//! Two things to know about the table it prints. Stages **nest where the code
//! nests**: `decode:jpeg` contains `decode:codec`, and `decode:reduce`
//! contains `decode:fit`, so the column does not add up to the run. And the
//! figures are CPU seconds summed over every worker, so on eight threads they
//! come to several times the wall clock.
#![allow(dead_code)]

pub const N: usize = 44;

#[rustfmt::skip]
pub static NAMES: [&str; N] = [
    // decode
    "decode:read", "decode:codec", "decode:reduce", "decode:fit",
    // extraction
    "thumb", "sift:base", "sift:blur", "sift:extrema", "sift:grad",
    "sift:describe", "sift:sort",
    // retrieval and verification
    "quantise", "vocab", "invfile", "query", "shared", "correspond",
    "best_transform", "encloses", "overlap", "pixel_check", "propagate",
    // decode, by format: each of these *contains* `decode:codec`
    "decode:jpeg", "decode:png", "decode:webp", "decode:tiff", "decode:jxl",
    "decode:heif",
    // assembly. Appended rather than filed with the matcher above, because
    // the decode-format slots are addressed by number from `decode.rs`.
    "group",
    // A second, finer pass (slots 29..): the budget queue, the two halves of
    // description, and the stages outside any worker pool.
    "decode:permit", "sift:ori", "sift:desc", "sift:halve",
    "main:exact", "main:pool", "main:candsort", "main:output", "main:release",
    // A third pass (slots 38..), splitting the two retrieval passes apart.
    // `variant:pass` contains the four slots after it *and* `query`,
    // `shared`, `correspond`, `best_transform`, `encloses`, `overlap` and
    // `pixel_check`; `verify:direct` contains the same seven for the direct
    // pass. The two together are what the matcher spends.
    "variant:pass", "variant:quantise", "variant:mkfeats", "cand:rank",
    "verify:direct",
    // `pixel_check` reached through `verify_transform` — propagation and
    // corroboration — so that the direct pass's share of slot 20 is its own.
    // Note that `propagate` is timed outside its own `par_iter`, so that slot
    // is one thread's wall clock and not a sum over the workers.
    "pixel_check:prop",
];

pub static ACC: [std::sync::atomic::AtomicU64; N] =
    [const { std::sync::atomic::AtomicU64::new(0) }; N];

/// A cycle counter, because a clock read is not free on every machine.
///
/// `Instant::now()` is a `clock_gettime` and the kernel can only serve that
/// from the vDSO when the clocksource is the TSC. This laptop's is the
/// **HPET** — a memory-mapped platform device, shared by every core — so a
/// clock read here costs about **1.5 microseconds** and does not scale with
/// threads. Measured: 2.9 microseconds for the pair of reads `timed!` takes.
///
/// That is longer than most of what this file is asked to measure. A stage
/// called five million times in a run would have been charged fifteen thousand
/// CPU-seconds of clock reads against a run of one thousand, so the table did
/// not merely add overhead — it *ranked by call count*, and the stages it
/// named loudest were the ones called most often rather than the ones doing
/// the work. `rdtsc` is twenty-odd cycles, needs no kernel and needs no lock;
/// the CPU reports `constant_tsc` and `nonstop_tsc`, so it ticks at a fixed
/// rate whatever the core clock does, and one calibration against the wall
/// turns cycles into seconds at the end of the run.
#[cfg(feature = "prof")]
#[inline(always)]
pub fn cycles() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe { core::arch::x86_64::_rdtsc() }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        std::time::Instant::now().elapsed().as_nanos() as u64
    }
}

/// Wall clock and cycle counter at the same moment, taken at the start of the
/// run and again when the table is printed. The ratio is the TSC's rate.
#[cfg(feature = "prof")]
static EPOCH: std::sync::OnceLock<(std::time::Instant, u64)> = std::sync::OnceLock::new();

/// Start the calibration. Without the feature it is nothing.
pub fn start() {
    #[cfg(feature = "prof")]
    {
        let _ = EPOCH.set((std::time::Instant::now(), cycles()));
    }
}

/// Time `$body` into slot `$i`. Without the feature this is `$body` and
/// nothing else — no branch, no counter, no atomic.
#[macro_export]
macro_rules! timed {
    ($i:expr, $body:expr) => {{
        #[cfg(feature = "prof")]
        {
            let t = $crate::prof::cycles();
            let r = $body;
            $crate::prof::ACC[$i].fetch_add(
                $crate::prof::cycles().wrapping_sub(t),
                std::sync::atomic::Ordering::Relaxed,
            );
            r
        }
        #[cfg(not(feature = "prof"))]
        {
            $body
        }
    }};
}

pub fn report() {
    #[cfg(feature = "prof")]
    {
        // Cycles to seconds, from the one calibration this run took.
        let hz = match EPOCH.get() {
            Some(&(t0, c0)) => {
                let secs = t0.elapsed().as_secs_f64();
                let ticks = cycles().wrapping_sub(c0) as f64;
                if secs > 0.05 { ticks / secs } else { 1e9 }
            }
            None => 1e9,
        };
        let mut v: Vec<(f64, &str)> = NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| (ACC[i].load(std::sync::atomic::Ordering::Relaxed) as f64 / hz, *n))
            .filter(|(t, _)| *t > 0.0)
            .collect();
        v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        eprintln!("--- profile (cpu seconds; stages nest; tsc {:.0} MHz) ---", hz / 1e6);
        for (t, n) in v {
            eprintln!("{n:>18}  {t:8.2}");
        }
    }
}
