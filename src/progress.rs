//! The progress line: one bar on stderr for the whole run.
//!
//! One bar, not one per stage. A bar that fills and starts again at every
//! stage says how far through *this* stage the run is and nothing about the
//! wait, and the wait is the question. So the bar is the share of the run's
//! time that has passed, as well as the run can tell it.
//!
//! **The bar is driven by the time left.** Every frame the run works out how
//! long it has still to go (`State::left`), and the bar closes that share of
//! the distance to its end: a frame of `dt` seconds with `left` to go moves it
//! `dt / left` of the way from where it stands to 100%. When `left` is right
//! the bar moves in step with the clock; when it is revised the bar speeds up
//! or slows down from where it is, and it never jumps, moves back or reaches
//! the end before the run does. Nothing reserves a stretch of the bar for a
//! stage before it runs, so nothing has to guess how long a stretch is.
//!
//! **The time left is counted work at a measured pace.** Each stage's work is
//! counted in its own unit (files, bytes, descriptors, pairs) as soon as the
//! run knows it (`Known`), and until then it is the most the code can make of
//! what is known: a descriptor count is at most `FEATURES` an image, a
//! candidate list at most `-k`. What a unit costs is measured:
//!
//! - **the running stage** by itself: the time it has taken over the work it
//!   has done;
//! - **the analysis**, which is most of a first run, file by file: each
//!   file's decode is timed against its source pixels, by format, and the
//!   rest against the pixels it is analysed at (`Describing`). A format
//!   thirty times slower than another is learnt from its first files, not
//!   written down;
//! - **a stage that has run** (a library's later rounds, and the second look,
//!   which is quantising, querying and verifying again) by what it took;
//! - **a stage that has not run yet** by its `Stage::prior`, the one number
//!   per stage that is not of this run: what a unit took on the machine the
//!   priors were measured on, scaled by how fast this machine has been on
//!   everything measured so far (`Paces::speed`). They shape the bar only
//!   until the stage begins, and a wrong one shows as the bar changing speed.
//!
//! Workers report into atomic counters and a drawing thread turns them into a
//! position twenty times a second, so a worker never takes a lock to say it
//! has done something; only a file's analysis, which takes milliseconds, is
//! timed by its worker. (On this machine's HPET a clock read is 1.5
//! microseconds, a third of what verifying a pair costs.)
//!
//! Drawn only when stderr is a terminal: indicatif hides itself otherwise, so
//! `bench.py`, a pipe and a log file see exactly what they saw before.
//!
//! **Under `img-fp-gui` the line is spoken rather than drawn** (`speak_json`):
//! the same position and message, one JSON object a frame on stdout, and every
//! line the run would have printed above the bar as another. The window reads
//! them from the pipe; nothing else about the run changes.

use crate::decode::{Kind, Probe};
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

/// Set once, before the run starts, by the window's worker.
static JSON: AtomicBool = AtomicBool::new(false);

/// Speak the progress line as JSON on stdout instead of drawing it on stderr.
///
/// Two kinds of line, one object each: `{"p": 0.431, "m": "describing ..."}`
/// for where the bar stands, sent when it moves, and `{"l": "..."}` for a line
/// the run says about itself.
pub fn speak_json() {
    JSON.store(true, Ordering::Relaxed);
}

fn speaking() -> bool {
    JSON.load(Ordering::Relaxed)
}

/// One line to the window. A window that has gone away is not the run's
/// problem: the worker is told to stop by a signal, not by a broken pipe.
/// A line on stderr, or nothing if nobody is reading it.
///
/// `eprintln!` panics when the write fails, and a write to a pipe whose reader
/// has gone fails: `img-fp DIR -o r.json 2>&1 | head -1` took the first line
/// and then aborted the run at the second, before the report it was asked to
/// write, and with the appends to the cache after that point lost. The report
/// on stdout already took a broken pipe as the end of a pipeline rather than a
/// failed run; what the run says about itself on stderr now does too.
pub fn to_stderr(line: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

fn speak(v: serde_json::Value) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, &v);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

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
    Assemble,
}

const STAGES: usize = Stage::ALL.len();

