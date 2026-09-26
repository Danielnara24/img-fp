//! What the run skipped, what it could not do, and where both are written.
//!
//! A run's last line is its results, and on a large corpus that is the only
//! part still on screen — so an image that would not decode, a root that does
//! not exist, or a cache that would not write have to be collected and said at
//! the end rather than shouted into a scrollback nobody keeps. That is half of
//! what this is for. The other half is the exit code: a script sees nothing
//! else, and "found 4,581 pairs" reads exactly like success whether or not a
//! third of the corpus refused to open.
//!
//! **The split between a skip and a problem is load-bearing**, and it is the
//! one `vid-fp`'s `stats.rs` draws: a skip is something the tool was always
//! going to pass over — a file whose extension is not an image format, a
//! symlink it does not follow — and a problem is something the run was asked
//! for and did not get. Only the second kind moves the exit code to
//! [`crate::EXIT_WITH_PROBLEMS`], which is what keeps that code worth testing:
//! pointed at a home directory, img-fp passes over a quarter of a million
//! files and has nothing to apologise for.
//!
//! Nothing is printed as it happens. A per-file line costs nothing until the
//! day it is a quarter of a million of them pushing the results off the
//! screen, and the summary says the same thing with a count in front of it.
//! `--log-file` is where the unabridged list lives: every line this module
//! records goes there in full, as it is recorded, whatever the console shows.
//!
//! The counters are plain, not atomic, and do not need to be: the walk is
//! single-threaded, the cache is touched once before and once after the
//! analysis, and a decode failure is recorded from `items` *after* every
//! worker has joined — the parallel phase writes its errors into its own
//! `Item` and this reads them afterwards. The `Log` is the one part that is
//! shared while the workers run, and it carries its own mutex for it. If a
//! counter is ever wanted from inside a `par_iter`, that is the assumption to
//! revisit.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// Worked examples kept per category, for the console.
///
/// The count is never capped and neither is the log file; this is how many of
/// the paths behind it the summary names. Ten keeps the whole summary inside a
/// screen even when every category has fired, which is what makes it readable
/// at the end of a long run — and a corpus where more than ten files failed is
/// one where the count is the finding and the eleventh path is not.
const MAX_SAMPLES: usize = 10;

/// Every line the run had to say, uncapped, in a file.
///
/// Truncating rather than appending: the file describes *this* run, and the
/// alternative is a file that quietly grows by a quarter of a million lines a
/// scan. Opened before any work starts, so that a path that cannot be written
/// is an ordinary fatal error at the top of the run rather than a discovery
/// made an hour into one.
///
/// Deliberately unbuffered — one `write` per line. There is nowhere honest to
/// flush a buffer: the interesting runs are the ones that end badly, and a
/// `BufWriter` would swallow exactly the tail that says why. The cost is a
/// debugging flag's alone, and small: a few microseconds a line, so even a
/// walk that skips a quarter of a million files pays well under a second.
#[derive(Default)]
pub struct Log(Option<Mutex<std::fs::File>>);

impl Log {
    pub fn open(path: Option<&Path>) -> Result<Log> {
        let Some(path) = path else { return Ok(Log(None)) };
        let mut f = std::fs::File::create(path)
            .with_context(|| format!("could not create the log file {}", path.display()))?;
        let argv: Vec<String> = std::env::args().collect();
        let _ = writeln!(f, "# img-fp {}\n# {}", env!("CARGO_PKG_VERSION"), argv.join(" "));
        Ok(Log(Some(Mutex::new(f))))
    }

    /// True when there is a file to write to, so a caller can decide whether a
    /// line is worth formatting at all.
    pub fn active(&self) -> bool {
        self.0.is_some()
    }

    /// A write that fails is dropped. The log is a record of the run, not part
    /// of it, and failing the run over its diary — after the results are
    /// already on screen — would be the tail wagging the dog.
    pub fn line(&self, s: &str) {
        if let Some(f) = &self.0 {
            let mut f = f.lock().unwrap_or_else(|e| e.into_inner());
            let _ = writeln!(f, "{s}");
        }
    }
}

