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

use anyhow::Result;
use std::io::Write;
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
///
/// Where it goes is `outputs::To::open_log`'s.
#[derive(Default)]
pub struct Log(Option<Mutex<Box<dyn Write + Send>>>);

impl Log {
    pub fn open(to: Option<&crate::outputs::To>) -> Result<Log> {
        let Some(to) = to else { return Ok(Log(None)) };
        let mut f = to.open_log()?;
        // `args_os`, not `args`: the latter panics on an argument that is not
        // UTF-8, and a folder with such a name is an ordinary thing to scan.
        let argv: Vec<String> = std::env::args_os().map(|a| a.to_string_lossy().into_owned()).collect();
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

/// Something the run passes over or could not do: one category of the
/// summary. Its words (the log's tag, the summary's label) and whether it is
/// a problem are [`Note::says`], the one place a category is described; the
/// summary lists them in the order of [`Note::ALL`].
///
/// **Adding a category**: a variant here, an arm in `says`, a place in
/// `ALL`. Not a counter of its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Note {
    // Skips: the tool passing over what it was never going to read.
    /// A file the walk's extension list turned away — by default, one whose
    /// extension names a format img-fp does not read, or that has none. The
    /// filter is why a scan of a home directory is not an attempt to decode
    /// it, and it is also the one thing that can hide a photograph: a JPEG
    /// saved as `.txt`, or with no extension, is passed over here and never
    /// sniffed unless `-x '*'` asks for it. Hence the count.
    NotAnImage,
    /// A Trash or a thumbnail cache met during a walk, and not gone into:
    /// what is inside is copies of pictures that are elsewhere, or were
    /// thrown away. See `SetAside` in `walk.rs`.
    SetAside,
    /// A hidden folder met during a walk without `--hidden`, and not gone
    /// into. See `is_hidden` in `walk.rs`.
    Hidden,
    /// A folder inside a folder scanned without `--recursive`, and not gone
    /// into. A photo library is folders of folders, and pointed at one, a
    /// scan said "No images found." and exited 0, with nothing to say what
    /// it had passed over or that `-r` was what it needed.
    Subfolder,
    /// A file a wildcard walk (`-x '*'`, `-x '!gif'`) handed over whose bytes
    /// are no picture format at all. It never claimed to be an image, so
    /// nothing was asked of it and nothing failed — which is what separates it
    /// from a `.jpg` that will not decode, still a problem.
    NotImageContent,
    /// A symlink met during a walk that does not follow them. A folder that
    /// is *all* symlinks scans as empty without `--follow-symlinks`, which is
    /// worth saying rather than leaving to be deduced.
    Symlink,
    /// A followed link leading back into a folder the walk is already inside.
    /// Everything behind it is being walked by the other route, so nothing is
    /// missing and this is not a problem.
    SymlinkLoop,
    /// A second name for a file already listed: named twice, reached by two
    /// overlapping roots, a symlink to it, a hard link. It is analysed once;
    /// the count is what explains a file total smaller than the names found.
    ListedTwice,
    /// A root named on the command line that `--exclude` covers. Only named
    /// roots are counted: an excluded subtree met during a walk is pruned
    /// whole, and a count of it would depend on which route reached it.
    Excluded,
    /// A macOS metadata file (AppleDouble): the `._` twin a Mac writes
    /// beside every file it copies onto a drive or a share that cannot hold
    /// its extended attributes, and puts in `__MACOSX` in a zip. It carries
    /// the picture's name, `._IMG_1234.JPG`, and no picture. Read as one, every
    /// such file was an image that would not decode, exit 2, and two of them
    /// alike — which they often are — a byte-identical *image* with one of
    /// them suggested for deletion.
    MacMetadata,
    /// A camera's raw file in a TIFF container (`decode::Refused::CameraRaw`). Its
    /// first picture is the camera's preview, a few hundred pixels across,
    /// and read as the photograph it was judged at that size and suggested
    /// for deletion beside the camera's own JPEG of it. Not read, so a skip:
    /// nothing failed. A byte-identical copy of one is still found, as the
    /// exact pass reads bytes, not pictures.
    CameraRaw,
    /// An image that decoded and described to nothing: no local features at
    /// all, which happens to a blank or near-blank picture — a solid colour,
    /// a smooth gradient. It can match nothing but a byte-identical copy of
    /// itself, because there is nothing for the geometry to agree about, and
    /// that is worth a line; but nothing failed, so it is a skip and does not
    /// move the exit code. It used to be a problem, and a folder holding one
    /// plain backdrop exited 2 on every run, which a script reads as a scan
    /// that went wrong.
    Featureless,

    // Problems: what was asked for and did not happen.
    /// An `--exclude` path that does not resolve, and so excludes nothing. A
    /// typo in the one flag whose job is "leave this alone" is not a detail.
    UnresolvedExclude,
    /// A directory — or a root that is not there at all — the walk could not
    /// read. Its contents are absent from the run entirely, which is the
    /// difference between this and an image that failed: there is no telling
    /// how many files it should have contributed.
    Unscannable,
    /// A `--against` folder or file that put no image in the library: an
    /// empty folder, one whose pictures are all in subfolders without
    /// `--recursive`, or one every file of which the scan holds. Every file
    /// scanned against it is `new`, a copy of one of its pictures too, and
    /// the run used to say only "against 0 library images" and exit 0.
    EmptyLibrary,
    /// An image that would not decode. It is in `files_enumerated` and not in
    /// `files_analysed`, and it is listed in the JSON output's `failures` with
    /// the same message.
    Unreadable,
    /// An image that decoded only in part: a JPEG cut off in its image data,
    /// which the decoder fills out with grey rather than refuse.
    /// It is analysed — what is there may still be worth matching — and its
    /// rows in the report say `damaged`, because a damaged file is the one
    /// copy not to keep, and nothing about its pairs says so. Every other
    /// format refuses such a file outright, which is `Unreadable`.
    Damaged,
    /// The `--cache` file could not be read or could not be written. Nothing
    /// about the result changes — the cache is an optimisation and a run
    /// without it computes the same pairs — but the user asked for it, the
    /// next run will be just as slow, and the exit code is the only place that
    /// can be said to a script.
    Cache,
}

/// Whether a category moves the exit code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Weight {
    Skip,
    Problem,
}