impl Stage {
    const ALL: [Stage; 13] = [
        Stage::Identical,
        Stage::CacheRead,
        Stage::Headers,
        Stage::Describe,
        Stage::CacheWrite,
        Stage::Vocabulary,
        Stage::Quantise,
        Stage::InvertedFile,
        Stage::Candidates,
        Stage::Verify,
        Stage::Variants,
        Stage::Propagate,
        Stage::Assemble,
    ];

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
            Stage::Assemble => "grouping and suggesting",
        }
    }

    /// The unit of the stage's work, as `Known::units` counts it.
    fn unit(self) -> &'static str {
        match self {
            Stage::Identical | Stage::Headers => "files",
            Stage::CacheRead => "bytes",
            Stage::Describe => "pixels analysed",
            Stage::CacheWrite | Stage::Quantise | Stage::InvertedFile | Stage::Candidates => "descriptors",
            Stage::Vocabulary => "samples",
            Stage::Verify => "pairs",
            Stage::Variants => "ns of the stages it repeats",
            Stage::Propagate | Stage::Assemble => "images and anchors",
        }
    }

    /// Whether the stage's pace is the processor's. Finding identical files,
    /// reading headers and reading and writing the cache wait on the disk
    /// and the page cache, which say nothing of how fast the matching will
    /// go: a rescan's cache read, at 1.6 to 2.4 times its prior under memory
    /// pressure, had every stage after it expected at twice what it took.
    fn computes(self) -> bool {
        !matches!(self, Stage::Identical | Stage::CacheRead | Stage::Headers | Stage::CacheWrite)
    }

    /// What a unit of the stage's work cost on the machine these were
    /// measured on (a Ryzen 7 3700U at eight threads), in nanoseconds of
    /// each thread: the stage's wall time, times eight, over its units. Only
    /// their ratios and their ratio to `EXTRACT_PER_PIXEL` matter. Read off
    /// the `-v` pace lines of first runs and rescans of IMGS, the found
    /// corpus, IMGS-ELONG and `derived/Desktop` at the default settings: the
    /// middle of what they measured, whose spread is given. A stage's own
    /// pace replaces its prior as soon as it runs; see CLAUDE.md, *The
    /// progress bar*. `None` for the second look, which is priced by the
    /// stages it repeats (`Paces::ask_again`).
    fn prior(self) -> Option<f64> {
        Some(match self {
            // 12-210 k: a size alone settles most files.
            Stage::Identical => 130_000.0,
            // 44-48.
            Stage::CacheRead => 46.0,
            // A header read, 200-550 k (a cached file reads none).
            Stage::Headers => 400_000.0,
            // 250-400: small JPEGs decode for little, photographs for more.
            Stage::Describe => 360.0,
            // Not measured: no run above rewrote its cache. A copy of the
            // records' bytes.
            Stage::CacheWrite => 300.0,
            // 36-66 k, the more the larger the corpus.
            Stage::Vocabulary => 60_000.0,
            // 3.2-9.8 k, deeper trees for larger corpora.
            Stage::Quantise => 7_000.0,
            // 190-1,100.
            Stage::InvertedFile => 400.0,
            // 300-1,650.
            Stage::Candidates => 650.0,
            // 15-220 k: a pair the geometry rejects costs little, one whose
            // pixels are read costs more.
            Stage::Verify => 43_000.0,
            Stage::Variants => return None,
            // Per image and per direct anchor, which cost about the same:
            // 40 k an image on the found corpus (one anchor in twenty
            // images), 41 k an anchor on IMGS (thirty an image).
            Stage::Propagate => 40_000.0,
            // Likewise: 86 k an image on the found corpus, 75 k an anchor
            // on IMGS.
            Stage::Assemble => 80_000.0,
        })
    }
}

/// What everything but the decode of one file's analysis cost per pixel
/// analysed, on the machine `Stage::prior` was measured on and in the same
/// unit (200-243 on the runs it was read from): how this machine's speed is
/// read off its first files (`Describing`).
const EXTRACT_PER_PIXEL: f64 = 220.0;

/// The most descriptors an image is given (`lib::FEATURES`): what an image's
/// count is taken to be until the analysis has counted it.
const FEATURES: f64 = crate::FEATURES as f64;

