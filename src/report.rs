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

use crate::against::{self, Answer};
use crate::outcome::{Evidence, File, Member, Outcome, Relation};
use crate::stated::Pair;
use crate::suggest::{Action, Mode};
use crate::table::{self, both, raw_bytes, Cell, Column, Json, Table};

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

/// The stated pairs, as the JSON's `pairs` list writes them: by the run's
/// file numbers, named from `files` as they are written.
///
/// A pair used to be held as its record here, two names and two byte
/// strings beside its evidence, from the end of the analysis to the end of
/// the run, a million of them at a run's peak; and other readers looked
/// their pairs up in it.
#[derive(Clone, Copy)]
pub struct Pairs<'a> {
    pub files: &'a [PathBuf],
    pub list: &'a [Pair],
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

impl<'a> Written<'a> {
    /// `p`, named from `names`: each file's name and, when it is not UTF-8,
    /// its bytes.
    fn of(p: &Pair, names: &'a [(String, Option<Vec<u8>>)]) -> Written<'a> {
        let (a, b) = (&names[p.a as usize], &names[p.b as usize]);
        let r = p.ev.relation;
        Written {
            a: &a.0,
            b: &b.0,
            a_bytes: &a.1,
            b_bytes: &b.1,
            aligned_points: p.ev.aligned_points.unwrap_or(0),
            frame_overlap: p.ev.frame_overlap,
            pixel_correlation: p.ev.pixel_correlation,
            scale: p.scale,
            mirrored: p.ev.mirrored,
            inverted: p.ev.inverted,
            identical: r == Relation::Identical,
            same_pixels: r == Relation::SamePixels,
            propagated: r == Relation::Propagated,
            corroborated: r == Relation::Corroborated,
        }
    }
}

/// The JSON's `pairs`, its last key.
fn write_pairs(w: &mut dyn Write, pairs: Pairs) -> std::io::Result<()> {
    let names: Vec<(String, Option<Vec<u8>>)> = pairs.files.par_iter().map(|f| (f.display().to_string(), raw_bytes(f))).collect();
    list_par(w, "pairs", pairs.list, |p| Written::of(p, &names), true)
}

/// Write the report of `scan` where `target` says, in the layout it says,
/// with `mode`'s suggestions; the JSON lists `pairs` when there are any to
/// list, which only a scan has.
///
/// A reader that goes away early — `img-fp DIR | head` — is how a pipeline
/// ends, not a failed run, so a broken pipe on stdout is not an error.
pub fn write_with(target: &Target, scan: &Outcome, mode: Mode, pairs: Option<Pairs>) -> Result<()> {
    let body = |w: &mut dyn Write| write_layout(w, target.format, scan, mode, pairs);
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

/// The report of `scan` in `format`: its groups, or against a library each
/// scanned file's verdict.
fn write_layout(w: &mut dyn Write, format: Format, scan: &Outcome, mode: Mode, pairs: Option<Pairs>) -> std::io::Result<()> {
    if scan.against.is_some() {
        let answers = against::answers(scan);
        let lines = answer_lines(scan, &answers);
        return match format {
            Format::Json => write_answers_json(w, scan, &lines, pairs),
            Format::Txt => write_answers_txt(w, &lines),
            Format::Csv => write_csv(w, &answer_columns(), &lines),
        };
    }
    let said = Said::of(scan, mode);
    match format {
        Format::Json => write_json(w, &said, pairs),
        Format::Txt => write_txt(w, &said),
        Format::Csv => write_csv(w, &group_columns(), &said.lines().flatten().collect::<Vec<_>>()),
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

    /// Each group's lines, representative first. A file in two groups has a
    /// line in each, because its evidence differs: each is its pair with
    /// *that* group's representative.
    fn lines(&self) -> impl Iterator<Item = Vec<Line<'_>>> + '_ {
        let files = &self.scan.files;
        self.groups.iter().enumerate().map(move |(group, g)| {
            g.iter()
                .map(|m| {
                    let action = self.actions[m.file];
                    let kept = if self.mode == Mode::Content { self.scan.kept_copy(m.file) } else { None };
                    let kept = kept.filter(|_| action == Some(Action::Delete)).map(|k| files[k].path.as_path());
                    Line { group, file: &files[m.file], action, with: m.with, kept }
                })
                .collect()
        })
    }
}

/// One group as the JSON states it: the text report's block, each line an
/// object whose keys are the CSV's columns.
struct JsonGroup<'t, 'a> {
    lines: Vec<Line<'a>>,
    columns: &'t Table<Line<'a>>,
}

impl Serialize for JsonGroup<'_, '_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("group", &group_name(self.lines[0].group))?;
        table::path_entry(&mut m, "representative", &self.lines[0].file.path)?;
        m.serialize_entry("files", &self.columns.rows(&self.lines))?;
        m.end()
    }
}

/// The JSON, one group, pair and failure to a line.
///
/// It is still one JSON document to any parser. Written on a single line it
/// was a megabyte-long line on a found corpus and fifty on the benchmark one,
/// and an editor asked to open that hangs; pretty-printed it would be ten lines
/// a pair. One record a line opens anywhere and still greps.
fn write_json(w: &mut dyn Write, said: &Said, pairs: Option<Pairs>) -> std::io::Result<()> {
    let columns = group_columns();
    let groups: Vec<JsonGroup> = said.lines().map(|lines| JsonGroup { lines, columns: &columns }).collect();
    write_head(w, said.scan, said.mode)?;
    list(w, "groups", &groups, pairs.is_none())?;
    if let Some(pairs) = pairs {
        write_pairs(w, pairs)?;
    }
    writeln!(w, "}}")
}

/// The JSON's opening lines: what the run was and what it read.
fn write_head(w: &mut dyn Write, scan: &Outcome, mode: Mode) -> std::io::Result<()> {
    let head = &scan.head;
    let mut config = head.config.clone();
    config["suggest"] = format!("{mode:?}").to_lowercase().into();
    writeln!(w, "{{")?;
    writeln!(w, "  \"tool\": \"img-fp\",")?;
    writeln!(w, "  \"config\": {},", serde_json::to_string(&config)?)?;
    writeln!(w, "  \"files_enumerated\": {},", head.files_enumerated)?;
    writeln!(w, "  \"files_analysed\": {},", head.files_analysed)?;
    if let Some(a) = &scan.against {
        writeln!(w, "  \"library_files\": {},", a.library_files)?;
    }
    list(w, "failures", &head.failures, false)?;
    writeln!(w, "  \"runtime_seconds\": {},", serde_json::to_string(&head.runtime_seconds)?)
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
pub(crate) fn list_par<T: Sync, S: Serialize>(w: &mut dyn Write, key: &str, xs: &[T], view: impl Fn(&T) -> S + Sync, last: bool) -> std::io::Result<()> {
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
                    serde_json::to_writer(&mut buf, &view(x))?;
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

/// A line that states one pair, and so the columns of what it rests on.
///
/// `None` is a figure that was not measured, and it is an empty cell or a
/// `null` rather than a zero: a representative's evidence (it is what the
/// others were compared with), `aligned_points` on a pair no keypoints vouch
/// for.
trait Paired {
    fn with(&self) -> Option<Evidence>;
}

/// A pair's columns, in every layout that states one.
fn evidence_columns<R: Paired>() -> [Column<R>; 6] {
    [
        both("relation", |r| Cell::of(r.with(), |e| Cell::Text(e.relation.as_str().into()))),
        both("aligned_points", |r| Cell::of(r.with().and_then(|e| e.aligned_points), |n| Cell::Count(n.into()))),
        both("frame_overlap", |r| Cell::of(r.with(), |e| Cell::Figure(e.frame_overlap))),
        both("pixel_correlation", |r| Cell::of(r.with(), |e| Cell::Figure(e.pixel_correlation))),
        both("mirrored", |r| Cell::of(r.with(), |e| Cell::Yes(e.mirrored))),
        both("inverted", |r| Cell::of(r.with(), |e| Cell::Yes(e.inverted))),
    ]
}

/// A file's frame, `None` where the header could not be read.
fn width(f: &File) -> Cell<'static> {
    Cell::of(f.dims, |d| Cell::Count(d.0.into()))
}

fn height(f: &File) -> Cell<'static> {
    Cell::of(f.dims, |d| Cell::Count(d.1.into()))
}

/// One file of one group, as every layout states it: its pair with that
/// group's representative, and what the rule says of it.
struct Line<'a> {
    group: usize,
    file: &'a File,
    /// KEEP, DELETE or REVIEW: one answer for the file in every group it is
    /// in.
    action: Option<Action>,
    with: Option<Evidence>,
    /// For a DELETE, the kept file that shows everything this one does.
    kept: Option<&'a Path>,
}

