//! Where a run's output goes: the report, `--dump`, `--log-file` and
//! `--list`, decided once, before anything is written.
//!
//! **Every rule about the outputs is `plan`'s.** Who has stdout, that no
//! output is a picture, that no two write one file (the cache and the kept
//! scan included) and that each can be written: before, these were four
//! checks in `lib.rs`, each building its own list of the flags and dropping
//! `-` itself, and whether the report went to stdout was worked out again by
//! the pipeline and by `--last`, three ways; the window's own check ran only
//! some of them. What `plan` hands back is where each output goes, and the
//! writers only read it.
//!
//! **Every output is written one way** (`To::write`, `To::open_log`): stdout
//! through a buffer, where a reader going away is how a pipeline ends; a file
//! whole or not at all; a descriptor, a pipe or a device as it was opened.
//! Before, each output wrote stdout its own way (`--dump - | head` exited 1
//! on the broken pipe the report ended on quietly), and the log opened its
//! file with `File::create`, so `--log-file /dev/stdout >> run.log` emptied
//! `run.log`.
//!
//! **Adding an output**: a field of `Outputs`, set in `plan` with its entry
//! in `claims`; written through `To`. Not a check of its own in `lib.rs`.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::report::Format;
use crate::Args;

/// Where one output goes. `-` is stdout; a file really named `-` is `./-`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum To {
    Stdout,
    Path(PathBuf),
}

impl To {
    fn of(p: &Path) -> To {
        if p == Path::new("-") {
            To::Stdout
        } else {
            To::Path(p.to_path_buf())
        }
    }

    /// The file it names, if it is not stdout.
    pub fn file(&self) -> Option<&Path> {
        match self {
            To::Path(p) => Some(p),
            To::Stdout => None,
        }
    }

    /// Where it goes, as the console says it.
    pub fn name(&self) -> String {
        match self {
            To::Path(p) => p.display().to_string(),
            To::Stdout => "stdout".into(),
        }
    }

    /// Write `what` whole: a file is written beside itself and renamed into
    /// place (`write_whole`); stdout through a buffer.
    ///
    /// A reader that goes away early (`img-fp DIR | head`) is how a pipeline
    /// ends, not a failed run, so a broken pipe on stdout is not an error.
    /// Any other failure is fatal, and says which output it was: this is the
    /// run's output, and there is nowhere else it went.
    pub fn write(&self, what: &str, body: impl FnOnce(&mut dyn Write) -> std::io::Result<()>) -> Result<()> {
        match self {
            To::Path(p) => write_whole(p, body),
            To::Stdout => {
                let mut w = std::io::BufWriter::new(std::io::stdout().lock());
                match body(&mut w).and_then(|()| w.flush()) {
                    Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                    other => other.with_context(|| format!("could not write {what} to stdout")),
                }
            }
        }
    }

    /// Opened for the log, which is written a line at a time through the run
    /// rather than whole at its end: a regular file is truncated, since it
    /// describes this run; a descriptor, a pipe or a device is written as it
    /// was opened (`Dest`), as the report is, so `>>` appends.
    pub fn open_log(&self) -> Result<Box<dyn Write + Send>> {
        let Some(path) = self.file() else { return Ok(Box::new(std::io::stdout())) };
        let ctx = || format!("could not create the log file {}", path.display());
        Ok(match destination(path) {
            Dest::Fd(fd) => Box::new(std::fs::File::from(borrowed(fd).with_context(ctx)?)),
            Dest::Special => Box::new(std::fs::OpenOptions::new().write(true).open(path).with_context(ctx)?),
            Dest::File(_) => Box::new(std::fs::File::create(path).with_context(ctx)?),
        })
    }
}

/// The report's destination and layout.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub to: To,
    pub format: Format,
}

impl Report {
    /// To `to`, in `format`, or in the one its name implies.
    pub fn new(to: To, format: Option<Format>) -> Report {
        let format = format.unwrap_or_else(|| to.file().map_or(Format::Txt, from_extension));
        Report { to, format }
    }
}

/// Where each of a run's outputs goes, or `None` for one it does not write.
#[derive(Debug)]
pub struct Outputs {
    pub report: Option<Report>,
    pub dump: Option<To>,
    pub log: Option<To>,
    /// `--list`'s paths, which always go to stdout.
    pub list: bool,
}