/// A counter with a few examples attached.
#[derive(Default)]
struct Tally {
    count: usize,
    samples: Vec<String>,
}

impl Tally {
    fn record(&mut self, what: String) {
        self.count += 1;
        if self.samples.len() < MAX_SAMPLES {
            self.samples.push(what);
        }
    }
}

/// Count it, keep it if there is room for it, and write it to the log
/// whatever happens. A free function rather than a method so that one field of
/// `Problems` can be written while another is read.
fn record(log: &Log, tally: &mut Tally, label: &str, what: String) {
    if log.active() {
        log.line(&format!("{label}: {what}"));
    }
    tally.record(what);
}

/// Everything the run skipped or could not do, in the order it is reported.
pub struct Problems<'a> {
    log: &'a Log,

    // Skips: the tool passing over what it was never going to read.
    not_an_image: Tally,
    not_image_content: Tally,
    symlink: Tally,
    symlink_loop: Tally,
    listed_twice: Tally,
    excluded: Tally,

    // Problems: what was asked for and did not happen.
    unresolved_exclude: Tally,
    unscannable: Tally,
    unreadable: Tally,
    featureless: Tally,
    cache: Tally,
}

impl<'a> Problems<'a> {
    pub fn new(log: &'a Log) -> Self {
        Problems {
            log,
            not_an_image: Tally::default(),
            not_image_content: Tally::default(),
            symlink: Tally::default(),
            symlink_loop: Tally::default(),
            listed_twice: Tally::default(),
            excluded: Tally::default(),
            unresolved_exclude: Tally::default(),
            unscannable: Tally::default(),
            unreadable: Tally::default(),
            featureless: Tally::default(),
            cache: Tally::default(),
        }
    }


    // ---- skips

    /// A file the walk's extension list turned away — by default, one whose
    /// extension names a format img-fp does not read, or that has none. The
    /// filter is why a scan of a home directory is not an attempt to decode
    /// it, and it is also the one thing that can hide a photograph: a JPEG
    /// saved as `.txt`, or with no extension, is passed over here and never
    /// sniffed unless `-x '*'` asks for it. Hence the count.
    pub fn not_an_image(&mut self, path: &str) {
        record(self.log, &mut self.not_an_image, "skip/not-an-image", path.into());
    }

    /// A symlink met during a walk that does not follow them. A folder that
    /// is *all* symlinks scans as empty without `--follow-symlinks`, which is
    /// worth saying rather than leaving to be deduced.
    pub fn symlink(&mut self, path: &str) {
        record(self.log, &mut self.symlink, "skip/symlink", path.into());
    }

    /// A followed link leading back into a folder the walk is already inside.
    /// Everything behind it is being walked by the other route, so nothing is
    /// missing and this is not a problem.
    pub fn symlink_loop(&mut self, path: &str) {
        record(self.log, &mut self.symlink_loop, "skip/symlink-loop", path.into());
    }

    /// A root named on the command line that `--exclude` covers. Only named
    /// roots are counted: an excluded subtree met during a walk is pruned
    /// whole, and a count of it would depend on which route reached it.
    pub fn excluded(&mut self, path: &str) {
        record(self.log, &mut self.excluded, "skip/excluded", path.into());
    }

    /// A file a wildcard walk (`-x '*'`, `-x '!gif'`) handed over whose bytes
    /// are no picture format at all. It never claimed to be an image, so
    /// nothing was asked of it and nothing failed — which is what separates it
    /// from a `.jpg` that will not decode, still a problem.
    pub fn not_image_content(&mut self, path: &str) {
        record(self.log, &mut self.not_image_content, "skip/not-image-content", path.into());
    }

    /// A second name for a file already listed: named twice, reached by two
    /// overlapping roots, a symlink to it, a hard link. It is analysed once;
    /// the count is what explains a file total smaller than the names found.
    pub fn listed_twice(&mut self, path: &str) {
        record(self.log, &mut self.listed_twice, "skip/listed-twice", path.into());
    }