impl Paired for Line<'_> {
    fn with(&self) -> Option<Evidence> {
        self.with
    }
}

/// The groups' columns. `group` is the CSV's alone, since the JSON holds
/// each line inside its group, and `kept_copy` the JSON's alone: the text
/// and the CSV keep the columns they had.
fn group_columns<'a>() -> Table<Line<'a>> {
    let mut c: Vec<Column<Line>> = vec![
        Column { name: "group", csv: true, json: Json::Never, cell: |r| Cell::Text(group_name(r.group).into()) },
        both("role", |r| Cell::Text(if r.with.is_none() { "representative" } else { "match" }.into())),
        both("action", |r| Cell::of(r.action, |a| Cell::Text(a.as_str().into()))),
        both("path", |r| Cell::Path(&r.file.path)),
        both("width", |r| width(r.file)),
        both("height", |r| height(r.file)),
        both("size", |r| Cell::of(r.file.bytes, |b| Cell::Text(format_size(b).into()))),
        both("size_bytes", |r| Cell::of(r.file.bytes, Cell::Count)),
    ];
    c.extend(evidence_columns());
    c.push(Column { name: "damaged", csv: true, json: Json::Present, cell: |r| Cell::flag(r.file.damaged) });
    c.push(Column { name: "kept_copy", csv: false, json: Json::Present, cell: |r| r.kept.map_or(Cell::None, Cell::Path) });
    Table(c)
}

