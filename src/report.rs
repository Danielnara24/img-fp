//! The results, in the three layouts `vid-fp` writes: text, CSV and JSON.
//!
//! The JSON is the complete record and the one the benchmark reads — every
//! pair the run asserts, with what it rests on, and the groups built from
//! them. The text and the CSV are the groups, one row per file, which is what
//! a person deciding what to delete actually reads: the text is for looking
//! at, the CSV for sorting. Both put each member beside the evidence it was
//! matched to its group's representative on, since that pair is the whole of
//! what a group claims about it.
//!
//! The format follows `-o`'s extension unless `--format` says otherwise, and
//! anything unrecognised is text, as in `vid-fp`. Unlike `vid-fp`, a run with
//! no `-o` still has a report — the text on stdout — so `--format` alone is
//! not refused: it is that same report in another layout.

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Serialize;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::outcome::{Evidence, Member, Outcome, Relation};
use crate::suggest::{Action, Mode};

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Txt,
    Csv,
    Json,
}

/// Where the report goes. `-o -` is stdout, which is also where it goes with
/// no `-o` at all; a file really named `-` is `./-`.
pub enum Sink {
    Stdout,
    File(PathBuf),
}

pub struct Target {
    pub sink: Sink,
    pub format: Format,
}

impl Target {
    /// The file the report goes to, if it goes to one.
    pub fn file(&self) -> Option<PathBuf> {
        match &self.sink {
            Sink::File(path) => Some(path.clone()),
            _ => None,
        }
    }

    /// `--output` and `--format` read together, in the one place either is.
    pub fn of(output: Option<&Path>, format: Option<Format>) -> Target {
        match output {
            None => Target { sink: Sink::Stdout, format: format.unwrap_or(Format::Txt) },
            Some(p) if p == Path::new("-") => Target { sink: Sink::Stdout, format: format.unwrap_or(Format::Txt) },
            Some(p) => {
                Target { sink: Sink::File(p.to_path_buf()), format: format.unwrap_or_else(|| from_extension(p)) }
            }
        }
    }
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
pub fn check_writable(path: &Path) -> Result<()> {
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
        Dest::Fd(fd) => {
            // SAFETY: `fd` is open in this process, or `metadata` would not
            // have reached it; the clone is ours to close.
            let fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) }.try_clone_to_owned().with_context(ctx)?;
            return write_into(std::fs::File::from(fd), body).with_context(ctx);
        }
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
pub fn same_destination(a: &Path, b: &Path) -> bool {
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

/// A path's bytes, when they are not UTF-8 and so cannot be a JSON string.
///
/// Every path in the JSON is written as text, and a name that is not UTF-8
/// comes out of that with `U+FFFD` where its odd bytes were: readable, and the
/// name of no file on disk. Anything that acts on the report — a script
/// deleting the matches, or the window moving one to the Trash — needs the
/// name itself, so such a path also carries its bytes beside it, as an array
/// of numbers. A UTF-8 path, which is nearly every path, carries nothing.
pub fn raw_bytes(p: &Path) -> Option<Vec<u8>> {
    match p.to_str() {
        Some(_) => None,
        None => Some(p.as_os_str().as_bytes().to_vec()),
    }
}

/// One stated pair, as the JSON's `pairs` list writes it.
pub struct OutPair {
    /// Shared with every other pair naming the same file, and written as the
    /// string it is: a copy of both names per pair was 2.3 million strings on
    /// 27,000 pictures, the largest thing alive where such a run peaks.
    pub a: std::sync::Arc<str>,
    pub b: std::sync::Arc<str>,
    /// `a` and `b` as bytes, for a name that is not UTF-8; see `raw_bytes`.
    pub a_bytes: Option<Vec<u8>>,
    pub b_bytes: Option<Vec<u8>>,
    pub ev: Evidence,
    /// How many pixels of `b` one pixel of `a` spans, in the files' own
    /// pixels: 0.5 when `b` is `a` at half the size. See `lib::file_scale`.
    pub scale: f32,
    /// Indices into the run's file list. Not written: `a` and `b` say the
    /// same thing.
    pub ia: usize,
    pub ib: usize,
}

/// A pair as the JSON writes it: `aligned_points` 0 where no keypoint was
/// matched, and the relation as a flag, set only for what is not `direct`.
#[derive(Serialize)]
struct Written<'a> {
    a: &'a str,
    b: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    a_bytes: &'a Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    b_bytes: &'a Option<Vec<u8>>,
    aligned_points: u32,
    frame_overlap: f32,
    pixel_correlation: f32,
    scale: f32,
    /// A left-right flip, folded into the transform as a negative
    /// determinant — so a composed pair carries it too.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    mirrored: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    inverted: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    identical: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    same_pixels: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    propagated: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    corroborated: bool,
}

