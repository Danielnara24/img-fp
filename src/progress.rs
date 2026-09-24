//! The progress line: one bar on stderr for the whole run, restyled per stage.
//!
//! Modelled on `vid-fp`'s, and the part worth copying is what the percentage
//! measures. The analysis stage — decode and describe, which is most of a
//! cold run — does not count files. A 224-pixel thumbnail and a 44-megapixel
//! JPEG XL differ in cost by a factor of a thousand, so a bar counting files
//! runs fast through a folder of one and crawls through a folder of the other,
//! and its percentage says little about how much of the wait is behind you.
//! It counts *estimated work* instead: each file is weighed before the stage
//! starts, from its header, by `analysis_cost`, so the bar moves at the rate
//! the machine does work rather than the rate files happen to finish.
//!
//! Everything after analysis is a stage with a natural count — images
//! quantised, queries answered, pairs verified — and gets a plainer bar over
//! that count, or a spinner where there is nothing to count. One bar object
//! throughout, so the clock on the left is the run's and never restarts.
//!
//! Drawn only when stderr is a terminal: indicatif hides itself otherwise, so
//! `bench.py`, a pipe and a log file see exactly what they saw before.

use crate::decode::{Kind, Probe};
use image::ImageFormat;
use indicatif::{ProgressBar, ProgressFinish, ProgressState, ProgressStyle};
use std::time::Duration;

/// A box-drawing rule rather than an ASCII pipe, as in `vid-fp`: it cannot be
/// confused with a `|` inside a file name.
const RULE: &str = " \u{2502} ";

pub struct Progress {
    bar: ProgressBar,
}

impl Progress {
    pub fn new() -> Progress {
        // Cleared when dropped, whatever path the run takes out of `run`: a
        // fatal error should read as the error, not as a bar frozen above it.
        let bar = ProgressBar::new(0).with_finish(ProgressFinish::AndClear);
        // Redrawn on a timer as well as on progress, so the clock keeps moving
        // through a stage that reports nothing for a while — a vocabulary
        // build, or one enormous file.
        bar.enable_steady_tick(Duration::from_millis(200));
        let p = Progress { bar };
        p.spin("scanning");
        p
    }

    /// Print a line above the bar rather than through it.
    pub fn println(&self, line: &str) {
        self.bar.suspend(|| eprintln!("{line}"));
    }

    /// A stage with nothing to count.
    pub fn spin(&self, what: &str) {
        self.bar.set_style(
            ProgressStyle::with_template(&format!("{{elapsed_precise}}{RULE}{{spinner}}{RULE}{{msg}}"))
                .unwrap()
                .tick_chars("\u{280b}\u{2819}\u{2839}\u{2838}\u{283c}\u{2834}\u{2826}\u{2827}\u{2807}\u{280f} "),
        );
        self.bar.set_message(what.to_string());
    }

    /// A stage over `len` things called `unit`, labelled `what`.
    pub fn count(&self, what: &str, len: u64, unit: &str) -> &ProgressBar {
        self.restart(len);
        self.bar.set_style(
            ProgressStyle::with_template(&format!(
                "{{elapsed_precise}}{RULE}[{{bar:28.cyan/blue}}]{RULE}{{percent}}%{RULE}{{human_pos}}/{{human_len}} {unit}{RULE}{{msg}}"
            ))
            .unwrap()
            .progress_chars("=>-"),
        );
        self.bar.set_message(what.to_string());
        &self.bar
    }

    /// The analysis stage. The bar's length is `work`, the summed estimate of
    /// what `files` files will cost; see the module note. The prefix is the
    /// file count and the message is the file most recently started.
    pub fn analysis(&self, work: u64, files: usize) -> &ProgressBar {
        self.restart(work);
        // The speedometer divides the work rate by the mean file's weight.
        // That makes it read in images a second — the unit a user can hold up
        // against the file count beside it — while keeping the steadiness of a
        // rate measured in work: a run of large photographs does not drag it
        // down and a run of thumbnails does not spike it. Over the whole stage
        // it averages to exactly the files-per-second the stage achieved.
        let mean = work as f64 / files.max(1) as f64;
        self.bar.set_style(
            ProgressStyle::with_template(&format!(
                "{{elapsed_precise}}{RULE}[{{bar:28.cyan/blue}}]{RULE}{{percent}}%{RULE}{{img_rate}}{RULE}{{prefix}}{RULE}{{msg}}"
            ))
            .unwrap()
            .with_key("img_rate", move |s: &ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = w.write_str(&image_rate(s.per_sec() / mean));
            })
            .progress_chars("=>-"),
        );
        self.bar.set_prefix(format!("0/{files}"));
        self.bar.set_message(String::new());
        &self.bar
    }

    fn restart(&self, len: u64) {
        // `set_position(0)` rather than `reset`, which would restart the clock.
        // The rate estimator is reset with it, so one stage's speed does not
        // bleed into the next one's.
        self.bar.set_length(len);
        self.bar.set_position(0);
        self.bar.reset_eta();
    }

    pub fn finish(&self) {
        self.bar.finish_and_clear();
    }
}

/// The analysis bar's speedometer: three significant figures, no padding, and
/// a dash for no reading — `vid-fp`'s `work_rate`, in images rather than
/// pixels.
pub fn image_rate(per_sec: f64) -> String {
    if !per_sec.is_finite() || per_sec <= 0.0 {
        return "-".to_string();
    }
    let (scaled, unit) = if per_sec >= 1e4 { (per_sec / 1e3, "k img/s") } else { (per_sec, " img/s") };
    let number = if scaled >= 100.0 {
        format!("{scaled:.0}")
    } else if scaled >= 10.0 {
        format!("{scaled:.1}")
    } else {
        format!("{scaled:.2}")
    };
    if number.len() > 4 {
        return "-".to_string();
    }
    format!("{number}{unit}")
}