/// What the run has counted so far of the work ahead. A field is a count the
/// run has made, or nothing yet; `units` makes the most of whichever.
#[derive(Clone, Debug, Default)]
pub struct Known {
    pub files: usize,
    /// Size of the cache file about to be read, 0 when there is none.
    pub cache_bytes: u64,
    /// Whether the cache will be rewritten at the end of the analysis.
    pub cache_write: bool,
    /// `--work-size` and `-k`.
    pub work_size: usize,
    pub candidates: usize,
    /// How many files the cache and the exact pass leave to be described.
    pub to_describe: Option<usize>,
    /// The files the matching stages are handed, and their descriptors.
    pub images: Option<usize>,
    pub descriptors: Option<usize>,
    /// The anchors the matching has found.
    pub anchors: Option<usize>,
    /// The files the second look asks again, and their descriptors.
    pub ask_again: Option<(usize, usize)>,
    /// Until then, the files and descriptors verification has already put
    /// past being asked again (`Scope::settled`), which only grows.
    pub settled: (usize, usize),
    /// The matching is the last run's, taken whole (`matched.rs`): its
    /// stages have nothing to do.
    pub matched_before: bool,
}

impl Known {
    fn images(&self) -> f64 {
        self.images.unwrap_or(self.files) as f64
    }

    fn descriptors(&self) -> f64 {
        self.descriptors.map_or(self.images() * FEATURES, |d| d as f64)
    }

    /// The files the second look asks again and their descriptors: as
    /// counted, or every file verification has not yet settled.
    fn ask_again(&self) -> (f64, f64) {
        let (n, d) = self.settled;
        self.ask_again.map_or(((self.images() - n as f64).max(0.0), (self.descriptors() - d as f64).max(0.0)), |(n, d)| (n as f64, d as f64))
    }

    /// The stage's work in its unit: what has been counted, or the most the
    /// code allows of what has.
    fn units(&self, s: Stage) -> f64 {
        let (files, images, desc) = (self.files as f64, self.images(), self.descriptors());
        let to_describe = self.to_describe.map_or(files, |n| n as f64);
        if self.matched_before && !matches!(s, Stage::Identical | Stage::CacheRead | Stage::Headers | Stage::Describe | Stage::CacheWrite | Stage::Assemble) {
            return 0.0;
        }
        match s {
            Stage::Identical => files,
            Stage::CacheRead => self.cache_bytes as f64,
            Stage::Headers => files,
            // No picture is analysed at more pixels than a square one at the
            // working size (the area rule gives a wider one a 4:3 one's).
            // With no working size, a twelve-megapixel photograph.
            Stage::Describe => {
                let work = self.work_size as f64;
                to_describe * if work > 0.0 { work * work } else { 12e6 }
            }
            Stage::CacheWrite => {
                if self.cache_write {
                    desc
                } else {
                    0.0
                }
            }
            Stage::Vocabulary => (crate::index::VocabParams::for_corpus(desc as usize).sample as f64).min(desc),
            Stage::Quantise | Stage::InvertedFile | Stage::Candidates => desc,
            Stage::Verify => images * (self.candidates as f64).min(images - 1.0).max(0.0),
            Stage::Variants => self.ask_again().0,
            // Nothing bounds the anchors before the matching has counted
            // them: none is taken for granted.
            Stage::Propagate | Stage::Assemble => images + self.anchors.unwrap_or(0) as f64,
        }
    }
}

/// What one file's analysis is expected to be: the picture's format (`None`
/// when its header would not read), its source pixels (its bytes, when
/// there is no header to say), and the pixels it is analysed at.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileCost {
    kind: Option<Kind>,
    source: u64,
    analysed: u64,
}

/// What analysing a file will be, from its header: `decode::working_size`
/// fitted to `--work-size` and `sift::enlargement` of that, as the analysis
/// itself sizes it.
pub fn file_cost(probe: Option<&Probe>, bytes: u64, work: usize, upsample_below: usize) -> FileCost {
    let Some(p) = probe else { return FileCost { kind: None, source: bytes, analysed: 0 } };
    let (w, h) = crate::decode::working_size(p.w as usize, p.h as usize, work);
    let f = crate::sift::enlargement(w.max(h), crate::analysis::enlarge_below(work, upsample_below));
    FileCost { kind: Some(p.kind), source: p.w as u64 * p.h as u64, analysed: (w * h * f * f) as u64 }
}

/// The analysis as it goes, file by file: what is left of it in each format,
/// and what each format's decode and the rest have cost so far.
pub struct Describing {
    kinds: Vec<Option<Kind>>,
    /// By format: source pixels in all, done, and the decode time they took.
    source: Vec<u64>,
    source_done: Vec<AtomicU64>,
    decode_ns: Vec<AtomicU64>,
    /// Pixels analysed in all and done, and the time the rest of each
    /// file's analysis took.
    analysed: u64,
    analysed_done: AtomicU64,
    rest_ns: AtomicU64,
    files: u64,
    files_done: AtomicU64,
}