impl Serialize for OutPair {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let r = self.ev.relation;
        Written {
            a: &self.a,
            b: &self.b,
            a_bytes: &self.a_bytes,
            b_bytes: &self.b_bytes,
            aligned_points: self.ev.aligned_points.unwrap_or(0),
            frame_overlap: self.ev.frame_overlap,
            pixel_correlation: self.ev.pixel_correlation,
            scale: self.scale,
            mirrored: self.ev.mirrored,
            inverted: self.ev.inverted,
            identical: r == Relation::Identical,
            same_pixels: r == Relation::SamePixels,
            propagated: r == Relation::Propagated,
            corroborated: r == Relation::Corroborated,
        }
        .serialize(s)
    }
}

/// Write the report of `scan` where `target` says, in the layout it says,
/// with `mode`'s suggestions; the JSON lists `pairs` when there are any to
/// list, which only a scan has.
///
/// A reader that goes away early — `img-fp DIR | head` — is how a pipeline
/// ends, not a failed run, so a broken pipe on stdout is not an error.
pub fn write_with(target: &Target, scan: &Outcome, mode: Mode, pairs: Option<&[OutPair]>) -> Result<()> {
    let said = Said::of(scan, mode);
    let body = |w: &mut dyn Write| -> std::io::Result<()> {
        match target.format {
            Format::Json => write_json(w, &said, pairs),
            Format::Txt => write_txt(w, &said),
            Format::Csv => write_csv(w, &said),
        }
    };
    match &target.sink {
        // Fatal, and the context is the whole of what makes exit 1 useful:
        // this is the run's output, there is nowhere else it went, and "No
        // such file or directory" on its own does not say which file.
        Sink::File(path) => write_whole(path, body),
        Sink::Stdout => {
            let mut w = std::io::BufWriter::new(std::io::stdout().lock());
            match body(&mut w).and_then(|()| w.flush()) {
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                other => other.context("could not write the results to stdout"),
            }
        }
    }
}

/// A scan and what one rule says of each of its files: what every layout
/// writes.
struct Said<'a> {
    scan: &'a Outcome,
    mode: Mode,
    actions: Vec<Option<Action>>,
    groups: Vec<Vec<Member>>,
}

impl<'a> Said<'a> {
    fn of(scan: &'a Outcome, mode: Mode) -> Said<'a> {
        Said { scan, mode, actions: scan.actions(mode), groups: scan.shown().collect() }
    }

    /// A group's rows, representative first. A file in two groups has a row
    /// in each, because its evidence differs: each row is its pair with
    /// *that* group's representative.
    fn rows(&self, g: &'a [Member]) -> impl Iterator<Item = Row<'a>> + '_ {
        let files = &self.scan.files;
        g.iter().map(move |m| {
            let f = &files[m.file];
            let action = self.actions[m.file];
            let kept = if self.mode == Mode::Content { self.scan.kept_copy(m.file) } else { None };
            let kept = kept.filter(|_| action == Some(Action::Delete)).map(|k| &files[k].path);
            let p = m.with;
            Row {
                path: f.path.display().to_string(),
                path_bytes: raw_bytes(&f.path),
                raw: &f.path,
                role: if p.is_none() { "representative" } else { "match" },
                action: action.map(Action::as_str),
                width: f.dims.map(|d| d.0),
                height: f.dims.map(|d| d.1),
                size: f.bytes.map(format_size),
                size_bytes: f.bytes,
                relation: p.map(|p| p.relation.as_str()),
                aligned_points: p.and_then(|p| p.aligned_points),
                frame_overlap: p.map(|p| p.frame_overlap),
                pixel_correlation: p.map(|p| p.pixel_correlation),
                mirrored: p.map(|p| p.mirrored),
                inverted: p.map(|p| p.inverted),
                damaged: f.damaged,
                kept_copy: kept.map(|k| k.display().to_string()),
                kept_copy_bytes: kept.and_then(|k| raw_bytes(k)),
            }
        })
    }
}

/// One group as the JSON states it: the text report's block, with each row
/// an object whose keys are the CSV's columns.
#[derive(Serialize)]
struct JsonGroup<'a> {
    group: String,
    representative: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    representative_bytes: Option<Vec<u8>>,
    files: Vec<Row<'a>>,
}

