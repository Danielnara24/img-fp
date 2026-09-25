//! The progress line: one bar on stderr for the whole run.
//!
//! One bar, not one per stage. A bar that fills and starts again at every
//! stage says how far through *this* stage the run is and nothing about the
//! wait, and the wait is the question. So the run is laid out as one line from
//! 0 to 100%, each stage owns a stretch of it in proportion to what it is
//! expected to cost, and the bar moves through the stretches in order.
//!
//! **Cost is estimated, and the estimates are revised as the run learns.**
//! Before anything is decoded the run knows only how many files there are;
//! after the headers it knows what describing them will cost; after the
//! analysis it knows how many descriptors there are, after verification how
//! many files the second look will re-ask. Each stage takes its stretch when
//! it *begins*, as its share of what is left of the bar under the estimates
//! current at that moment, so a revision changes how fast the rest of the bar
//! moves and never moves the bar backwards or makes it jump. `Forecast` holds
//! what is known and turns it into those estimates.
//!
//! The estimates are in the unit `analysis_cost` uses — nanoseconds of one
//! core of the machine they were measured on — and every stage is parallel, so
//! only their ratios matter. The per-stage constants in `Forecast` were fitted
//! to the stage table of a cached run on the found corpus, with the analysis
//! of a cold run on the same corpus as the yardstick; see `CLAUDE.md`, *What a
//! cached run still has to do*.
//!
//! **Within a stage**, a stage with a natural count — files described,
//! descriptors quantised, pairs verified — moves by that count, weighted by
//! what each item costs where the items differ (a file by `analysis_cost`, an
//! image's queries by its descriptor count). A stage with nothing to count —
//! reading the cache, building the vocabulary — creeps at the rate the counted
//! stages have measured, slowing as it nears the end of its stretch so that a
//! stage slower than its estimate does not run past it.
//!
//! Workers report into atomic counters and a drawing thread turns them into a
//! position twenty times a second, so a worker never takes a lock or reads the
//! clock to say it has done something. (On this machine's HPET a clock read is
//! 1.5 microseconds, a third of what verifying a pair costs, and indicatif
//! reads one on every `inc`.)
//!
//! Drawn only when stderr is a terminal: indicatif hides itself otherwise, so
//! `bench.py`, a pipe and a log file see exactly what they saw before.

use crate::decode::{Kind, Probe};
use image::ImageFormat;
use indicatif::{ProgressBar, ProgressFinish, ProgressState, ProgressStyle};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A box-drawing rule rather than an ASCII pipe, as in `vid-fp`: it cannot be
/// confused with a `|` inside a file name.
const RULE: &str = " \u{2502} ";

/// Positions on the bar. Far more than it has columns, so the percentage
/// beside it moves in tenths.
const LEN: u64 = 1_000_000;

/// How often the drawing thread moves the bar.
const FRAME: Duration = Duration::from_millis(50);

/// The stages of a run, in the order they run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Identical,
    CacheRead,
    Headers,
    Describe,
    CacheWrite,
    Vocabulary,
    Quantise,
    InvertedFile,
    Candidates,
    Verify,
    Variants,
    Propagate,
}

const STAGES: usize = 12;

impl Stage {
    fn label(self) -> &'static str {
        match self {
            Stage::Identical => "finding identical files",
            Stage::CacheRead => "reading the cache",
            Stage::Headers => "reading headers",
            Stage::Describe => "describing",
            Stage::CacheWrite => "writing the cache",
            Stage::Vocabulary => "building the vocabulary",
            Stage::Quantise => "quantising",
            Stage::InvertedFile => "building the inverted file",
            Stage::Candidates => "finding candidates",
            Stage::Verify => "verifying",
            Stage::Variants => "mirrored and inverted",
            Stage::Propagate => "propagating",
        }
    }
}