impl Describing {
    fn new(costs: &[FileCost]) -> Describing {
        let mut kinds: Vec<Option<Kind>> = Vec::new();
        for c in costs {
            if !kinds.contains(&c.kind) {
                kinds.push(c.kind);
            }
        }
        let mut source = vec![0u64; kinds.len()];
        for c in costs {
            source[kinds.iter().position(|k| *k == c.kind).unwrap()] += c.source;
        }
        let zero = |n: usize| (0..n).map(|_| AtomicU64::new(0)).collect::<Vec<_>>();
        Describing {
            source_done: zero(kinds.len()),
            decode_ns: zero(kinds.len()),
            kinds,
            source,
            analysed: costs.iter().map(|c| c.analysed).sum(),
            analysed_done: AtomicU64::new(0),
            rest_ns: AtomicU64::new(0),
            files: costs.len() as u64,
            files_done: AtomicU64::new(0),
        }
    }

    /// One file analysed: its decode took `decode`, the whole of it `total`.
    pub fn done(&self, c: &FileCost, decode: Duration, total: Duration) {
        let k = self.kinds.iter().position(|k| *k == c.kind).expect("a file of the plan");
        // A file whose header would not read is all decode: there is no
        // count of what it is analysed at.
        let (decode, rest) = if c.kind.is_none() { (total, Duration::ZERO) } else { (decode, total.saturating_sub(decode)) };
        self.source_done[k].fetch_add(c.source, Ordering::Relaxed);
        self.decode_ns[k].fetch_add(decode.as_nanos() as u64, Ordering::Relaxed);
        self.analysed_done.fetch_add(c.analysed, Ordering::Relaxed);
        self.rest_ns.fetch_add(rest.as_nanos() as u64, Ordering::Relaxed);
        self.files_done.fetch_add(1, Ordering::Relaxed);
    }

    /// Seconds left on `threads` threads, from what the files done so far
    /// took; `None` until a file is done. A format none of whose files is
    /// done yet decodes at the pace of the formats seen.
    fn left(&self, threads: f64) -> Option<f64> {
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed) as f64;
        if load(&self.files_done) == 0.0 {
            return None;
        }
        let decoded: Vec<(f64, f64)> = (0..self.kinds.len()).map(|k| (load(&self.source_done[k]), load(&self.decode_ns[k]))).collect();
        let seen = decoded.iter().zip(&self.kinds).filter(|(d, k)| k.is_some() && d.0 > 0.0);
        let (px, ns) = seen.fold((0.0, 0.0), |a, ((p, n), _)| (a.0 + p, a.1 + n));
        let any = if px > 0.0 { ns / px } else { 0.0 };
        let mut ns_left = 0.0;
        for (k, &(done, ns)) in decoded.iter().enumerate() {
            let rate = if done > 0.0 { ns / done } else if self.kinds[k].is_some() { any } else { 0.0 };
            ns_left += rate * (self.source[k] as f64 - done).max(0.0);
        }
        let analysed = load(&self.analysed_done);
        if analysed > 0.0 {
            ns_left += load(&self.rest_ns) / analysed * (self.analysed as f64 - analysed).max(0.0);
        }
        // Thread time to wall time: a file's time was taken on a loaded
        // machine, waiting for the decode budget included, so the run's
        // threads share what is left while there are files for them all.
        let files_left = (self.files as f64 - load(&self.files_done)).max(1.0);
        Some(ns_left / 1e9 / threads.min(files_left))
    }

    /// The wall time of the files done that went on more than decoding, and
    /// what it would have been on the priors' machine: this machine's speed,
    /// as `Paces` reads it.
    fn speed(&self, secs: f64) -> Option<(f64, f64)> {
        let load = |a: &AtomicU64| a.load(Ordering::Relaxed) as f64;
        let rest = load(&self.rest_ns);
        let spent = rest + self.decode_ns.iter().map(load).sum::<f64>();
        let analysed = load(&self.analysed_done);
        (rest > 0.0 && analysed > 0.0).then(|| (secs * rest / spent, analysed * EXTRACT_PER_PIXEL))
    }
}

/// What the run has measured of each stage's pace, and of this machine's
/// speed against the priors'.
struct Paces {
    /// Wall seconds and units of work done, by stage.
    secs: [f64; STAGES],
    units: [f64; STAGES],
    /// From the analysis: wall seconds, and the priors' nanoseconds for them.
    analysis: Option<(f64, f64)>,
    threads: f64,
}