    // ---- problems

    /// An `--exclude` path that does not resolve, and so excludes nothing. A
    /// typo in the one flag whose job is "leave this alone" is not a detail.
    pub fn unresolved_exclude(&mut self, path: &str, err: &dyn std::fmt::Display) {
        record(self.log, &mut self.unresolved_exclude, "problem/unresolved-exclude", format!("{path}: {err}"));
    }

    /// A directory — or a root that is not there at all — the walk could not
    /// read. Its contents are absent from the run entirely, which is the
    /// difference between this and an image that failed: there is no telling
    /// how many files it should have contributed.
    pub fn unscannable(&mut self, path: &str, err: &dyn std::fmt::Display) {
        record(self.log, &mut self.unscannable, "problem/unscannable", format!("{path}: {err}"));
    }

    /// An image that would not decode. It is in `files_enumerated` and not in
    /// `files_analysed`, and it is listed in the JSON output's `failures` with
    /// the same message.
    pub fn unreadable(&mut self, path: &str, err: &str) {
        record(self.log, &mut self.unreadable, "problem/unreadable", format!("{path}: {err}"));
    }

    /// An image that decoded and described to nothing: no local features at
    /// all, which happens to a blank or near-blank picture. Nothing failed,
    /// and that is exactly why it is worth a line — the file is in the corpus,
    /// it is in `files_analysed`, and it cannot match anything but a
    /// byte-identical copy of itself, because there is nothing for the
    /// geometry to agree about. Declining to answer is not the same as having
    /// answered.
    pub fn featureless(&mut self, path: &str) {
        record(self.log, &mut self.featureless, "problem/featureless", path.into());
    }

    /// The `--cache` file could not be read or could not be written. Nothing
    /// about the result changes — the cache is an optimisation and a run
    /// without it computes the same pairs — but the user asked for it, the
    /// next run will be just as slow, and the exit code is the only place that
    /// can be said to a script.
    pub fn cache(&mut self, what: String) {
        record(self.log, &mut self.cache, "problem/cache", what);
    }

    // ---- reporting