/// What the run knows so far about how much work is ahead of it. Every field
/// starts as a guess made from the file count and is overwritten when the run
/// learns the real figure; `estimates` turns whatever is there into a cost
/// per stage.
#[derive(Clone, Debug, Default)]
pub struct Forecast {
    pub files: usize,
    /// Size of the cache file about to be read, 0 when there is none.
    pub cache_bytes: u64,
    /// How many files the cache and the exact pass leave to be described.
    pub to_describe: Option<usize>,
    /// Their summed `analysis_cost`, once the headers have been read.
    pub describe: Option<u64>,
    /// Whether the cache will be compacted at the end of the analysis. A run
    /// whose file holds nothing but records worth keeping leaves it alone,
    /// and the stage costs nothing.
    pub cache_write: bool,
    pub images: Option<usize>,
    pub descriptors: Option<usize>,
    pub vocab_sample: Option<usize>,
    pub live_words: Option<usize>,
    pub candidates_per_query: usize,
    pub candidate_pairs: Option<usize>,
    /// Summed `variant_cost` of the files the second look will re-ask.
    pub variants: Option<u64>,
    pub edges: Option<usize>,
}

impl Forecast {
    // Nanoseconds of one core, per unit named. Fitted to the found corpus's
    // `test` folder, cold and cached, so that every stage runs at the same
    // rate in these units to within about 25%; see the module note.
    const HASH_PER_FILE: f64 = 20_000.0;
    const CACHE_READ_PER_BYTE: f64 = 7.4;
    const HEADER_PER_FILE: f64 = 60_000.0;
    /// A file whose header nothing has read yet: a photograph at the default
    /// work size, roughly.
    const DESCRIBE_PER_FILE: f64 = 20e6;
    /// Keypoints per image before the analysis has counted them. `FEATURES`
    /// is the ceiling and most photographs come close to it.
    const KEYPOINTS_PER_IMAGE: f64 = 500.0;
    /// Copying a keypoint's share of a record into a compacted cache, and the
    /// memory handed back after the analysis, which is charged to this stage.
    /// Records are packed by the describing workers now, so this is a copy:
    /// 159,216 keypoints compacted in under the 0.1 s the stage log resolves,
    /// and this is that ceiling rather than a fit. (Deflating them here, as
    /// the stage used to, was 7,500.)
    const CACHE_WRITE_PER_KEYPOINT: f64 = 300.0;
    const VOCAB_PER_SAMPLE: f64 = 26_000.0;
    const VOCAB_SAMPLE: f64 = 160_000.0;
    const QUANTISE_PER_DESC: f64 = 2_700.0;
    const INVERT_PER_DESC: f64 = 150.0;
    const QUERY_PER_POSTING: f64 = 55.0;
    const VERIFY_PER_PAIR: f64 = 10_400.0;
    /// Candidate pairs per image once both directions are merged, as a
    /// fraction of `-k`: 116 of 150 on the found corpus, 105 on the benchmark.
    const PAIRS_PER_QUERY: f64 = 0.75;
    /// Before verification has said which files the second look will re-ask:
    /// the two corpora measured are at 94% and 6%, and the first is the shape
    /// of a real photo library, where most files have no duplicate.
    const LONELY_GUESS: f64 = 0.8;
    /// The describe stage's weights come from `analysis_cost`, which was
    /// fitted single-threaded; at eight threads it runs this much faster than
    /// the rest of the pipeline in the same units.
    const DESCRIBE_SCALE: f64 = 0.8;
    /// The second look's permutations, merge and ranking, over and above its
    /// descents, queries and verdicts.
    const VARIANT_OVERHEAD: f64 = 1.3;
    const EDGES_PER_IMAGE_GUESS: f64 = 2.0;
    const PROPAGATE_PER_EDGE: f64 = 20_000.0;

    fn images(&self) -> f64 {
        self.images.unwrap_or(self.files) as f64
    }

    fn descriptors(&self) -> f64 {
        self.descriptors.map(|d| d as f64).unwrap_or(self.images() * Self::KEYPOINTS_PER_IMAGE)
    }