impl Paces {
    /// Wall seconds a prior's nanosecond takes on this machine: everything
    /// the processor has done so far against what the priors say it would
    /// have cost, or, before it has done anything, the priors' machine at
    /// this run's threads.
    fn speed(&self) -> f64 {
        let (mut secs, mut ns) = self.analysis.unwrap_or((0.0, 0.0));
        // The analysis is read past its decode, whose pace is the format's,
        // and no stage that waits on the disk is read at all.
        for s in Stage::ALL.into_iter().filter(|&s| s.computes() && s != Stage::Describe) {
            if let Some(p) = s.prior() {
                secs += self.secs[s as usize];
                ns += self.units[s as usize] * p;
            }
        }
        if secs >= 0.2 && ns > 0.0 { secs / ns } else { 1e-9 / self.threads }
    }

    /// Seconds a unit of the stage takes: as measured when it has run long
    /// enough to say, else its prior at this machine's speed.
    fn pace(&self, s: Stage) -> f64 {
        let (secs, units) = (self.secs[s as usize], self.units[s as usize]);
        if secs >= 0.2 && units > 0.0 {
            secs / units
        } else {
            s.prior().unwrap_or(0.0) * self.speed()
        }
    }

    /// Seconds the second look takes over a file of `keypoints` descriptors:
    /// three turned copies quantised and asked, and up to `-k` of what they
    /// find verified (`pipeline::second_look`).
    fn ask_again(&self, keypoints: f64, verified: f64) -> f64 {
        3.0 * keypoints * (self.pace(Stage::Quantise) + self.pace(Stage::Candidates)) + verified * self.pace(Stage::Verify)
    }
}