/// Where `args` sends each output, or why it cannot: the one place every rule
/// about them is. `window` is a scan the window runs, whose stdout is its
/// progress channel and whose report goes only to a file `-o` names.
///
/// Nothing is written but the probe of a file that is not there yet, which is
/// removed again.
pub fn plan(args: &Args, window: bool) -> Result<Outputs> {
    if window && !args.against.is_empty() {
        anyhow::bail!("--against cannot be used in the window");
    }
    if window && args.last {
        anyhow::bail!("--last cannot be used in the window");
    }
    let list = !args.list.is_empty();
    // The report goes to stdout unless `-o` names a file, or `--list` or the
    // window's progress has stdout; then only where `-o` names it.
    let to = match args.output.as_deref().map(To::of) {
        None if list || window => None,
        None => Some(To::Stdout),
        to => to,
    };
    let out = Outputs {
        report: to.map(|to| Report::new(to, args.format)),
        dump: args.dump.as_deref().map(To::of),
        log: args.log_file.as_deref().map(To::of),
        list,
    };
    let claims = out.claims();
    stdout_has_one_reader(&claims, window)?;
    let named: Vec<(&str, &Path)> = claims.iter().filter_map(|c| Some((c.flag?, c.to?.file()?))).collect();
    // A picture is what this tool is asked to look after, so it never writes
    // over one: `img-fp photos -o photos/a.jpg` wrote the report over
    // `a.jpg`, exit 0, and the report named `a.jpg` as its group's
    // representative. A glob gets there (`img-fp -o *.jpg` is `-o` the first
    // JPEG and a scan of the rest), and the log is the worst, truncated
    // before anything else has run.
    for (flag, p) in &named {
        if crate::decode::is_a_picture(p) {
            anyhow::bail!("{flag} {} is a picture, and img-fp will not write over one; name another file", p.display());
        }
    }
    outputs_are_distinct(args, &named)?;
    // The two files written at the end would otherwise fail after the whole
    // run: a directory named where a file was meant is the usual way. The
    // log is opened at once, which is its check.
    for p in [out.report.as_ref().map(|r| &r.to), out.dump.as_ref()].into_iter().flatten().filter_map(To::file) {
        check_writable(p)?;
    }
    Ok(out)
}

/// One output as the rules read it: its flag (none for `--list`, which names
/// no file), what it is called on stdout, and where it goes.
struct Claim<'a> {
    flag: Option<&'static str>,
    on_stdout: &'static str,
    to: Option<&'a To>,
}

impl Outputs {
    /// Every output, in the order the messages name them.
    fn claims(&self) -> [Claim<'_>; 4] {
        [
            Claim { flag: Some("-o"), on_stdout: "the report", to: self.report.as_ref().map(|r| &r.to) },
            Claim { flag: None, on_stdout: "--list", to: self.list.then_some(&To::Stdout) },
            Claim { flag: Some("--dump"), on_stdout: "--dump -", to: self.dump.as_ref() },
            Claim { flag: Some("--log-file"), on_stdout: "--log-file -", to: self.log.as_ref() },
        ]
    }
}

/// At most one output on stdout, and under the window none.
///
/// Any two would be one stream of interleaved CSV, log lines and report that
/// nothing can read back; the window reads its stdout as the scan's progress.
fn stdout_has_one_reader(claims: &[Claim], window: bool) -> Result<()> {
    let on: Vec<&str> = claims.iter().filter(|c| c.to == Some(&To::Stdout)).map(|c| c.on_stdout).collect();
    if window && !on.is_empty() {
        anyhow::bail!("{} cannot be used here: stdout carries the scan's progress", on.join(" and "));
    }
    if on.len() > 1 {
        let fix = if on[0] == "the report" { "; send the report to a file with -o" } else { "" };
        anyhow::bail!("{} would both be written to stdout{fix}", on.join(" and "));
    }
    Ok(())
}

/// Refuse two of the outputs, the cache and the kept scan naming one file.
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
/// default cache is out of sight, so it is the easiest to name by mistake.
/// The kept scan beside it is another: the report written over it was the
/// next `--last`'s "could not read the kept scan".
fn outputs_are_distinct(args: &Args, named: &[(&str, &Path)]) -> Result<()> {
    let cache = (!args.no_cache).then(|| crate::cache::locate(args.cache.as_deref()));
    let kept = (!args.no_cache && !args.no_last).then(|| crate::last::dir(args.cache.as_deref()));
    let kept = kept.iter().flat_map(|d| [d.join(crate::last::REPORT), d.join(crate::last::NOTES)]).collect::<Vec<_>>();
    let all: Vec<(&str, &Path)> = named
        .iter()
        .copied()
        .chain(cache.as_deref().map(|p| ("the cache", p)))
        .chain(kept.iter().map(|p| ("the kept scan", p.as_path())))
        .collect();
    for (i, (fa, a)) in all.iter().enumerate() {
        for (fb, b) in &all[i + 1..] {
            if same_destination(a, b) {
                anyhow::bail!("{fa} and {fb} would both write {}; give each its own file", b.display());
            }
        }
    }
    Ok(())
}