/// The JSON, one group, pair and failure to a line.
///
/// It is still one JSON document to any parser. Written on a single line it
/// was a megabyte-long line on a found corpus and fifty on the benchmark one,
/// and an editor asked to open that hangs; pretty-printed it would be ten lines
/// a pair. One record a line opens anywhere and still greps.
fn write_json(w: &mut dyn Write, said: &Said, pairs: Option<&[OutPair]>) -> std::io::Result<()> {
    let head = &said.scan.head;
    let groups: Vec<JsonGroup> = said
        .groups
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            let rep = &said.scan.files[g[0].file].path;
            JsonGroup { group: group_name(gi), representative: rep.display().to_string(), representative_bytes: raw_bytes(rep), files: said.rows(g).collect() }
        })
        .collect();
    let mut config = head.config.clone();
    config["suggest"] = format!("{:?}", said.mode).to_lowercase().into();
    writeln!(w, "{{")?;
    writeln!(w, "  \"tool\": \"img-fp\",")?;
    writeln!(w, "  \"config\": {},", serde_json::to_string(&config)?)?;
    writeln!(w, "  \"files_enumerated\": {},", head.files_enumerated)?;
    writeln!(w, "  \"files_analysed\": {},", head.files_analysed)?;
    list(w, "failures", &head.failures, false)?;
    writeln!(w, "  \"runtime_seconds\": {},", serde_json::to_string(&head.runtime_seconds)?)?;
    list(w, "groups", &groups, pairs.is_none())?;
    if let Some(pairs) = pairs {
        list_par(w, "pairs", pairs, true)?;
    }
    writeln!(w, "}}")
}

/// One key of the document and its list, a record a line.
pub(crate) fn list<T: Serialize>(w: &mut dyn Write, key: &str, xs: &[T], last: bool) -> std::io::Result<()> {
    write!(w, "  \"{key}\": [")?;
    for (i, x) in xs.iter().enumerate() {
        write!(w, "{}\n    ", if i == 0 { "" } else { "," })?;
        serde_json::to_writer(&mut *w, x)?;
    }
    writeln!(w, "{}]{}", if xs.is_empty() { "" } else { "\n  " }, if last { "" } else { "," })
}

/// `list`, for a list long enough to be worth writing on every thread: each
/// piece of it is written into a buffer of its own, and the buffers go out in
/// order, so the bytes are the ones `list` writes. The pairs of a corpus of
/// 27,000 pictures are a million records and a quarter of a gigabyte, and
/// they were the last thing a run did on one thread. A batch of pieces at a
/// time, so that the report is never held whole.
pub(crate) fn list_par<T: Serialize + Sync>(w: &mut dyn Write, key: &str, xs: &[T], last: bool) -> std::io::Result<()> {
    const PIECE: usize = 4096;
    const BATCH: usize = 64 * PIECE;
    write!(w, "  \"{key}\": [")?;
    for (bi, batch) in xs.chunks(BATCH).enumerate() {
        let bufs: Vec<std::io::Result<Vec<u8>>> = batch
            .par_chunks(PIECE)
            .enumerate()
            .map(|(pi, piece)| {
                let mut buf = Vec::with_capacity(piece.len() * 256);
                for (k, x) in piece.iter().enumerate() {
                    let first = bi == 0 && pi == 0 && k == 0;
                    buf.extend_from_slice(if first { b"\n    " } else { b",\n    " });
                    serde_json::to_writer(&mut buf, x)?;
                }
                Ok(buf)
            })
            .collect();
        for b in bufs {
            w.write_all(&b?)?;
        }
    }
    writeln!(w, "{}]{}", if xs.is_empty() { "" } else { "\n  " }, if last { "" } else { "," })
}

