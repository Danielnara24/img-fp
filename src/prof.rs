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

pub const N: usize = 29;

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
];

pub static ACC: [std::sync::atomic::AtomicU64; N] =
    [const { std::sync::atomic::AtomicU64::new(0) }; N];

/// Time `$body` into slot `$i`. Without the feature this is `$body` and
/// nothing else — no branch, no counter, no atomic.
#[macro_export]
macro_rules! timed {
    ($i:expr, $body:expr) => {{
        #[cfg(feature = "prof")]
        {
            let t = std::time::Instant::now();
            let r = $body;
            $crate::prof::ACC[$i].fetch_add(
                t.elapsed().as_nanos() as u64,
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
        let mut v: Vec<(f64, &str)> = NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| (ACC[i].load(std::sync::atomic::Ordering::Relaxed) as f64 / 1e9, *n))
            .filter(|(t, _)| *t > 0.0)
            .collect();
        v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        eprintln!("--- profile (cpu seconds; stages nest) ---");
        for (t, n) in v {
            eprintln!("{n:>18}  {t:8.2}");
        }
    }
}