/// The format a path implies. Anything that is not `.csv` or `.json` is text:
/// the extension is a hint about a file the user named, not a declaration to
/// refuse.
fn from_extension(path: &Path) -> Format {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("csv") => Format::Csv,
        Some("json") => Format::Json,
        _ => Format::Txt,
    }
}

/// A descriptor this process holds, cloned: the clone is ours to close.
fn borrowed(fd: i32) -> std::io::Result<std::os::fd::OwnedFd> {
    // SAFETY: `fd` is open in this process, or `metadata` would not have
    // reached it through `/dev/fd`.
    unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) }.try_clone_to_owned()
}

/// What writing to a path writes to.
///
/// **Only a regular file is replaced.** A report is written beside its
/// destination and renamed over it, so that a run that dies part-way leaves
/// the last report whole; but a rename puts a new file where the old name
/// was, and that is a mistake for anything a name can stand for that is not
/// a file. It used to be done to all of them: `-o /dev/stdout` into a pipe,
/// and `-o >(gzip > r.gz)`, failed before the run began, since their link
/// ends in `pipe:[…]`, which names nothing a file can be made beside; a named
/// pipe was opened by the probe and closed again, which its reader took for
/// the end, and was then replaced by a file; `-o /dev/stdout >> log` replaced
/// the log instead of adding to it; and as root, `-o /dev/null` would have
/// put a file in place of the device.
enum Dest {
    /// A descriptor this process holds, named through `/dev/fd/N` or
    /// `/proc/self/fd/N` (which `/dev/stdout` and its kin are links to): it
    /// is written as it was opened, so a pipe stays a pipe and `>>` appends.
    Fd(i32),
    /// Not a regular file nor a folder: a named pipe, a device. Opened by its
    /// name and written in place.
    Special,
    /// A regular file, or nothing yet: at the end of any chain of symlinks.
    File(PathBuf),
}

fn destination(path: &Path) -> Dest {
    let (end, fd) = resolve(path);
    if let Some(fd) = fd {
        return Dest::Fd(fd);
    }
    match std::fs::metadata(&end) {
        Ok(m) if !m.is_file() && !m.is_dir() => Dest::Special,
        _ => Dest::File(end),
    }
}

/// Fails now if `path` could not be written later: it is a directory, its
/// parent is missing, or either refuses writes. Touches nothing that exists,
/// and removes the probe file when there was nothing there before. A
/// descriptor or a pipe is only asked whether it is there: opening a pipe to
/// find out is a write its reader sees end.
fn check_writable(path: &Path) -> Result<()> {
    let fail = || format!("cannot write output to {}", path.display());
    if path.is_dir() {
        return Err(anyhow::anyhow!("is a directory; name a file inside it")).with_context(fail);
    }
    // The probe file goes where the report will: through a symlink, at the
    // file it names. Created at `path` and removed at `path`, a link to a
    // file not yet there had its target made and the *link* deleted, so the
    // report was later written as a plain file in the link's place and an
    // empty file was left where the link had pointed.
    match destination(path) {
        Dest::Fd(_) | Dest::Special => {
            std::fs::metadata(path).with_context(fail)?;
        }
        Dest::File(dest) if dest.exists() => {
            std::fs::OpenOptions::new().write(true).open(&dest).with_context(fail)?;
        }
        Dest::File(dest) => {
            std::fs::File::create(&dest).with_context(fail)?;
            let _ = std::fs::remove_file(&dest);
        }
    }
    Ok(())
}