/// Batches a worker's increments so the bar is touched a few hundred times a
/// stage rather than once per pair. indicatif reads the clock on every `inc`,
/// and on this machine's HPET a clock read is 1.5 microseconds, which is a
/// third of what verifying a pair costs. Flushed on drop, so the last partial
/// batch is always counted and the bar lands on full.
pub struct Ticker<'a> {
    bar: &'a ProgressBar,
    pending: u64,
}

impl<'a> Ticker<'a> {
    const BATCH: u64 = 256;

    pub fn new(bar: &'a ProgressBar) -> Self {
        Ticker { bar, pending: 0 }
    }

    pub fn tick(&mut self) {
        self.pending += 1;
        if self.pending >= Self::BATCH {
            self.bar.inc(self.pending);
            self.pending = 0;
        }
    }
}

impl Drop for Ticker<'_> {
    fn drop(&mut self) {
        if self.pending > 0 {
            self.bar.inc(self.pending);
        }
    }
}

/// What analysing one file is expected to cost, in nanoseconds of one core of
/// the machine the constants were measured on. Only the ratios matter — the
/// bar divides by the sum — so a faster machine changes nothing.
///
/// Measured single-threaded over 3,482 files — the benchmark corpus's
/// `Desktop` subset and its seeds, and the found corpus's `test` folder — by timing
/// decode and extraction per file and fitting each separately:
///
/// - **Decode** is per-format, and for the two formats that are most of any
///   corpus it is best predicted by pixels *and* bytes: JPEG's entropy
///   decoding is paid per compressed byte and its transform per pixel, and
///   the two terms together halve the error of either alone (weighted error
///   0.17 against 0.37 on pixels only). The formats span thirty to one per
///   pixel, from PNG and JPEG at ~4.5 ns to JPEG XL at ~120.
/// - **Extraction** is ~118 ns per pixel of the image the pyramid is built on,
///   which is the working image — the picture fitted to `--work-size` — or, for
///   a small picture, the enlargement of it `upsample_below` asks for. So it is
///   near constant for photographs and rises for thumbnails, and it is most of
///   the cost at the default work size.
///
/// A file the probe could not read is weighed as a JPEG of typical density
/// (0.48 bytes a pixel, the corpus median). It will most likely fail to decode
/// in a millisecond and be over-weighed, which costs the bar a jump.
pub fn analysis_cost(probe: Option<&Probe>, bytes: u64, work: usize, upsample_below: usize) -> u64 {
    let (kind, w, h) = match probe {
        Some(p) => (p.kind, p.w as f64, p.h as f64),
        None => {
            let side = (bytes as f64 / 0.48 / 0.75).sqrt();
            (Kind::Image(ImageFormat::Jpeg), side, side * 0.75)
        }
    };
    // Nanoseconds per source pixel and per file byte.
    let (per_px, per_byte) = match kind {
        Kind::Image(ImageFormat::Jpeg) => (4.4, 22.0),
        Kind::Image(ImageFormat::Png) => (4.5, 8.5),
        Kind::Image(ImageFormat::WebP) => (35.0, 0.0),
        Kind::Image(ImageFormat::Tiff) => (37.0, 0.0),
        Kind::Heif => (40.0, 290.0),
        Kind::Jxl => (123.0, 0.0),
        _ => (10.0, 0.0),
    };
    let decode = per_px * w * h + per_byte * bytes as f64;

    // The pyramid's base, as `decode::fit_to` and `sift::extract` size it.
    let long = w.max(h).max(1.0);
    let s = if work > 0 && long > work as f64 { work as f64 / long } else { 1.0 };
    let (bw, bh) = ((w * s).round().max(1.0), (h * s).round().max(1.0));
    let mut factor = 1.0;
    while bw.max(bh) * factor * 2.0 <= upsample_below.max(2) as f64 {
        factor *= 2.0;
    }
    let extract = 118.0 * bw * bh * factor * factor;

    // Opening the file, the thumbnail, and the rest of what a file costs
    // whatever its size.
    const PER_FILE: f64 = 200_000.0;
    (decode + extract + PER_FILE) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_holds_three_figures_and_climbs_the_ladder() {
        assert_eq!(image_rate(412.3), "412 img/s");
        assert_eq!(image_rate(56.24), "56.2 img/s");
        assert_eq!(image_rate(2.5), "2.50 img/s");
        assert_eq!(image_rate(12_345.0), "12.3k img/s");
        assert_eq!(image_rate(0.0), "-");
        assert_eq!(image_rate(f64::NAN), "-");
    }

    #[test]
    fn cost_follows_the_format_and_the_size() {
        let jpeg = |w, h| Probe { kind: Kind::Image(ImageFormat::Jpeg), w, h };
        let jxl = Probe { kind: Kind::Jxl, w: 4000, h: 3000 };
        let big = analysis_cost(Some(&jpeg(4000, 3000)), 3_000_000, 384, 512);
        let small = analysis_cost(Some(&jpeg(224, 224)), 20_000, 384, 512);
        let jx = analysis_cost(Some(&jxl), 1_000_000, 384, 512);
        assert!(big > 5 * small, "{big} {small}");
        assert!(jx > 2 * big, "{jx} {big}");
        // A thumbnail is enlarged before it is described, so it costs more to
        // describe than its pixels suggest.
        assert!(small > 118 * 448 * 448, "{small}");
    }
}