impl Note {
    /// Every category, in the order the summary lists them.
    const ALL: [Note; 18] = {
        use Note::*;
        [
            NotAnImage, SetAside, Hidden, Subfolder, NotImageContent, Symlink, SymlinkLoop, ListedTwice, Excluded,
            MacMetadata, CameraRaw, Featureless, UnresolvedExclude, Unscannable, EmptyLibrary, Unreadable, Damaged, Cache,
        ]
    };

    /// What a category is: a skip or a problem, its tag in the log, and its
    /// label in the summary.
    fn says(self) -> (Weight, &'static str, &'static str) {
        use Weight::*;
        match self {
            Note::NotAnImage => (Skip, "skip/not-an-image", "file(s) whose extension is not searched, or with none (see -x)"),
            Note::SetAside => (Skip, "skip/trash-or-thumbnails", "Trash, thumbnail or snapshot folder(s), not gone into (name one to scan it)"),
            Note::Hidden => (Skip, "skip/hidden-folder", "hidden folder(s), not gone into (see --hidden)"),
            Note::Subfolder => (Skip, "skip/subfolder", "subfolder(s), not gone into (see --recursive)"),
            Note::NotImageContent => (Skip, "skip/not-image-content", "file(s) that are not images (reached by a wildcard -x)"),
            Note::Symlink => (Skip, "skip/symlink", "symlink(s), which are not followed (see --follow-symlinks)"),
            Note::SymlinkLoop => (Skip, "skip/symlink-loop", "symlink(s) leading back into a folder already being walked"),
            Note::ListedTwice => (Skip, "skip/listed-twice", "path(s) already listed under another name (named twice, overlapping roots, a symlink or a hard link)"),
            Note::Excluded => (Skip, "skip/excluded", "named path(s) skipped because --exclude covers them"),
            Note::MacMetadata => (Skip, "skip/mac-metadata", "macOS metadata file(s) (\"._\" files), which hold no picture"),
            Note::CameraRaw => (Skip, "skip/camera-raw", "camera raw file(s), which img-fp does not read (only a byte-identical copy is found)"),
            // The label says what it costs rather than what the code did.
            Note::Featureless => (Skip, "skip/featureless", "image(s) with no detail to match, such as a blank picture: only an identical copy can match one"),
            Note::UnresolvedExclude => (Problem, "problem/unresolved-exclude", "--exclude path(s) could not be resolved; nothing was excluded for them"),
            Note::Unscannable => (Problem, "problem/unscannable", "path(s) could not be scanned"),
            Note::EmptyLibrary => (Problem, "problem/empty-library", "--against path(s) put no image in the library (a library of subfolders needs --recursive)"),
            Note::Unreadable => (Problem, "problem/unreadable", "image(s) could not be read"),
            Note::Damaged => (Problem, "problem/damaged", "image(s) are cut off and were analysed as far as they could be read"),
            Note::Cache => (Problem, "problem/cache", "cache problem(s)"),
        }
    }

    fn weight(self) -> Weight {
        self.says().0
    }
}

/// Everything the run skipped or could not do: a tally per [`Note`].
pub struct Problems<'a> {
    log: &'a Log,
    tallies: [Tally; Note::ALL.len()],
}

impl<'a> Problems<'a> {
    pub fn new(log: &'a Log) -> Self {
        Problems { log, tallies: Default::default() }
    }

    /// Count it, keep it if there is room for it, and write it to the log
    /// whatever happens. `what` is the path, or for a problem the path and
    /// why (`"{path}: {err}"`).
    pub fn note(&mut self, n: Note, what: impl std::fmt::Display) {
        let what = what.to_string();
        if self.log.active() {
            self.log.line(&format!("{}: {what}", n.says().1));
        }
        self.tallies[n as usize].record(what);
    }