/// Write the file at `path` whole, or leave the one there as it was.
///
/// **A report replaces the last one only once it is written.** It used to be
/// created over it and filled in place, so a run that died while writing —
/// an interrupt during a quarter-gigabyte JSON, a disk filling up — left the
/// last run's report cut off where this one stopped: a valid 179 KB report
/// became 16 KB of JSON nothing could parse. Now it is written beside its
/// destination (through any symlink, as before) and renamed over it, with the
/// mode the file it replaces had; an interrupt removes the half-written one
/// (`cache::seal`). A folder that will not take a new file, where the file
/// itself may still be written, is written in place as before, and so is
/// anything that is not a regular file (`Dest`).
pub(crate) fn write_whole(path: &Path, body: impl FnOnce(&mut dyn Write) -> std::io::Result<()>) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let ctx = || format!("could not write {}", path.display());
    let dest = match destination(path) {
        Dest::Fd(fd) => return write_into(std::fs::File::from(borrowed(fd).with_context(ctx)?), body).with_context(ctx),
        Dest::Special => return write_into(std::fs::OpenOptions::new().write(true).open(path).with_context(ctx)?, body).with_context(ctx),
        Dest::File(dest) => dest,
    };
    let name = dest.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(&name);
    tmp_name.push(format!(".img-fp-{}.new", std::process::id()));
    let tmp = dest.with_file_name(tmp_name);
    let mode = std::fs::metadata(&dest).ok().map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()));
    let _ = std::fs::remove_file(&tmp);
    crate::cache::begin_partial(&tmp);
    let made = std::fs::OpenOptions::new().write(true).create_new(true).mode(mode.unwrap_or(0o666)).open(&tmp);
    let f = match made {
        Ok(f) => f,
        Err(_) => {
            crate::cache::abandon_partial(&tmp);
            return write_into(std::fs::File::create(path).with_context(ctx)?, body).with_context(ctx);
        }
    };
    let written = (|| -> std::io::Result<()> {
        // The mode asked for at creation is cut by the umask; the file it
        // replaces had the one it had.
        if let Some(m) = mode {
            f.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(m))?;
        }
        write_into(&f, body)?;
        crate::cache::finish_partial(&tmp, &dest)
    })();
    if written.is_err() {
        crate::cache::abandon_partial(&tmp);
    }
    written.with_context(ctx)
}

/// `body`, written into `f` through a buffer.
fn write_into(f: impl Write, body: impl FnOnce(&mut dyn Write) -> std::io::Result<()>) -> std::io::Result<()> {
    let mut w = std::io::BufWriter::new(f);
    body(&mut w)?;
    w.flush()
}

/// Whether writing to `a` and writing to `b` would write one file.
///
/// Two files that exist are one when they are one inode, which catches a hard
/// link and a symlink as well as the same path spelled twice. A file not there
/// yet is placed where a write would put it — through any symlink, into its
/// folder's canonical path — and compared by that.
fn same_destination(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
        return (x.dev(), x.ino()) == (y.dev(), y.ino());
    }
    let lands = |p: &Path| -> Option<PathBuf> {
        let t = resolve(p).0;
        let dir = match t.parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        Some(std::fs::canonicalize(dir).ok()?.join(t.file_name()?))
    };
    matches!((lands(a), lands(b)), (Some(x), Some(y)) if x == y)
}

/// Where writing to `path` would land: the end of the chain of symlinks it
/// starts, however far that is from existing — or, beside it, the descriptor
/// the chain names on the way (`Dest::Fd`), where it stops.
fn resolve(path: &Path) -> (PathBuf, Option<i32>) {
    let fd_of = |p: &Path| -> Option<i32> {
        let n = p.strip_prefix("/dev/fd").or_else(|_| p.strip_prefix("/proc/self/fd")).ok()?;
        n.to_str()?.parse().ok()
    };
    let mut p = path.to_path_buf();
    // The kernel's own limit on a chain, so a loop ends where `open` would.
    for _ in 0..40 {
        if let Some(fd) = fd_of(&p) {
            return (p, Some(fd));
        }
        match std::fs::read_link(&p) {
            Ok(to) => p = p.parent().map_or(to.clone(), |dir| dir.join(&to)),
            Err(_) => break,
        }
    }
    (p, None)
}