    /// How many postings a query of one descriptor reads: the corpus's
    /// descriptors spread over its live words.
    fn postings_per_word(&self) -> f64 {
        let words = self.live_words.map(|w| w as f64).unwrap_or(self.descriptors().min(Self::VOCAB_SAMPLE));
        (self.descriptors() / words.max(1.0)).max(1.0)
    }

    /// Quantising an image with `keypoints` descriptors.
    pub fn quantise_cost(&self, keypoints: usize) -> u64 {
        (keypoints as f64 * Self::QUANTISE_PER_DESC) as u64
    }

    /// One retrieval query for an image with `keypoints` descriptors.
    pub fn query_cost(&self, keypoints: usize) -> u64 {
        (keypoints as f64 * self.postings_per_word() * Self::QUERY_PER_POSTING) as u64
    }

    /// The second look at one image: three permuted copies quantised and
    /// queried, and up to `-k` candidates verified.
    pub fn variant_cost(&self, keypoints: usize) -> u64 {
        let verify = (self.candidates_per_query as f64).min(self.images()) * Self::VERIFY_PER_PAIR;
        let cost = 3 * (self.quantise_cost(keypoints) + self.query_cost(keypoints)) + verify as u64;
        (cost as f64 * Self::VARIANT_OVERHEAD) as u64
    }

    fn estimates(&self) -> [f64; STAGES] {
        let files = self.files as f64;
        let images = self.images();
        let desc = self.descriptors();
        let keypoints = (desc / images.max(1.0)) as usize;
        let pairs = self
            .candidate_pairs
            .map(|p| p as f64)
            .unwrap_or(images * (self.candidates_per_query as f64).min(images - 1.0).max(0.0) * Self::PAIRS_PER_QUERY);
        let variants = self
            .variants
            .map(|v| v as f64)
            .unwrap_or(images * Self::LONELY_GUESS * self.variant_cost(keypoints) as f64);
        let edges = self.edges.map(|e| e as f64).unwrap_or(images * Self::EDGES_PER_IMAGE_GUESS);
        let mut e = [0.0; STAGES];
        e[Stage::Identical as usize] = files * Self::HASH_PER_FILE;
        e[Stage::CacheRead as usize] = self.cache_bytes as f64 * Self::CACHE_READ_PER_BYTE;
        e[Stage::Headers as usize] = files * Self::HEADER_PER_FILE;
        e[Stage::Describe as usize] = match (self.describe, self.to_describe) {
            (Some(d), _) => d as f64 * Self::DESCRIBE_SCALE,
            (None, Some(n)) => n as f64 * Self::DESCRIBE_PER_FILE * Self::DESCRIBE_SCALE,
            // A cache of this size answers roughly this many files; the rest
            // are guessed at a typical photograph's cost.
            (None, None) => {
                (files - (self.cache_bytes as f64 / 50_000.0).min(files)) * Self::DESCRIBE_PER_FILE * Self::DESCRIBE_SCALE
            }
        };
        e[Stage::CacheWrite as usize] =
            if self.cache_write { desc * Self::CACHE_WRITE_PER_KEYPOINT } else { 0.0 };
        e[Stage::Vocabulary as usize] =
            self.vocab_sample.map(|s| s as f64).unwrap_or(desc.min(Self::VOCAB_SAMPLE)) * Self::VOCAB_PER_SAMPLE;
        e[Stage::Quantise as usize] = desc * Self::QUANTISE_PER_DESC;
        e[Stage::InvertedFile as usize] = desc * Self::INVERT_PER_DESC;
        e[Stage::Candidates as usize] = desc * self.postings_per_word() * Self::QUERY_PER_POSTING;
        e[Stage::Verify as usize] = pairs * Self::VERIFY_PER_PAIR;
        e[Stage::Variants as usize] = variants;
        e[Stage::Propagate as usize] = edges * Self::PROPAGATE_PER_EDGE;
        e
    }
}

/// What a counted stage's workers report into. Two counts: `weight` moves the
/// bar and `items` is what the message says, because a file is one file on
/// the screen and a thousand times another one's cost on the bar.
#[derive(Default)]
pub struct Counter {
    weight: AtomicU64,
    items: AtomicU64,
}