    fn tally(&self, n: Note) -> &Tally {
        &self.tallies[n as usize]
    }

    /// The categories of one weight that fired, in the summary's order.
    fn fired(&self, w: Weight) -> impl Iterator<Item = (&Tally, &'static str)> {
        Note::ALL.into_iter().filter(move |n| n.weight() == w).map(|n| (self.tally(n), n.says().2)).filter(|(t, _)| t.count > 0)
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
    /// not decode was still *found*, and its record is dropped because the
    /// run neither took nor made one for it (`cache::Store::finish`), not for
    /// this reason.
    pub fn walk_was_complete(&self) -> bool {
        self.tally(Note::Unscannable).count == 0
    }

    /// Problems only. A skip is not a failure and must not move the exit code.
    pub fn count(&self) -> usize {
        self.fired(Weight::Problem).map(|(t, _)| t.count).sum()
    }

    /// Printed last, after the results, because it is the part to act on.
    /// Silent when there is nothing to say: a clean run says so by printing
    /// nothing here and exiting 0.
    pub fn print_summary(&self) {
        for line in self.summary() {
            self.say(&line);
        }
    }

    /// The lines `print_summary` prints, which a kept scan also keeps as its
    /// log (`last::Notes::log`).
    pub fn summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let skips: Vec<_> = self.fired(Weight::Skip).collect();
        if !skips.is_empty() {
            lines.push("\nSkipped:".to_string());
            for (tally, label) in skips {
                self.render(tally, label, &mut lines);
            }
        }
        let problems: Vec<_> = self.fired(Weight::Problem).collect();
        if !problems.is_empty() {
            lines.push(format!("\nProblems ({} total):", self.count()));
            for (tally, label) in problems {
                self.render(tally, label, &mut lines);
            }
        }
        lines
    }

    fn render(&self, tally: &Tally, label: &str, lines: &mut Vec<String>) {
        lines.push(format!("  {:>5}  {}", tally.count, label));
        for s in tally.samples.iter() {
            lines.push(format!("         - {s}"));
        }
        // Only ever elided because the console has a budget; the log file
        // holds every one of them, which is what the line points at.
        let hidden = tally.count - tally.samples.len();
        if hidden > 0 {
            lines.push(match self.log.active() {
                true => format!("         - ... and {hidden} more (all of them are in the log file)"),
                false => format!("         - ... and {hidden} more (--log-file lists them all)"),
            });
        }
    }

    /// The summary goes to both. A log file that holds every failure and not
    /// the conclusion drawn from them is the wrong half.
    fn say(&self, line: &str) {
        crate::progress::to_stderr(line);
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
            p.note(Note::Unreadable, format_args!("/x/{i}.jpg: not an image"));
        }
        assert_eq!(p.count(), 50, "every failure is counted");
        assert_eq!(p.tally(Note::Unreadable).samples.len(), MAX_SAMPLES, "only a few are named");
        assert!(p.any());
    }

    #[test]
    fn every_category_has_its_tally() {
        // `tallies` is indexed by the variant's number, and `ALL` is what
        // the summary walks: each variant once, in declaration order.
        for (i, n) in Note::ALL.into_iter().enumerate() {
            assert_eq!(n as usize, i, "{n:?} is out of place in ALL");
        }
        assert_eq!(Note::ALL.len(), Note::Cache as usize + 1, "a variant is missing from ALL");
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
        p.note(Note::NotAnImage, "/notes.txt");
        p.note(Note::NotImageContent, "/README");
        p.note(Note::Symlink, "/link.jpg");
        p.note(Note::SymlinkLoop, "/a/up");
        p.note(Note::ListedTwice, "/a.jpg");
        p.note(Note::Excluded, "/keep");
        p.note(Note::MacMetadata, "/._a.jpg");
        p.note(Note::Featureless, "/blank.png");
        p.note(Note::SetAside, "/.Trash-1000");
        p.note(Note::Hidden, "/.git");
        p.note(Note::CameraRaw, "/a.dng");
        assert_eq!(p.count(), 0, "skips are not failures");
        assert!(!p.any(), "so a scan of a home directory still exits 0");
    }

    #[test]
    fn every_problem_category_counts_towards_the_exit_code() {
        let log = Log::default();
        let mut p = p(&log);
        p.note(Note::Unscannable, "/nope: No such file or directory");
        p.note(Note::Unreadable, "/a.jpg: unsupported");
        p.note(Note::Cache, "could not write /tmp/c");
        p.note(Note::UnresolvedExclude, "/kepe: No such file or directory");
        p.note(Note::Damaged, "/cut.jpg: cut off");
        assert_eq!(p.count(), 5);
    }

    #[test]
    fn the_log_file_holds_every_line_the_console_elides() {
        let dir = std::env::temp_dir().join(format!("img-fp-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("run.log");
        {
            let log = Log::open(Some(&crate::outputs::To::Path(path.clone()))).unwrap();
            let mut p = p(&log);
            for i in 0..50 {
                p.note(Note::Unreadable, format_args!("/x/{i}.jpg: not an image"));
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