#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Two outputs never share stdout, and under the window neither the dump
    /// nor the log may use it at all.
    #[test]
    fn stdout_carries_one_output() {
        // Named files are probed, so they are named where a probe may go.
        let dir = std::env::temp_dir().join(format!("img-fp-stdout-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (r, dash) = (dir.join("r.json"), dir.join("-"));
        let (r, dash) = (r.to_str().unwrap(), dash.to_str().unwrap());
        let check = |extra: &[&str], gui: bool| plan(&args(extra), gui).is_ok();
        assert!(check(&[], false));
        assert!(check(&["-o", r, "--dump", "-"], false));
        assert!(check(&["-o", r, "--log-file", "-"], false));
        assert!(!check(&["--dump", "-"], false), "the report is on stdout already");
        assert!(!check(&["-o", "-", "--log-file", "-"], false));
        assert!(!check(&["-o", r, "--dump", "-", "--log-file", "-"], false));
        assert!(check(&["--dump", dash], false), "a file named - is a path to it");
        assert!(check(&[], true), "the window's report is a file");
        assert!(!check(&["--dump", "-"], true));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Two outputs on one file are refused before anything is written.
    #[test]
    fn two_outputs_on_one_file_are_refused() {
        let planned = |a: &Args| plan(a, false);
        let dir = std::env::temp_dir().join(format!("img-fp-outputs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (r, l) = (dir.join("r.txt"), dir.join("l.txt"));
        let (r, l) = (r.to_str().unwrap(), l.to_str().unwrap());
        assert!(planned(&args(&["-o", r, "--log-file", l])).is_ok());
        assert!(planned(&args(&["-o", r, "--log-file", r])).is_err());
        assert!(planned(&args(&["-o", r, "--dump", r])).is_err());
        assert!(planned(&args(&["--dump", r, "--log-file", r, "-o", l])).is_err());
        assert!(planned(&args(&["-o", "-", "--log-file", r])).is_ok());
        // Nor the cache, while the run uses it.
        assert!(planned(&args(&["-o", r, "--cache", r])).is_err());
        assert!(planned(&args(&["--log-file", l, "--cache", l])).is_err());
        assert!(planned(&args(&["-o", r, "--cache", l])).is_ok());
        assert!(planned(&args(&["-o", r, "--cache", r, "--no-cache", "--clear-cache"])).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn args(extra: &[&str]) -> Args {
        Args::try_parse_from(["img-fp"].iter().chain(extra).chain(&["."])).unwrap()
    }

    /// The report goes to stdout unless something else has it, in the
    /// format its name implies unless `--format` says otherwise.
    #[test]
    fn the_report_goes_where_one_rule_says() {
        let report = |extra: &[&str], window: bool| plan(&args(extra), window).unwrap().report;
        assert_eq!(report(&[], false), Some(Report { to: To::Stdout, format: Format::Txt }));
        assert_eq!(report(&["--format", "csv"], false), Some(Report { to: To::Stdout, format: Format::Csv }));
        assert_eq!(report(&["-o", "-", "--format", "json"], false), Some(Report { to: To::Stdout, format: Format::Json }));
        assert_eq!(report(&["-o", "./-"], false).unwrap().to, To::Path("./-".into()), "a file named - is ./-");
        assert_eq!(report(&["--against", "/l", "--list", "new"], false), None, "the list has stdout");
        assert_eq!(report(&[], true), None, "the window's stdout is its progress");
        let r = std::env::temp_dir().join(format!("img-fp-plan-{}.JSON", std::process::id()));
        assert_eq!(report(&["-o", r.to_str().unwrap()], true).unwrap().format, Format::Json);
        assert!(plan(&args(&["-o", "-"]), true).is_err(), "not dropped unsaid");
        for (name, f) in [("r.csv", Format::Csv), ("r.JSON", Format::Json), ("r.txt", Format::Txt), ("r.bak", Format::Txt)] {
            assert_eq!(Report::new(To::Path(name.into()), None).format, f, "{name}");
        }
        assert_eq!(Report::new(To::Path("r.bak".into()), Some(Format::Json)).format, Format::Json);
    }

    /// The kept scan beside the cache is one of the files no output may name,
    /// while a scan keeps one.
    #[test]
    fn no_output_writes_over_the_kept_scan() {
        let dir = std::env::temp_dir().join(format!("img-fp-kept-{}", std::process::id()));
        let kept = crate::last::dir(Some(&dir.join("c.bin")));
        std::fs::create_dir_all(&kept).unwrap();
        let found = kept.join(crate::last::REPORT);
        std::fs::write(&found, "{}").unwrap();
        let (c, f) = (dir.join("c.bin"), found.to_str().unwrap().to_string());
        let c = c.to_str().unwrap();
        assert!(plan(&args(&["--cache", c, "-o", &f]), false).is_err());
        assert!(plan(&args(&["--cache", c, "--log-file", &f]), false).is_err());
        assert!(plan(&args(&["--cache", c, "--last", "-o", &f]), false).is_err(), "--last reads it, and the next one would");
        assert!(plan(&args(&["--cache", c, "--no-last", "-o", &f]), false).is_ok(), "nothing is kept");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The log through a descriptor is written as it was opened: onto the end
    /// of a file opened to append, not over it; a file it names is begun anew.
    #[test]
    fn a_log_to_a_descriptor_appends_and_a_file_is_truncated() {
        use std::os::fd::AsRawFd;
        let dir = std::env::temp_dir().join(format!("img-fp-logfd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("run.log");
        std::fs::write(&log, "before\n").unwrap();
        let f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        let mut w = To::Path(format!("/proc/self/fd/{}", f.as_raw_fd()).into()).open_log().unwrap();
        w.write_all(b"after\n").unwrap();
        drop(w);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "before\nafter\n");
        let mut w = To::Path(log.clone()).open_log().unwrap();
        w.write_all(b"this run\n").unwrap();
        drop(w);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "this run\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// One file named two ways is one destination; two files are two.
    #[test]
    fn one_file_by_two_names_is_one_destination() {
        let dir = std::env::temp_dir().join(format!("img-fp-dest-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        assert!(same_destination(&a, &dir.join("sub/../a.txt")), "neither there yet, one place");
        assert!(!same_destination(&a, &b));
        std::fs::write(&a, "x").unwrap();
        std::fs::hard_link(&a, dir.join("hard.txt")).unwrap();
        std::os::unix::fs::symlink("a.txt", dir.join("soft.txt")).unwrap();
        assert!(same_destination(&a, &dir.join("hard.txt")));
        assert!(same_destination(&a, &dir.join("soft.txt")));
        std::os::unix::fs::symlink("new.txt", dir.join("to-new.txt")).unwrap();
        assert!(same_destination(&dir.join("to-new.txt"), &dir.join("new.txt")), "a link to a file not there yet");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A report replaces the last one only once it is written whole: one that
    /// fails part-way leaves the last as it was and nothing beside it, and
    /// one written through a symlink keeps the link and the target's mode.
    #[test]
    fn a_report_replaces_the_last_only_when_written_whole() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("img-fp-whole-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("r.json");
        std::fs::write(&out, "the last report").unwrap();
        std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o640)).unwrap();
        let failed = write_whole(&out, |w| {
            w.write_all(b"half of a new")?;
            Err(std::io::Error::other("disk full"))
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "the last report");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "nothing left beside it");
        let link = dir.join("link.json");
        std::os::unix::fs::symlink("r.json", &link).unwrap();
        write_whole(&link, |w| w.write_all(b"the new report")).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "the new report");
        assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o640);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A report to a descriptor is written into it as it was opened: into a
    /// pipe, which stays a pipe, and onto the end of a file opened to append;
    /// and a device is written, not replaced.
    #[test]
    fn a_report_to_a_pipe_a_descriptor_or_a_device_is_written_in_place() {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        let (mut r, w) = std::io::pipe().unwrap();
        let at = PathBuf::from(format!("/dev/fd/{}", w.as_raw_fd()));
        check_writable(&at).unwrap();
        write_whole(&at, |o| o.write_all(b"through the pipe")).unwrap();
        drop(w);
        let mut got = String::new();
        r.read_to_string(&mut got).unwrap();
        assert_eq!(got, "through the pipe");

        let dir = std::env::temp_dir().join(format!("img-fp-fd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("log.txt");
        std::fs::write(&log, "before\n").unwrap();
        let f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        write_whole(&PathBuf::from(format!("/proc/self/fd/{}", f.as_raw_fd())), |o| o.write_all(b"after\n")).unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "before\nafter\n");
        std::fs::remove_dir_all(&dir).ok();

        write_whole(Path::new("/dev/null"), |o| o.write_all(b"nothing")).unwrap();
        use std::os::unix::fs::FileTypeExt;
        assert!(std::fs::metadata("/dev/null").unwrap().file_type().is_char_device());
    }

    /// Checking that `-o` can be written leaves a symlink a symlink, and
    /// leaves nothing behind where it points.
    #[test]
    fn the_output_probe_writes_through_a_link_and_leaves_it_alone() {
        let dir = std::env::temp_dir().join(format!("img-fp-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let link = dir.join("out.json");
        std::os::unix::fs::symlink("target.json", &link).unwrap();
        check_writable(&link).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(!dir.join("target.json").exists());
        let deep = dir.join("deep.json");
        std::os::unix::fs::symlink(dir.join("missing/x.json"), &deep).unwrap();
        assert!(check_writable(&deep).is_err(), "a link into a folder that is not there cannot be written");
        std::fs::remove_dir_all(&dir).ok();
    }
}