/// What is left of a stage expected to take `expected` seconds, `secs` into
/// it: the difference until half of it has passed, then `expected² / 4
/// secs`, which meets it there with the same slope and never reaches zero.
/// A stage that runs past its estimate slows the bar down rather than
/// sending it to the end.
fn overrun(expected: f64, secs: f64) -> f64 {
    if secs <= expected / 2.0 { expected - secs } else { expected * expected / (4.0 * secs) }
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

/// How the running stage's progress is read.
enum Work {
    /// By what it was expected to take: nothing to count inside it.
    Uncounted,
    /// Items done of `items`, weighing `weight` in all; `per` seconds a unit
    /// of weight before anything is done.
    Counted { counter: Arc<Counter>, weight: u64, items: u64, unit: &'static str, per: f64 },
    Describing(Arc<Describing>),
}

struct Current {
    stage: Stage,
    started: Instant,
    work: Work,
    /// The stage's work in its unit (`Known::units`, or the counted weight).
    units: f64,
}

struct State {
    known: Known,
    paces: Paces,
    /// Stages that have run, and the one running.
    done: [bool; STAGES],
    current: Option<Current>,
    /// Where the bar stands, as a fraction of the whole; never decreases.
    at: f64,
    last: Instant,
}

impl State {
    /// Seconds the stage would take, from what is known now.
    fn expected(&self, s: Stage) -> f64 {
        let k = &self.known;
        if s == Stage::Variants {
            // Which files the second look asks again is decided by what
            // verification finds: until it has, all of them, and as it goes,
            // all it has not yet settled. In a photo library most files have
            // no duplicate and nearly all are asked (97% of the found
            // corpus); in a folder of copies (6% of IMGS) the bound falls as
            // the copies anchor.
            let (n, d) = k.ask_again();
            let verified = (k.candidates as f64).min(k.images());
            return self.paces.ask_again(d, n * verified);
        }
        k.units(s) * self.paces.pace(s)
    }

    /// Seconds the run has still to go: the rest of the running stage, and
    /// every stage after it not yet run.
    fn left(&self, now: Instant) -> f64 {
        let (from, running) = match &self.current {
            None => (0, 0.0),
            Some(c) => {
                let secs = now.duration_since(c.started).as_secs_f64();
                let left = match &c.work {
                    Work::Uncounted => overrun(self.expected(c.stage), secs),
                    Work::Counted { counter, weight, per, .. } => {
                        let w = counter.weight.load(Ordering::Relaxed) as f64;
                        let rest = (*weight as f64 - w).max(0.0);
                        if w > 0.0 && secs >= 0.1 { secs * rest / w } else { rest * per }
                    }
                    Work::Describing(d) => d.left(self.paces.threads).unwrap_or_else(|| overrun(self.expected(Stage::Describe), secs)),
                };
                (c.stage as usize + 1, left.max(0.0))
            }
        };
        running + Stage::ALL[from..].iter().filter(|s| !self.done[**s as usize]).map(|&s| self.expected(s)).sum::<f64>()
    }

    /// Close the running stage, and keep what it measured.
    fn close(&mut self) {
        let Some(c) = self.current.take() else { return };
        let i = c.stage as usize;
        self.done[i] = true;
        let secs = c.started.elapsed().as_secs_f64();
        match &c.work {
            Work::Describing(d) => {
                self.paces.analysis = d.speed(secs);
                self.paces.secs[i] += secs;
                self.paces.units[i] += d.analysed_done.load(Ordering::Relaxed) as f64;
            }
            Work::Counted { counter, .. } if c.stage != Stage::Variants => {
                self.paces.secs[i] += secs;
                self.paces.units[i] += counter.weight.load(Ordering::Relaxed) as f64;
            }
            Work::Counted { .. } => {}
            Work::Uncounted => {
                self.paces.secs[i] += secs;
                self.paces.units[i] += c.units;
            }
        }
    }

    /// Where the bar stands now, and what to say about it.
    fn frame(&mut self) -> Option<(f64, String)> {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        let c = self.current.as_ref()?;
        // The analysis says how fast this machine is as it goes.
        if let Work::Describing(d) = &c.work {
            if let Some(a) = d.speed(now.duration_since(c.started).as_secs_f64()) {
                self.paces.analysis = Some(a);
            }
        }
        let left = self.left(now).max(0.05);
        self.at += (1.0 - self.at) * (dt / left).min(0.5);
        let msg = match &c.work {
            Work::Uncounted => c.stage.label().to_string(),
            Work::Counted { counter, items, unit, .. } => {
                let n = counter.items.load(Ordering::Relaxed).min(*items);
                format!("{} {}/{} {}", c.stage.label(), n, items, unit)
            }
            Work::Describing(d) => {
                let n = d.files_done.load(Ordering::Relaxed);
                let rate = n as f64 / now.duration_since(c.started).as_secs_f64();
                format!("{} {}/{} images{RULE}{}", c.stage.label(), n, d.files, image_rate(rate))
            }
        };
        Some((self.at, msg))
    }
}

pub struct Progress {
    bar: ProgressBar,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    drawer: Mutex<Option<JoinHandle<()>>>,
}

impl Progress {
    pub fn new() -> Progress {
        // Cleared when dropped, whatever path the run takes out of `run`: a
        // fatal error should read as the error, not as a bar frozen above it.
        let bar = if speaking() { ProgressBar::hidden() } else { ProgressBar::new(LEN) };
        let bar = bar.with_finish(ProgressFinish::AndClear);
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
            known: Known::default(),
            paces: Paces {
                secs: [0.0; STAGES],
                units: [0.0; STAGES],
                analysis: None,
                threads: rayon::current_num_threads() as f64,
            },
            done: [false; STAGES],
            current: None,
            at: 0.0,
            last: Instant::now(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let drawer = {
            let (bar, state, stop) = (bar.clone(), state.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut said = (u64::MAX, String::new());
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(FRAME);
                    let frame = state.lock().unwrap().frame();
                    if let Some((at, msg)) = frame {
                        let pos = (at * LEN as f64) as u64;
                        if speaking() {
                            if (pos, &msg) != (said.0, &said.1) {
                                speak(serde_json::json!({"p": at, "m": msg}));
                                said = (pos, msg);
                            }
                            continue;
                        }
                        bar.set_position(pos);
                        bar.set_message(msg);
                    }
                }
            })
        };
        Progress { bar, state, stop, drawer: Mutex::new(Some(drawer)) }
    }

    /// Print a line above the bar rather than through it.
    pub fn println(&self, line: &str) {
        if speaking() {
            return speak(serde_json::json!({"l": line}));
        }
        self.bar.suspend(|| to_stderr(line));
    }

    /// Note what the run has counted of the work ahead.
    pub fn learn(&self, f: impl FnOnce(&mut Known)) {
        f(&mut self.state.lock().unwrap().known);
    }

    fn start(&self, stage: Stage, work: impl FnOnce(&State) -> (Work, f64)) {
        let mut s = self.state.lock().unwrap();
        s.close();
        let (work, units) = work(&s);
        s.current = Some(Current { stage, started: Instant::now(), work, units });
        drop(s);
        self.bar.set_message(stage.label());
    }

    /// A stage with nothing to count.
    pub fn begin(&self, stage: Stage) {
        self.start(stage, |s| (Work::Uncounted, s.known.units(stage)));
    }

    /// A stage over `items` things called `unit`, whose weights sum to
    /// `weight`. Workers report each item into the returned counter. Begun
    /// again (a library's later rounds), it counts the round's work.
    pub fn begin_counted(&self, stage: Stage, weight: u64, items: usize, unit: &'static str) -> Arc<Counter> {
        let counter = Arc::new(Counter::default());
        let c = counter.clone();
        self.start(stage, move |s| {
            // A second look's weights are nanoseconds; every other stage's
            // are its units.
            let per = if stage == Stage::Variants { 1e-9 } else { s.paces.pace(stage) };
            (Work::Counted { counter: c, weight, items: items as u64, unit, per }, weight as f64)
        });
        counter
    }

    /// The analysis of the files `costs` describes, which its workers report
    /// into file by file (`Describing::done`).
    pub fn begin_describe(&self, costs: &[FileCost]) -> Arc<Describing> {
        let d = Arc::new(Describing::new(costs));
        let units = costs.iter().map(|c| c.analysed as f64).sum();
        let w = d.clone();
        self.start(Stage::Describe, move |_| (Work::Describing(w), units));
        d
    }

    /// What the second look over a file of `keypoints` descriptors is
    /// expected to cost, in nanoseconds: its weight on the bar.
    pub fn ask_again_cost(&self, keypoints: usize) -> u64 {
        let s = self.state.lock().unwrap();
        let verified = (s.known.candidates as f64).min(s.known.images());
        (s.paces.ask_again(keypoints as f64, verified) * 1e9) as u64
    }

    /// Each stage that ran: its work in its unit, the seconds it took, and
    /// the nanoseconds of each thread a unit took, which is what
    /// `Stage::prior` is read from. Closes the running stage.
    pub fn paces(&self) -> Vec<String> {
        let mut s = self.state.lock().unwrap();
        s.close();
        let p = &s.paces;
        Stage::ALL
            .iter()
            .filter(|&&st| st.prior().is_some() && p.secs[st as usize] > 0.0)
            .map(|&st| (st.label(), st.unit(), p.secs[st as usize], p.units[st as usize]))
            .chain(p.analysis.map(|(secs, ns)| ("describing, past the decode", "pixels analysed", secs, ns / EXTRACT_PER_PIXEL)))
            .map(|(label, unit, secs, units)| {
                let per = if units > 0.0 { secs * p.threads * 1e9 / units } else { 0.0 };
                format!("pace: {label}: {units:.0} {unit} in {secs:.2}s, {per:.1} ns a unit on each of {} threads", p.threads)
            })
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageFormat;

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
    fn a_file_is_costed_at_the_size_the_analysis_works_at() {
        let probe = |w, h| Probe { kind: Kind::Image(ImageFormat::Jpeg), w, h };
        // Every size the decode reduces, and the enlargement after it, as
        // the decode and the extractor themselves size them.
        for (w, h) in [(4000, 3000), (3000, 4000), (1080, 11520), (11520, 1080), (224, 224), (100, 30), (512, 384), (513, 2), (7, 9)] {
            for work in [0, 128, 384, 512, 640] {
                let c = file_cost(Some(&probe(w, h)), 1, work, 512);
                let (ww, wh) = crate::decode::working_size(w as usize, h as usize, work);
                let f = crate::sift::enlargement(ww.max(wh), crate::analysis::enlarge_below(work, 512));
                assert_eq!(c.analysed, (ww * wh * f * f) as u64);
                assert_eq!(c.source, w as u64 * h as u64);
            }
        }
        // A panorama is analysed at a 4:3 picture's pixels, not its long
        // side's share of them (`decode::area_factor`).
        let pano = file_cost(Some(&probe(11520, 1080)), 1, 512, 512).analysed;
        let photo = file_cost(Some(&probe(4000, 3000)), 1, 512, 512).analysed;
        assert!((pano as f64 / photo as f64 - 1.0).abs() < 0.05, "{pano} {photo}");
        // A thumbnail is enlarged, but never past the working size.
        assert_eq!(file_cost(Some(&probe(224, 224)), 1, 512, 512).analysed, 448 * 448);
        assert_eq!(file_cost(Some(&probe(224, 224)), 1, 384, 512).analysed, 224 * 224);
    }

    #[test]
    fn every_stage_has_its_words_and_a_price() {
        for (i, s) in Stage::ALL.iter().enumerate() {
            assert_eq!(*s as usize, i, "{s:?} out of order");
            assert!(!s.label().is_empty() && !s.unit().is_empty());
            assert!(s.prior().is_some() || *s == Stage::Variants, "{s:?}");
        }
    }

    #[test]
    fn the_analysis_is_priced_by_what_its_own_files_took() {
        let jpeg = Some(Kind::Image(ImageFormat::Jpeg));
        let jxl = Some(Kind::Jxl);
        let c = |kind, source, analysed| FileCost { kind, source, analysed };
        let files = [c(jpeg, 1000, 100), c(jpeg, 1000, 100), c(jxl, 1000, 100), c(jxl, 1000, 100)];
        let d = Describing::new(&files);
        assert_eq!(d.left(1.0), None);
        // One JPEG: 1 ms decoding, 1 ms the rest.
        d.done(&files[0], Duration::from_millis(1), Duration::from_millis(2));
        // Left: a JPEG and two JPEG XLs, decoded at the JPEG's pace since no
        // JPEG XL is done, so 6 ms on one thread, and on two, half that.
        let left = d.left(1.0).unwrap();
        assert!((left - 0.006).abs() < 1e-9, "{left}");
        assert!((d.left(2.0).unwrap() - 0.003).abs() < 1e-9);
        // A JPEG XL done, ten times the JPEG's decode: the other is priced
        // at its own format's pace, and two files are left for eight threads.
        d.done(&files[2], Duration::from_millis(10), Duration::from_millis(11));
        let left = d.left(8.0).unwrap();
        assert!((left - (0.002 + 0.011) / 2.0).abs() < 1e-9, "{left}");
    }

    #[test]
    fn the_second_look_is_every_file_verification_has_not_settled() {
        let mut k = Known { files: 100, images: Some(100), descriptors: Some(50_000), ..Default::default() };
        assert_eq!(k.ask_again(), (100.0, 50_000.0));
        assert_eq!(k.units(Stage::Variants), 100.0);
        k.settled = (60, 30_000);
        assert_eq!(k.ask_again(), (40.0, 20_000.0));
        // Once the second look has counted its askers, that is what it asks.
        k.ask_again = Some((10, 4_000));
        assert_eq!(k.ask_again(), (10.0, 4_000.0));
        assert_eq!(k.units(Stage::Variants), 10.0);
    }

    #[test]
    fn a_stage_past_its_estimate_is_never_done() {
        let mut last = f64::INFINITY;
        for i in 0..1000 {
            let left = overrun(10.0, i as f64 * 0.05);
            assert!(left > 0.0 && left < last, "{i} {left}");
            last = left;
        }
        // Smooth where the two meet.
        let (a, b) = (overrun(10.0, 4.999), overrun(10.0, 5.001));
        assert!((a - b).abs() < 0.003, "{a} {b}");
    }

    #[test]
    fn the_bar_moves_with_the_time_left_and_never_back() {
        let mut s = State {
            known: Known { files: 100, work_size: 512, candidates: 150, ..Default::default() },
            paces: Paces { secs: [0.0; STAGES], units: [0.0; STAGES], analysis: None, threads: 8.0 },
            done: [false; STAGES],
            current: None,
            at: 0.0,
            last: Instant::now(),
        };
        let counter = Arc::new(Counter::default());
        s.current = Some(Current {
            stage: Stage::Verify,
            started: Instant::now() - Duration::from_secs(10),
            work: Work::Counted { counter: counter.clone(), weight: 100, items: 100, unit: "pairs", per: 0.1 },
            units: 100.0,
        });
        for s2 in &Stage::ALL[..Stage::Verify as usize] {
            s.done[*s2 as usize] = true;
        }
        // Half the pairs in ten seconds: ten seconds of verifying left, plus
        // what follows.
        for _ in 0..50 {
            counter.tick();
        }
        let left = s.left(Instant::now());
        assert!(left > 10.0 - 0.1, "{left}");
        let mut last = 0.0;
        for _ in 0..20 {
            s.last -= Duration::from_millis(500);
            let (at, _) = s.frame().unwrap();
            assert!(at >= last && at < 1.0, "{at} {last}");
            last = at;
        }
    }
}