    fn skips(&self) -> [(&Tally, &'static str); 6] {
        [
            (&self.not_an_image, "file(s) whose extension is not searched (see -x)"),
            (&self.not_image_content, "file(s) that are not images (reached by a wildcard -x)"),
            (&self.symlink, "symlink(s), which are not followed (see --follow-symlinks)"),
            (&self.symlink_loop, "symlink(s) leading back into a folder already being walked"),
            (&self.listed_twice, "path(s) already listed under another name (named twice, overlapping roots, a symlink or a hard link)"),
            (&self.excluded, "named path(s) skipped because --exclude covers them"),
        ]
    }

    fn problems(&self) -> [(&Tally, &'static str); 5] {
        [
            (&self.unresolved_exclude, "--exclude path(s) could not be resolved; nothing was excluded for them"),
            (&self.unscannable, "path(s) could not be scanned"),
            (&self.unreadable, "image(s) could not be read"),
            // Not "failed": the analysis ran and came back empty-handed. The
            // label says what it costs the user rather than what the code did,
            // because the count is only worth printing for that consequence.
            (&self.featureless, "image(s) have no features and can only match a byte-identical copy"),
            (&self.cache, "cache problem(s)"),
        ]
    }

    pub fn any(&self) -> bool {
        self.count() > 0
    }

    /// Whether the walk saw everything it was pointed at.
    ///
    /// Only `--prune-cache` asks: pruning against a scan that could not read
    /// one of its roots would throw away good records for files that are
    /// still there, and the cache is the one thing a run keeps. Every other
    /// problem leaves the walk's own account complete — an image that would
    /// not decode was still *found*, and its record is dropped for the reason
    /// `carry_over` gives rather than for this one.
    pub fn walk_was_complete(&self) -> bool {
        self.unscannable.count == 0
    }

    /// Problems only. A skip is not a failure and must not move the exit code.
    pub fn count(&self) -> usize {
        self.problems().iter().map(|(t, _)| t.count).sum()
    }

    /// Printed last, after the results, because it is the part to act on.
    /// Silent when there is nothing to say: a clean run says so by printing
    /// nothing here and exiting 0.
    pub fn print_summary(&self) {
        let skips: Vec<_> = self.skips().into_iter().filter(|(t, _)| t.count > 0).collect();
        if !skips.is_empty() {
            self.say("\nSkipped:");
            for (tally, label) in skips {
                self.render(tally, label);
            }
        }
        let problems: Vec<_> = self.problems().into_iter().filter(|(t, _)| t.count > 0).collect();
        if !problems.is_empty() {
            self.say(&format!("\nProblems ({} total):", self.count()));
            for (tally, label) in problems {
                self.render(tally, label);
            }
        }
    }

    fn render(&self, tally: &Tally, label: &str) {
        self.say(&format!("  {:>5}  {}", tally.count, label));
        for s in tally.samples.iter() {
            self.say(&format!("         - {s}"));
        }
        // Only ever elided because the console has a budget; the log file
        // holds every one of them, which is what the line points at.
        let hidden = tally.count - tally.samples.len();
        if hidden > 0 {
            let more = match self.log.active() {
                true => format!("         - ... and {hidden} more (all of them are in the log file)"),
                false => format!("         - ... and {hidden} more (--log-file lists them all)"),
            };
            self.say(&more);
        }
    }

    /// The summary goes to both. A log file that holds every failure and not
    /// the conclusion drawn from them is the wrong half.
    fn say(&self, line: &str) {
        eprintln!("{line}");
        self.log.line(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(log: &Log) -> Problems<'_> {
        Problems::new(log)
    }

    #[test]
    fn samples_are_capped_but_the_count_is_not() {
        let log = Log::default();
        let mut p = p(&log);
        for i in 0..50 {
            p.unreadable(&format!("/x/{i}.jpg"), "not an image");
        }
        assert_eq!(p.count(), 50, "every failure is counted");
        assert_eq!(p.unreadable.samples.len(), MAX_SAMPLES, "only a few are named");
        assert!(p.any());
    }

    #[test]
    fn a_clean_run_has_nothing_to_report() {
        let log = Log::default();
        let p = p(&log);
        assert!(!p.any(), "and so exits 0");
        assert_eq!(p.count(), 0);
    }

    #[test]
    fn a_skip_is_not_a_problem_and_does_not_move_the_exit_code() {
        let log = Log::default();
        let mut p = p(&log);
        p.not_an_image("/notes.txt");
        p.not_image_content("/README");
        p.symlink("/link.jpg");
        p.symlink_loop("/a/up");
        p.listed_twice("/a.jpg");
        p.excluded("/keep");
        assert_eq!(p.count(), 0, "skips are not failures");
        assert!(!p.any(), "so a scan of a home directory still exits 0");
    }

    #[test]
    fn every_problem_category_counts_towards_the_exit_code() {
        let log = Log::default();
        let mut p = p(&log);
        p.unscannable("/nope", &"No such file or directory");
        p.unreadable("/a.jpg", "unsupported");
        p.featureless("/blank.png");
        p.cache("could not write /tmp/c".into());
        p.unresolved_exclude("/kepe", &"No such file or directory");
        assert_eq!(p.count(), 5);
    }

    #[test]
    fn the_log_file_holds_every_line_the_console_elides() {
        let dir = std::env::temp_dir().join(format!("img-fp-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("run.log");
        {
            let log = Log::open(Some(&path)).unwrap();
            let mut p = p(&log);
            for i in 0..50 {
                p.unreadable(&format!("/x/{i}.jpg"), "not an image");
            }
            p.print_summary();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# img-fp"), "the log names the run that wrote it");
        for i in 0..50 {
            assert!(text.contains(&format!("/x/{i}.jpg")), "every failure, not the first ten");
        }
        assert!(text.contains("Problems (50 total):"), "and the conclusion drawn from them");
        std::fs::remove_dir_all(&dir).ok();
    }
}