impl Counter {
    /// One more item done, of the given weight.
    #[inline]
    pub fn add(&self, weight: u64) {
        self.weight.fetch_add(weight, Ordering::Relaxed);
        self.items.fetch_add(1, Ordering::Relaxed);
    }

    /// One more item done, of weight one.
    #[inline]
    pub fn tick(&self) {
        self.add(1);
    }
}

struct Counted {
    counter: Arc<Counter>,
    weight: u64,
    items: u64,
    unit: &'static str,
}

struct Current {
    stage: Stage,
    /// The stretch of the bar this stage owns, as fractions of the whole.
    from: f64,
    span: f64,
    started: Instant,
    counted: Option<Counted>,
    /// For a stage with nothing to count: how long it is expected to take.
    expected: f64,
}

struct State {
    estimates: [f64; STAGES],
    /// Where the bar stands, as a fraction of the whole; never decreases.
    at: f64,
    current: Option<Current>,
    /// Estimated cost and wall time of the counted stages finished so far,
    /// which is the rate an uncounted stage is expected to move at.
    measured_cost: f64,
    measured_secs: f64,
}

impl State {
    fn rate(&self) -> f64 {
        if self.measured_secs >= 0.5 && self.measured_cost > 0.0 {
            self.measured_cost / self.measured_secs
        } else {
            // Until something has been measured: the found corpus's analysis
            // ran at about 2.4e9 of these units a second on eight threads.
            3e8 * rayon::current_num_threads() as f64
        }
    }

    /// Close the running stage: the bar goes to the end of its stretch, and a
    /// counted stage's time goes into the measured rate.
    fn close(&mut self) {
        if let Some(c) = self.current.take() {
            self.at = self.at.max(c.from + c.span);
            if c.counted.is_some() {
                self.measured_cost += self.estimates[c.stage as usize];
                self.measured_secs += c.started.elapsed().as_secs_f64();
            }
        }
    }

    /// Where the running stage has got to, and what to say about it.
    fn frame(&mut self) -> Option<(f64, String)> {
        let c = self.current.as_ref()?;
        let secs = c.started.elapsed().as_secs_f64();
        let (p, msg) = match &c.counted {
            Some(k) => {
                let w = k.counter.weight.load(Ordering::Relaxed);
                let n = k.counter.items.load(Ordering::Relaxed).min(k.items);
                let p = if k.weight == 0 { 1.0 } else { (w as f64 / k.weight as f64).min(1.0) };
                let mut msg = format!("{} {}/{} {}", c.stage.label(), n, k.items, k.unit);
                if c.stage == Stage::Describe {
                    msg.push_str(RULE);
                    msg.push_str(&image_rate(n as f64 / secs));
                }
                (p, msg)
            }
            None => (creep(secs / c.expected.max(1e-3)), c.stage.label().to_string()),
        };
        let at = (c.from + c.span * p).max(self.at);
        self.at = at;
        Some((at, msg))
    }
}

/// How far through its stretch an uncounted stage is shown, `x` being the
/// time it has taken over the time it was expected to take: in step with the
/// clock to 90%, then closing on the end without reaching it.
fn creep(x: f64) -> f64 {
    const KNEE: f64 = 0.9;
    if x <= KNEE {
        x
    } else {
        KNEE + (1.0 - KNEE) * (1.0 - (-(x - KNEE) / (1.0 - KNEE)).exp())
    }
}

pub struct Progress {
    bar: ProgressBar,
    state: Arc<Mutex<State>>,
    forecast: Mutex<Forecast>,
    stop: Arc<AtomicBool>,
    drawer: Mutex<Option<JoinHandle<()>>>,
}