/// Width of the role column, comma included: `MATCH,`.
const ROLE_COLUMN: usize = 6;
/// Width of the action column, comma included: `DELETE,`.
const ACTION_COLUMN: usize = 7;

/// A frame as the text writes it.
fn dims_text(dims: Option<(u32, u32)>) -> String {
    match dims {
        Some((w, h)) => format!("{w}x{h}"),
        None => "-".into(),
    }
}

/// One block per group, representative first, the layout `vid-fp`'s text
/// report has: the role leads at a fixed width so it forms a column, and the
/// path trails because it is the one field with no bounded length. Every
/// figure carries its name, since there is no header to say which is which.
fn write_txt(w: &mut dyn Write, said: &Said) -> std::io::Result<()> {
    for (gi, g) in said.lines().enumerate() {
        writeln!(w, "{}: {} files", group_name(gi), g.len())?;
        for r in g {
            let role = if r.with.is_none() { "REP," } else { "MATCH," };
            let action = format!("{},", r.action.map_or("-", Action::as_str));
            let size = r.file.bytes.map_or_else(|| "-".into(), format_size);
            let damaged = if r.file.damaged { "DAMAGED, " } else { "" };
            // The path as its own bytes, as `ls` would print it: a name that
            // is not UTF-8 is still the name of the file.
            write!(w, "\t{role:<ROLE_COLUMN$} {action:<ACTION_COLUMN$} {}, {size}, {damaged}{}", dims_text(r.file.dims), evidence(r.with))?;
            w.write_all(r.file.path.as_os_str().as_bytes())?;
            writeln!(w)?;
        }
        writeln!(w)?;
    }
    Ok(())
}

/// What a pair says, as a text row's clause ends it: before the path.
fn evidence(e: Option<Evidence>) -> String {
    let Some(e) = e else { return String::new() };
    let (ov, corr) = (e.frame_overlap, e.pixel_correlation);
    let mut s = match (e.relation, e.aligned_points) {
        (Relation::Identical, _) => "identical".to_string(),
        (Relation::SamePixels, _) => "same pixels".to_string(),
        (Relation::Corroborated, Some(n)) => format!("corroborated, {n} points, overlap {ov:.2}, correlation {corr:.2}"),
        (_, Some(n)) => format!("{n} points, overlap {ov:.2}, correlation {corr:.2}"),
        (how, None) => format!("{}, overlap {ov:.2}, correlation {corr:.2}", how.as_str()),
    };
    if e.mirrored {
        s.push_str(", mirrored");
    }
    if e.inverted {
        s.push_str(", inverted");
    }
    s.push_str(", ");
    s
}

/// The CSV of either layout: its header, then a line a row, `;`-separated
/// as `vid-fp`'s is.
fn write_csv<R>(w: &mut dyn Write, columns: &Table<R>, rows: &[R]) -> std::io::Result<()> {
    columns.csv_header(w, b';')?;
    rows.iter().try_for_each(|r| columns.csv(w, r, b';'))
}

// ---------------------------------------------------------------- --against

/// One scanned file against the library, as every layout states it, with
/// the file its verdict rests on and the file kept at the end of a chain of
/// repeats, and which side each is from.
struct AnswerLine<'a> {
    a: &'a Answer,
    file: &'a File,
    by: Option<(&'a File, &'static str)>,
    kept: Option<(&'a File, &'static str)>,
}

