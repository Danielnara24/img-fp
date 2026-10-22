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

use anyhow::Result;
use rayon::prelude::*;
use serde::Serialize;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::against::{self, Answer};
use crate::outputs::Report;
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

/// Write the report of `scan` where `report` says, in the layout it says,
/// with `mode`'s suggestions; the JSON lists `pairs` when there are any to
/// list, which only a scan has.
pub fn write_with(report: &Report, scan: &Outcome, mode: Mode, pairs: Option<Pairs>) -> Result<()> {
    report.to.write("the results", |w| write_layout(w, report.format, scan, mode, pairs))
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

}