impl Progress {
    pub fn new() -> Progress {
        // Cleared when dropped, whatever path the run takes out of `run`: a
        // fatal error should read as the error, not as a bar frozen above it.
        let bar = ProgressBar::new(LEN).with_finish(ProgressFinish::AndClear);
        let _ = BAR.set(bar.clone());
        bar.set_style(
            ProgressStyle::with_template(&format!(
                "{{elapsed_precise}}{RULE}[{{bar:28.cyan/blue}}]{RULE}{{pct}}{RULE}{{msg}}"
            ))
            .unwrap()
            .with_key("pct", |s: &ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = write!(w, "{:5.1}%", s.fraction() * 100.0);
            })
            .progress_chars("=>-"),
        );
        bar.set_message("scanning");
        // Redrawn on a timer as well as on progress, so the clock keeps moving
        // through a stretch where the bar does not.
        bar.enable_steady_tick(Duration::from_millis(200));
        let state = Arc::new(Mutex::new(State {
            estimates: [0.0; STAGES],
            at: 0.0,
            current: None,
            measured_cost: 0.0,
            measured_secs: 0.0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let drawer = {
            let (bar, state, stop) = (bar.clone(), state.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(FRAME);
                    let frame = state.lock().unwrap().frame();
                    if let Some((at, msg)) = frame {
                        bar.set_position((at * LEN as f64) as u64);
                        bar.set_message(msg);
                    }
                }
            })
        };
        Progress { bar, state, forecast: Mutex::new(Forecast::default()), stop, drawer: Mutex::new(Some(drawer)) }
    }

    /// Print a line above the bar rather than through it.
    pub fn println(&self, line: &str) {
        self.bar.suspend(|| eprintln!("{line}"));
    }

    /// Revise what the run knows about the work ahead. Takes effect from the
    /// next stage to begin; the running one keeps the stretch it was given.
    pub fn forecast(&self, f: impl FnOnce(&mut Forecast)) {
        let mut fc = self.forecast.lock().unwrap();
        f(&mut fc);
        self.state.lock().unwrap().estimates = fc.estimates();
    }

    /// A copy of the forecast, for costing items with its `*_cost` methods.
    pub fn forecast_now(&self) -> Forecast {
        self.forecast.lock().unwrap().clone()
    }

    fn start(&self, stage: Stage, counted: Option<Counted>) {
        let mut s = self.state.lock().unwrap();
        s.close();
        let remaining: f64 = s.estimates[stage as usize..].iter().sum();
        let own = s.estimates[stage as usize];
        let span = if remaining > 0.0 { (1.0 - s.at) * own / remaining } else { 0.0 };
        let expected = own / s.rate();
        let (from, label) = (s.at, stage.label());
        s.current = Some(Current { stage, from, span, started: Instant::now(), counted, expected });
        drop(s);
        self.bar.set_message(label);
    }

    /// A stage with nothing to count.
    pub fn begin(&self, stage: Stage) {
        self.start(stage, None);
    }

    /// A stage over `items` things called `unit`, whose weights sum to
    /// `weight`. Workers report each item into the returned counter.
    pub fn begin_counted(&self, stage: Stage, weight: u64, items: usize, unit: &'static str) -> Arc<Counter> {
        let counter = Arc::new(Counter::default());
        self.start(stage, Some(Counted { counter: counter.clone(), weight, items: items as u64, unit }));
        counter
    }

    /// The whole run is done: stop drawing and clear the line.
    pub fn finish(&self) {
        self.state.lock().unwrap().close();
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.drawer.lock().unwrap().take() {
            let _ = h.join();
        }
        self.bar.finish_and_clear();
    }
}

/// The run's bar, for the one caller that cannot reach the `Progress`: an
/// interrupt, which exits from a thread of its own without unwinding `run`.
static BAR: std::sync::OnceLock<ProgressBar> = std::sync::OnceLock::new();

/// Clear the bar, so that a process exiting on a signal leaves its last word
/// on a clean line rather than after half a bar.
pub fn clear_for_exit() {
    if let Some(bar) = BAR.get() {
        bar.finish_and_clear();
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.finish();
    }
}

/// The describing stage's speedometer: three significant figures, no padding, and
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