pub(crate) fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_073_741_824.0 {
        format!("{:.1}GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.1}MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.1}KB", b / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

fn group_name(gi: usize) -> String {
    format!("group_{}", gi + 1)
}

/// One file of one group, as all three layouts state it: the CSV writes each
/// field as a column, the JSON as a key, and the text as a clause.
///
/// `None` is a figure that was not measured, and it is an empty cell or a
/// `null` rather than a zero: the representative's evidence (it is what the
/// others were compared with), `aligned_points` on a pair no keypoints vouch
/// for, `width` on a header the probe cannot read.
#[derive(Serialize)]
struct Row<'a> {
    path: String,
    /// `path` as bytes, for a name that is not UTF-8; see `raw_bytes`.
    #[serde(skip_serializing_if = "Option::is_none")]
    path_bytes: Option<Vec<u8>>,
    /// The path itself, which the text and the CSV write as it is.
    #[serde(skip)]
    raw: &'a Path,
    role: &'static str,
    /// KEEP, DELETE or REVIEW: what the run suggests doing with the file,
    /// which is one answer for the file in every group it is in.
    action: Option<&'static str>,
    width: Option<u32>,
    height: Option<u32>,
    size: Option<String>,
    size_bytes: Option<u64>,
    relation: Option<&'static str>,
    aligned_points: Option<u32>,
    frame_overlap: Option<f32>,
    pixel_correlation: Option<f32>,
    mirrored: Option<bool>,
    inverted: Option<bool>,
    /// The file decoded only in part. Written only when it did.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    damaged: bool,
    /// For a DELETE, the kept file that shows everything this one does.
    /// The JSON's alone: the text and the CSV keep their columns.
    #[serde(skip_serializing_if = "Option::is_none")]
    kept_copy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kept_copy_bytes: Option<Vec<u8>>,
}

/// Width of the role column, comma included: `MATCH,`.
const ROLE_COLUMN: usize = 6;
/// Width of the action column, comma included: `DELETE,`.
const ACTION_COLUMN: usize = 7;

/// One block per group, representative first, the layout `vid-fp`'s text
/// report has: the role leads at a fixed width so it forms a column, and the
/// path trails because it is the one field with no bounded length. Every
/// figure carries its name, since there is no header to say which is which.
fn write_txt(w: &mut dyn Write, said: &Said) -> std::io::Result<()> {
    for (gi, g) in said.groups.iter().enumerate() {
        writeln!(w, "{}: {} files", group_name(gi), g.len())?;
        for r in said.rows(g) {
            let role = if r.role == "representative" { "REP," } else { "MATCH," };
            let action = format!("{},", r.action.unwrap_or("-"));
            let dims = match (r.width, r.height) {
                (Some(w), Some(h)) => format!("{w}x{h}"),
                _ => "-".into(),
            };
            let size = r.size.as_deref().unwrap_or("-");
            let damaged = if r.damaged { "DAMAGED, " } else { "" };
            // The path as its own bytes, as `ls` would print it: a name that
            // is not UTF-8 is still the name of the file.
            write!(w, "\t{role:<ROLE_COLUMN$} {action:<ACTION_COLUMN$} {dims}, {size}, {damaged}{}", evidence(&r))?;
            w.write_all(r.raw.as_os_str().as_bytes())?;
            writeln!(w)?;
        }
        writeln!(w)?;
    }
    Ok(())
}

/// The evidence clause of a member's text row, ending in the separator before
/// the path. Empty for the representative.
fn evidence(r: &Row) -> String {
    let (Some(how), Some(ov), Some(corr)) = (r.relation, r.frame_overlap, r.pixel_correlation) else {
        return String::new();
    };
    let mut s = match (how, r.aligned_points) {
        ("identical", _) => "identical".to_string(),
        ("same_pixels", _) => "same pixels".to_string(),
        ("corroborated", Some(n)) => format!("corroborated, {n} points, overlap {ov:.2}, correlation {corr:.2}"),
        (_, Some(n)) => format!("{n} points, overlap {ov:.2}, correlation {corr:.2}"),
        (how, None) => format!("{how}, overlap {ov:.2}, correlation {corr:.2}"),
    };
    if r.mirrored == Some(true) {
        s.push_str(", mirrored");
    }
    if r.inverted == Some(true) {
        s.push_str(", inverted");
    }
    s.push_str(", ");
    s
}

/// The CSV's columns: `group`, then a `Row`'s fields in its order.
const CSV_HEADER: [&str; 15] = [
    "group",
    "role",
    "action",
    "path",
    "width",
    "height",
    "size",
    "size_bytes",
    "relation",
    "aligned_points",
    "frame_overlap",
    "pixel_correlation",
    "mirrored",
    "inverted",
    "damaged",
];

/// One row per file per group, `;`-separated as `vid-fp`'s is, an unmeasured
/// figure an empty cell.
fn write_csv(w: &mut dyn Write, said: &Said) -> std::io::Result<()> {
    fn cell<T: ToString>(x: Option<T>) -> String {
        x.map(|x| x.to_string()).unwrap_or_default()
    }
    csv_row(w, &CSV_HEADER.map(str::as_bytes))?;
    for (gi, g) in said.groups.iter().enumerate() {
        let group = group_name(gi);
        for r in said.rows(g) {
            csv_row(
                w,
                &[
                    group.as_bytes(),
                    r.role.as_bytes(),
                    r.action.unwrap_or_default().as_bytes(),
                    r.raw.as_os_str().as_bytes(),
                    cell(r.width).as_bytes(),
                    cell(r.height).as_bytes(),
                    r.size.as_deref().unwrap_or_default().as_bytes(),
                    cell(r.size_bytes).as_bytes(),
                    r.relation.unwrap_or_default().as_bytes(),
                    cell(r.aligned_points).as_bytes(),
                    cell(r.frame_overlap).as_bytes(),
                    cell(r.pixel_correlation).as_bytes(),
                    cell(r.mirrored).as_bytes(),
                    cell(r.inverted).as_bytes(),
                    if r.damaged { b"true".as_slice() } else { b"".as_slice() },
                ],
            )?;
        }
    }
    Ok(())
}

/// A field is quoted when it holds the separator, a quote or a line break,
/// with its quotes doubled — RFC 4180's rule, which is what every spreadsheet
/// reads. Only a path can need it, and a path is written as its own bytes.
fn csv_row(w: &mut dyn Write, fields: &[&[u8]]) -> std::io::Result<()> {
    csv_row_with(w, fields, b';')
}

/// One CSV row with `sep` between the fields: a field holding the separator,
/// a quote or a line break is quoted, its quotes doubled, and every field is
/// written as its own bytes. `--dump` writes with commas.
pub fn csv_row_with(w: &mut dyn Write, fields: &[&[u8]], sep: u8) -> std::io::Result<()> {
    for (k, f) in fields.iter().enumerate() {
        if k > 0 {
            w.write_all(&[sep])?;
        }
        if f.iter().any(|&b| b == sep || matches!(b, b'"' | b'\n' | b'\r')) {
            w.write_all(b"\"")?;
            for piece in f.split_inclusive(|&b| b == b'"') {
                w.write_all(piece)?;
                if piece.ends_with(b"\"") {
                    w.write_all(b"\"")?;
                }
            }
            w.write_all(b"\"")?;
        } else {
            w.write_all(f)?;
        }
    }
    w.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{File, Group, Head};

    fn ev(relation: Relation) -> Evidence {
        let points = (!relation.one_file()).then_some(42);
        Evidence { relation, aligned_points: points, frame_overlap: 0.987, pixel_correlation: 0.912, mirrored: false, inverted: false }
    }

    fn pair(ia: usize, ib: usize) -> OutPair {
        OutPair { a: format!("/{ia}.jpg").into(), b: format!("/{ib}.jpg").into(), a_bytes: None, b_bytes: None, ev: ev(Relation::Direct), scale: 0.5, ia, ib }
    }

    fn file(p: &str) -> File {
        File { path: PathBuf::from(p), dims: None, bytes: None, damaged: false }
    }

    /// One group: `/0.jpg`, which holds an identical `/1.jpg` and a mirrored
    /// `/2;x.jpg`.
    fn scan() -> Outcome {
        let mut files = vec![file("/0.jpg"), file("/1.jpg"), file("/2;x.jpg")];
        (files[0].dims, files[0].bytes) = (Some((4032, 3024)), Some(3_250_000));
        let mirrored = Evidence { mirrored: true, ..ev(Relation::Direct) };
        let members = vec![Member { file: 0, with: None }, Member { file: 1, with: Some(ev(Relation::Identical)) }, Member { file: 2, with: Some(mirrored) }];
        let head = Head { config: serde_json::json!({}), files_enumerated: 3, files_analysed: 3, failures: vec![], runtime_seconds: 1.0 };
        Outcome::new(head, 0.7, files, vec![Group { members }], vec![Some(Action::Keep), Some(Action::Delete), Some(Action::Review)], vec![None, Some(0), None])
    }

    fn pairs() -> Vec<OutPair> {
        let mut mirrored = pair(0, 2);
        mirrored.ev.mirrored = true;
        let mut identical = pair(1, 0);
        identical.ev = ev(Relation::Identical);
        vec![identical, mirrored]
    }

    fn txt(o: &Outcome) -> Vec<u8> {
        let mut buf = Vec::new();
        write_txt(&mut buf, &Said::of(o, Mode::Content)).unwrap();
        buf
    }

    fn csv(o: &Outcome) -> Vec<u8> {
        let mut buf = Vec::new();
        write_csv(&mut buf, &Said::of(o, Mode::Content)).unwrap();
        buf
    }

    fn json(o: &Outcome, pairs: Option<&[OutPair]>) -> serde_json::Value {
        let mut buf = Vec::new();
        write_json(&mut buf, &Said::of(o, Mode::Content), pairs).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    #[test]
    fn the_extension_decides_unless_format_does() {
        let t = |o: Option<&str>, f| Target::of(o.map(Path::new), f).format;
        assert_eq!(t(Some("r.csv"), None), Format::Csv);
        assert_eq!(t(Some("r.JSON"), None), Format::Json);
        assert_eq!(t(Some("r.txt"), None), Format::Txt);
        assert_eq!(t(Some("r.bak"), None), Format::Txt);
        assert_eq!(t(Some("r.bak"), Some(Format::Json)), Format::Json);
        assert_eq!(t(None, None), Format::Txt);
        assert_eq!(t(None, Some(Format::Csv)), Format::Csv);
        assert!(matches!(Target::of(Some(Path::new("-")), None).sink, Sink::Stdout));
        assert!(matches!(Target::of(Some(Path::new("./-")), None).sink, Sink::File(_)));
    }

    #[test]
    fn a_text_row_is_the_members_pair_with_its_representative() {
        assert_eq!(
            String::from_utf8(txt(&scan())).unwrap(),
            "group_1: 3 files\n\
             \tREP,   KEEP,   4032x3024, 3.1MB, /0.jpg\n\
             \tMATCH, DELETE, -, -, identical, /1.jpg\n\
             \tMATCH, REVIEW, -, -, 42 points, overlap 0.99, correlation 0.91, mirrored, /2;x.jpg\n\n"
        );
    }

    #[test]
    fn a_csv_row_quotes_only_what_needs_it() {
        let mut o = scan();
        (o.files[0].dims, o.files[0].bytes) = (None, None);
        let text = String::from_utf8(csv(&o)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], CSV_HEADER.join(";"));
        assert_eq!(lines[1], "group_1;representative;KEEP;/0.jpg;;;;;;;;;;;");
        assert_eq!(lines[2], "group_1;match;DELETE;/1.jpg;;;;;identical;;0.987;0.912;false;false;");
        assert_eq!(lines[3], "group_1;match;REVIEW;\"/2;x.jpg\";;;;;direct;42;0.987;0.912;true;false;");
    }

    #[test]
    fn the_json_is_one_document() {
        let v = json(&scan(), Some(&pairs()));
        let g = &v["groups"][0];
        assert_eq!(g["group"], "group_1");
        assert_eq!(g["representative"], "/0.jpg");
        assert_eq!(
            g["files"][0],
            serde_json::json!({"path": "/0.jpg", "role": "representative", "action": "KEEP", "width": 4032, "height": 3024,
                "size": "3.1MB", "size_bytes": 3_250_000, "relation": null, "aligned_points": null,
                "frame_overlap": null, "pixel_correlation": null, "mirrored": null, "inverted": null})
        );
        assert_eq!(g["files"][1]["relation"], "identical");
        assert_eq!(g["files"][1]["aligned_points"], serde_json::Value::Null);
        assert_eq!(g["files"][2]["path"], "/2;x.jpg");
        assert_eq!(g["files"][2]["aligned_points"], 42);
        assert_eq!(g["files"][2]["mirrored"], true);
        // A DELETE names the kept file it rests on; nothing else does.
        assert_eq!(g["files"][1]["kept_copy"], "/0.jpg");
        assert!(g["files"][2].get("kept_copy").is_none(), "a REVIEW rests on nothing");
        assert_eq!(v["pairs"][1]["mirrored"], true);
        assert!(v["pairs"][0].get("mirrored").is_none());
        assert!(v["pairs"][0].get("ia").is_none());
    }

    #[test]
    fn the_pairs_are_written_on_every_thread_byte_for_byte() {
        for n in [0usize, 1, 2, 4095, 4096, 4097, 300_000] {
            let xs: Vec<OutPair> = (0..n).map(|i| pair(i, i + 1)).collect();
            for last in [false, true] {
                let (mut a, mut b) = (Vec::new(), Vec::new());
                list(&mut a, "pairs", &xs, last).unwrap();
                list_par(&mut b, "pairs", &xs, last).unwrap();
                assert!(a == b, "{n} pairs");
            }
        }
    }

    /// `--last`'s JSON is the same document without the pairs, and still one
    /// document.
    #[test]
    fn a_report_without_pairs_is_still_one_document() {
        let v = json(&scan(), None);
        assert!(v.get("pairs").is_none());
        assert_eq!(v["groups"][0]["files"][2]["relation"], "direct", "a row's evidence is still its pair's");
    }

    #[test]
    fn a_corroborated_pair_says_so_in_every_format() {
        let mut o = scan();
        let mut pairs = pairs();
        let corroborated = Evidence { relation: Relation::Corroborated, aligned_points: Some(9), pixel_correlation: 0.54, mirrored: true, ..ev(Relation::Direct) };
        o.groups[0].members[2].with = Some(corroborated);
        pairs[1].ev = corroborated;
        assert!(String::from_utf8(txt(&o))
            .unwrap()
            .contains("\tMATCH, REVIEW, -, -, corroborated, 9 points, overlap 0.99, correlation 0.54, mirrored, /2;x.jpg\n"));
        assert!(String::from_utf8(csv(&o)).unwrap().contains(";corroborated;9;0.987;0.54;true;false"));
        let v = json(&o, Some(&pairs));
        assert_eq!(v["groups"][0]["files"][2]["relation"], "corroborated");
        assert_eq!(v["pairs"][1]["corroborated"], true);
        assert!(v["pairs"][0].get("corroborated").is_none());
    }

    /// A name that is not UTF-8 is written as itself by the text and the CSV,
    /// and carried as bytes beside its readable form by the JSON, so that
    /// whatever acts on a report acts on the file and not on a name no file
    /// has.
    #[test]
    fn a_name_that_is_not_utf8_reaches_every_format_intact() {
        use std::os::unix::ffi::OsStringExt;
        let odd = b"/2;\xffx.jpg".to_vec();
        let mut o = scan();
        o.files[2].path = PathBuf::from(std::ffi::OsString::from_vec(odd.clone()));
        let mut pairs = pairs();
        pairs[1].b = o.files[2].path.display().to_string().into();
        pairs[1].b_bytes = raw_bytes(&o.files[2].path);

        let txt = txt(&o);
        assert!(txt.windows(odd.len()).any(|w| w == odd), "the text names the file itself");

        let csv = csv(&o);
        let quoted = [b"\"".as_slice(), &odd, b"\""].concat();
        assert!(csv.windows(quoted.len()).any(|w| w == quoted), "and so does the CSV, quoted for its ';'");

        let v = json(&o, Some(&pairs));
        let bytes = |x: &serde_json::Value| x.as_array().unwrap().iter().map(|b| b.as_u64().unwrap() as u8).collect::<Vec<u8>>();
        assert_eq!(bytes(&v["groups"][0]["files"][2]["path_bytes"]), odd);
        assert_eq!(bytes(&v["pairs"][1]["b_bytes"]), odd);
        assert!(v["groups"][0]["files"][0].get("path_bytes").is_none(), "a UTF-8 path carries nothing extra");
        assert!(v["pairs"][1].get("a_bytes").is_none());
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

    #[test]
    fn a_quote_in_a_csv_field_is_doubled() {
        let mut buf = Vec::new();
        csv_row(&mut buf, &[b"a\"b;c", b"plain"]).unwrap();
        assert_eq!(buf, b"\"a\"\"b;c\";plain\n");
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