impl Paired for AnswerLine<'_> {
    fn with(&self) -> Option<Evidence> {
        self.a.with
    }
}

/// The verdicts' columns.
fn answer_columns<'a>() -> Table<AnswerLine<'a>> {
    let mut c: Vec<Column<AnswerLine>> = vec![
        both("verdict", |r| Cell::Text(r.a.verdict.as_str().into())),
        both("path", |r| Cell::Path(&r.file.path)),
        both("width", |r| width(r.file)),
        both("height", |r| height(r.file)),
        both("size_bytes", |r| Cell::Count(r.file.bytes.unwrap_or(0))),
        both("match", |r| r.by.map_or(Cell::None, |(g, _)| Cell::Path(&g.path))),
        both("match_in", |r| Cell::of(r.by, |(_, side)| Cell::Text(side.into()))),
        both("match_width", |r| r.by.map_or(Cell::None, |(g, _)| width(g))),
        both("match_height", |r| r.by.map_or(Cell::None, |(g, _)| height(g))),
        both("match_size_bytes", |r| Cell::of(r.by.and_then(|(g, _)| g.bytes), Cell::Count)),
    ];
    c.extend(evidence_columns());
    c.extend([
        both::<AnswerLine>("kept", |r| r.kept.map_or(Cell::None, |(g, _)| Cell::Path(&g.path))),
        both("kept_in", |r| Cell::of(r.kept, |(_, side)| Cell::Text(side.into()))),
        both("library_matches", |r| Cell::Count(r.a.library_matches as u64)),
        Column { name: "damaged", csv: true, json: Json::Present, cell: |r| Cell::flag(r.file.damaged) },
    ]);
    Table(c)
}

/// The lines, in the answers' order.
fn answer_lines<'a>(scan: &'a Outcome, answers: &'a [Answer]) -> Vec<AnswerLine<'a>> {
    let files = &scan.files;
    let side = |g: usize| (&files[g], if files[g].library { "library" } else { "scan" });
    answers.iter().map(|a| AnswerLine { a, file: &files[a.file], by: a.by.map(side), kept: a.kept.map(side) }).collect()
}

/// One block per verdict. Each file is a line, the verdict leading at a fixed
/// width and the path trailing, as in the groups report; the file it rests on
/// follows on a line of its own, with the evidence.
fn write_answers_txt(w: &mut dyn Write, lines: &[AnswerLine]) -> std::io::Result<()> {
    for block in lines.chunk_by(|x, y| x.a.verdict.as_str() == y.a.verdict.as_str()) {
        let v = block[0].a.verdict.as_str();
        writeln!(w, "{v}: {} {}", block.len(), if block.len() == 1 { "file" } else { "files" })?;
        for r in block {
            let damaged = if r.file.damaged { "DAMAGED, " } else { "" };
            let size = format_size(r.file.bytes.unwrap_or(0));
            write!(w, "\t{:<ACTION_COLUMN$} {}, {size}, {damaged}", format!("{},", v.to_uppercase()), dims_text(r.file.dims))?;
            w.write_all(r.file.path.as_os_str().as_bytes())?;
            writeln!(w)?;
            if let Some((by, side)) = r.by {
                let size = by.bytes.map_or_else(|| "-".into(), format_size);
                write!(w, "\t        in {side}: {}, {size}, {}", dims_text(by.dims), evidence(r.a.with))?;
                w.write_all(by.path.as_os_str().as_bytes())?;
                writeln!(w)?;
                if let Some((kept, side)) = r.kept {
                    write!(w, "\t        which repeats, in {side}: ")?;
                    w.write_all(kept.path.as_os_str().as_bytes())?;
                    writeln!(w)?;
                }
            }
        }
        writeln!(w)?;
    }
    Ok(())
}

fn write_answers_json(w: &mut dyn Write, scan: &Outcome, lines: &[AnswerLine], pairs: Option<Pairs>) -> std::io::Result<()> {
    let columns = answer_columns();
    let files: Vec<_> = lines.iter().map(|r| columns.json(r)).collect();
    write_head(w, scan, Mode::Content)?;
    list(w, "files", &files, pairs.is_none())?;
    if let Some(pairs) = pairs {
        write_pairs(w, pairs)?;
    }
    writeln!(w, "}}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{File, Group, Head};

    fn ev(relation: Relation) -> Evidence {
        let points = (!relation.one_file()).then_some(42);
        Evidence { relation, aligned_points: points, frame_overlap: 0.987, pixel_correlation: 0.912, mirrored: false, inverted: false }
    }

    fn pair(a: usize, b: usize) -> Pair {
        Pair { a: a as u32, b: b as u32, ev: ev(Relation::Direct), scale: 0.5, place: None, clean: true }
    }

    fn file(p: &str) -> File {
        File { path: PathBuf::from(p), dims: None, bytes: None, damaged: false, library: false }
    }

    /// One group: `/0.jpg`, which holds an identical `/1.jpg` and a mirrored
    /// `/2;x.jpg`.
    fn scan() -> Outcome {
        let mut files = vec![file("/0.jpg"), file("/1.jpg"), file("/2;x.jpg")];
        (files[0].dims, files[0].bytes) = (Some((4032, 3024)), Some(3_250_000));
        let mirrored = Evidence { mirrored: true, ..ev(Relation::Direct) };
        let members = vec![Member { file: 0, with: None, at: None }, Member { file: 1, with: Some(ev(Relation::Identical)), at: None }, Member { file: 2, with: Some(mirrored), at: None }];
        let head = Head { config: serde_json::json!({}), files_enumerated: 3, files_analysed: 3, failures: vec![], runtime_seconds: 1.0 };
        Outcome::new(head, 0.7, files, vec![Group { members }], vec![Some(Action::Keep), Some(Action::Delete), Some(Action::Review)], vec![None, Some(0), None])
    }

    fn pairs() -> Vec<Pair> {
        let mut mirrored = pair(0, 2);
        mirrored.ev.mirrored = true;
        let mut identical = pair(1, 0);
        identical.ev = ev(Relation::Identical);
        vec![identical, mirrored]
    }

    fn txt(o: &Outcome) -> Vec<u8> {
        let mut buf = Vec::new();
        write_layout(&mut buf, Format::Txt, o, Mode::Content, None).unwrap();
        buf
    }

    fn csv(o: &Outcome) -> Vec<u8> {
        let mut buf = Vec::new();
        write_layout(&mut buf, Format::Csv, o, Mode::Content, None).unwrap();
        buf
    }

    fn json(o: &Outcome, pairs: Option<&[Pair]>) -> serde_json::Value {
        let mut buf = Vec::new();
        let files: Vec<PathBuf> = o.files.iter().map(|f| f.path.clone()).collect();
        let pairs = pairs.map(|list| Pairs { files: &files, list });
        write_layout(&mut buf, Format::Json, o, Mode::Content, pairs).unwrap();
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
        assert_eq!(lines[0], "group;role;action;path;width;height;size;size_bytes;relation;aligned_points;frame_overlap;pixel_correlation;mirrored;inverted;damaged");
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
            let xs: Vec<Pair> = (0..n).map(|i| pair(i, i + 1)).collect();
            let names: Vec<(String, Option<Vec<u8>>)> = (0..=n).map(|i| (format!("/{i}.jpg"), None)).collect();
            let written: Vec<Written> = xs.iter().map(|p| Written::of(p, &names)).collect();
            for last in [false, true] {
                let (mut a, mut b) = (Vec::new(), Vec::new());
                list(&mut a, "pairs", &written, last).unwrap();
                list_par(&mut b, "pairs", &xs, |p| Written::of(p, &names), last).unwrap();
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

    /// And against a library, whose text said only "9 points" while it had
    /// a layout of its own.
    #[test]
    fn a_corroborated_pair_says_so_against_a_library() {
        use crate::outcome::{Against, Asked, Match};
        let corroborated = Evidence { relation: Relation::Corroborated, aligned_points: Some(9), pixel_correlation: 0.97, ..ev(Relation::Direct) };
        let mut files = vec![file("/s.jpg"), file("/l.jpg")];
        files[1].library = true;
        let asked = vec![Asked { file: 0, holder: Some(Member { file: 1, with: Some(corroborated), at: None }), library: vec![Match { file: 1, with: corroborated, clean: true }] }];
        let o = Outcome::of_against(Head::default(), 0.7, files, Against { library_files: 1, asked }, vec![Some(Action::Delete), Some(Action::Keep)], vec![Some(1), None]);
        let text = String::from_utf8(txt(&o)).unwrap();
        assert!(text.contains("\tREPEAT, -, 0B, /s.jpg\n\t        in library: -, -, corroborated, 9 points, overlap 0.99, correlation 0.97, /l.jpg\n"), "{text}");
        assert!(String::from_utf8(csv(&o)).unwrap().contains(";library;;;;corroborated;9;0.987;0.97;false;false;"));
        assert_eq!(json(&o, None)["files"][0]["relation"], "corroborated");
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
        let pairs = pairs();

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
