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
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::decode;

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
    /// `--output` and `--format` read together, in the one place either is.
    pub fn of(output: Option<&Path>, format: Option<Format>) -> Target {
        match output {
            None => Target { sink: Sink::Stdout, format: format.unwrap_or(Format::Txt) },
            Some(p) if p == Path::new("-") => Target { sink: Sink::Stdout, format: format.unwrap_or(Format::Txt) },
            Some(p) => Target { sink: Sink::File(p.to_path_buf()), format: format.unwrap_or_else(|| from_extension(p)) },
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

/// Fails now if `path` could not be created later: it is a directory, its
/// parent is missing, or either refuses writes. Touches nothing that exists,
/// and removes the probe file when there was nothing there before.
pub fn check_writable(path: &Path) -> Result<()> {
    let fail = || format!("cannot write output to {}", path.display());
    if path.is_dir() {
        return Err(anyhow::anyhow!("is a directory; name a file inside it")).with_context(fail);
    }
    if path.exists() {
        std::fs::OpenOptions::new().write(true).open(path).with_context(fail)?;
    } else {
        std::fs::File::create(path).with_context(fail)?;
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct OutPair {
    pub a: String,
    pub b: String,
    pub aligned_points: u32,
    pub frame_overlap: f32,
    pub pixel_correlation: f32,
    pub scale: f32,
    /// A left-right flip, folded into the transform as a negative
    /// determinant — so a composed pair carries it too.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mirrored: bool,
    /// Tones inverted: a negative of the other.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inverted: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub identical: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub propagated: bool,
    /// Indices into the run's file list, for finding a member's pair with its
    /// representative. Not written: `a` and `b` say the same thing.
    #[serde(skip)]
    pub ia: usize,
    #[serde(skip)]
    pub ib: usize,
}

/// One group: the file everything in it was verified against, and the files.
/// `files` includes the representative. The JSON writes each file as a row
/// object rather than a bare path (see `JsonGroup`); `score.py` reads both.
pub struct OutGroup {
    pub representative: String,
    pub files: Vec<String>,
    /// `representative` and `files` as indices into the run's file list.
    pub rep: usize,
    pub members: Vec<usize>,
}

pub struct Output {
    pub tool: &'static str,
    pub config: serde_json::Value,
    pub files_enumerated: usize,
    pub files_analysed: usize,
    pub failures: Vec<serde_json::Value>,
    pub runtime_seconds: f64,
    /// Largest first. Each is a representative and the files that matched it
    /// directly. These overlap: a file that is a duplicate of two files that
    /// are not duplicates of each other appears under both.
    pub groups: Vec<OutGroup>,
    pub pairs: Vec<OutPair>,
}

/// Write the report where `target` says, in the layout it says.
///
/// A reader that goes away early — `img-fp DIR | head` — is how a pipeline
/// ends, not a failed run, so a broken pipe on stdout is not an error.
pub fn write(target: &Target, out: &Output, files: &[PathBuf]) -> Result<()> {
    let facts = facts(out, files);
    let body = |w: &mut dyn Write| -> std::io::Result<()> {
        match target.format {
            Format::Json => write_json(w, out, &facts),
            Format::Txt => write_txt(w, out, &facts),
            Format::Csv => write_csv(w, out, &facts),
        }
    };
    match &target.sink {
        Sink::File(path) => {
            // Fatal, and the context is the whole of what makes exit 1 useful:
            // this is the run's output, there is nowhere else it went, and
            // "No such file or directory" on its own does not say which file.
            let ctx = || format!("could not write {}", path.display());
            let f = std::fs::File::create(path).with_context(ctx)?;
            let mut w = std::io::BufWriter::new(f);
            body(&mut w).and_then(|()| w.flush()).with_context(ctx)
        }
        Sink::Stdout => {
            let mut w = std::io::BufWriter::new(std::io::stdout().lock());
            match body(&mut w).and_then(|()| w.flush()) {
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                other => other.context("could not write the results to stdout"),
            }
        }
    }
}

/// One group as the JSON states it: the text report's block, with each row
/// an object whose keys are the CSV's columns.
#[derive(Serialize)]
struct JsonGroup<'a> {
    group: String,
    representative: &'a str,
    files: Vec<Row<'a>>,
}

/// The JSON, one group, pair and failure to a line.
///
/// It is still one JSON document to any parser. Written on a single line it
/// was a megabyte-long line on a found corpus and fifty on the benchmark one,
/// and an editor asked to open that hangs; pretty-printed it would be ten lines
/// a pair. One record a line opens anywhere and still greps.
fn write_json(w: &mut dyn Write, out: &Output, facts: &HashMap<usize, Facts>) -> std::io::Result<()> {
    fn list<T: Serialize>(w: &mut dyn Write, key: &str, xs: &[T], last: bool) -> std::io::Result<()> {
        write!(w, "  \"{key}\": [")?;
        for (i, x) in xs.iter().enumerate() {
            write!(w, "{}\n    ", if i == 0 { "" } else { "," })?;
            serde_json::to_writer(&mut *w, x)?;
        }
        writeln!(w, "{}]{}", if xs.is_empty() { "" } else { "\n  " }, if last { "" } else { "," })
    }
    let pairs = pair_index(out);
    let groups: Vec<JsonGroup> = out
        .groups
        .iter()
        .enumerate()
        .map(|(gi, g)| JsonGroup {
            group: group_name(gi),
            representative: &g.representative,
            files: rows(g, &pairs, facts).collect(),
        })
        .collect();
    writeln!(w, "{{")?;
    writeln!(w, "  \"tool\": {},", serde_json::to_string(out.tool)?)?;
    writeln!(w, "  \"config\": {},", serde_json::to_string(&out.config)?)?;
    writeln!(w, "  \"files_enumerated\": {},", out.files_enumerated)?;
    writeln!(w, "  \"files_analysed\": {},", out.files_analysed)?;
    list(w, "failures", &out.failures, false)?;
    writeln!(w, "  \"runtime_seconds\": {},", serde_json::to_string(&out.runtime_seconds)?)?;
    list(w, "groups", &groups, false)?;
    list(w, "pairs", &out.pairs, true)?;
    writeln!(w, "}}")
}

/// What the per-file rows say about a file beyond its path. Read from the
/// file's header and its metadata at report time, for the grouped files only:
/// the analysis keeps neither, since the picture it describes is the working
/// image and not the file.
///
/// It costs 0.21 s of wall clock on the benchmark corpus (5,372 files probed,
/// 1.7 thread-seconds, the same whether the analysis was cached or not, and
/// 0.05 s with the headers in the page cache) and 0.08-0.10 s on the found one
/// (2,139 files). Against a 77 s and a 126 s cold run that is 0.3% and 0.1%.
#[derive(Clone, Copy, Default)]
struct Facts {
    dims: Option<(u32, u32)>,
    bytes: Option<u64>,
}

fn facts(out: &Output, files: &[PathBuf]) -> HashMap<usize, Facts> {
    let mut wanted: Vec<usize> = out.groups.iter().flat_map(|g| g.members.iter().copied()).collect();
    wanted.sort_unstable();
    wanted.dedup();
    wanted
        .into_par_iter()
        .map(|i| {
            let p = &files[i];
            let dims = decode::probe(p).map(|pr| (pr.w, pr.h));
            let bytes = std::fs::metadata(p).ok().map(|m| m.len());
            (i, Facts { dims, bytes })
        })
        .collect()
}

/// How a member came to be in its group: the pair it has with the
/// representative, from whichever side it was recorded.
fn pair_index(out: &Output) -> HashMap<(usize, usize), &OutPair> {
    out.pairs.iter().map(|p| ((p.ia.min(p.ib), p.ia.max(p.ib)), p)).collect()
}

/// `identical` for the same bytes, `propagated` for a transform composed along
/// a path and then checked against the pixels, `direct` for one fitted to
/// keypoints the two files share.
fn relation(p: &OutPair) -> &'static str {
    if p.identical {
        "identical"
    } else if p.propagated {
        "propagated"
    } else {
        "direct"
    }
}

fn format_size(bytes: u64) -> String {
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
    path: &'a str,
    role: &'static str,
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
}

/// A group's rows, representative first. A file in two groups has a row in
/// each, because its evidence differs: each row is its pair with *that*
/// group's representative.
fn rows<'a>(
    g: &'a OutGroup,
    pairs: &'a HashMap<(usize, usize), &'a OutPair>,
    facts: &'a HashMap<usize, Facts>,
) -> impl Iterator<Item = Row<'a>> + 'a {
    let rep = g.members.iter().position(|&m| m == g.rep).expect("a representative is a member of its group");
    std::iter::once(rep).chain((0..g.members.len()).filter(move |&k| k != rep)).map(move |k| {
        let i = g.members[k];
        let f = facts.get(&i).copied().unwrap_or_default();
        let p = if i == g.rep { None } else { pairs.get(&(i.min(g.rep), i.max(g.rep))).copied() };
        Row {
            path: &g.files[k],
            role: if i == g.rep { "representative" } else { "match" },
            width: f.dims.map(|d| d.0),
            height: f.dims.map(|d| d.1),
            size: f.bytes.map(format_size),
            size_bytes: f.bytes,
            relation: p.map(relation),
            aligned_points: p.filter(|p| relation(p) == "direct").map(|p| p.aligned_points),
            frame_overlap: p.map(|p| p.frame_overlap),
            pixel_correlation: p.map(|p| p.pixel_correlation),
            mirrored: p.map(|p| p.mirrored),
            inverted: p.map(|p| p.inverted),
        }
    })
}

/// Width of the role column, comma included: `MATCH,`.
const ROLE_COLUMN: usize = 6;

/// One block per group, representative first, the layout `vid-fp`'s text
/// report has: the role leads at a fixed width so it forms a column, and the
/// path trails because it is the one field with no bounded length. Every
/// figure carries its name, since there is no header to say which is which.
fn write_txt(w: &mut dyn Write, out: &Output, facts: &HashMap<usize, Facts>) -> std::io::Result<()> {
    let pairs = pair_index(out);
    for (gi, g) in out.groups.iter().enumerate() {
        writeln!(w, "{}: {} files", group_name(gi), g.members.len())?;
        for r in rows(g, &pairs, facts) {
            let role = if r.role == "representative" { "REP," } else { "MATCH," };
            let dims = match (r.width, r.height) {
                (Some(w), Some(h)) => format!("{w}x{h}"),
                _ => "-".into(),
            };
            let size = r.size.as_deref().unwrap_or("-");
            writeln!(w, "\t{role:<ROLE_COLUMN$} {dims}, {size}, {}{}", evidence(&r), r.path)?;
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
const CSV_HEADER: [&str; 13] = [
    "group",
    "role",
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
];

/// One row per file per group, `;`-separated as `vid-fp`'s is, an unmeasured
/// figure an empty cell.
fn write_csv(w: &mut dyn Write, out: &Output, facts: &HashMap<usize, Facts>) -> std::io::Result<()> {
    fn cell<T: ToString>(x: Option<T>) -> String {
        x.map(|x| x.to_string()).unwrap_or_default()
    }
    let pairs = pair_index(out);
    csv_row(w, &CSV_HEADER)?;
    for (gi, g) in out.groups.iter().enumerate() {
        let group = group_name(gi);
        for r in rows(g, &pairs, facts) {
            csv_row(
                w,
                &[
                    &group,
                    r.role,
                    r.path,
                    &cell(r.width),
                    &cell(r.height),
                    r.size.as_deref().unwrap_or_default(),
                    &cell(r.size_bytes),
                    r.relation.unwrap_or_default(),
                    &cell(r.aligned_points),
                    &cell(r.frame_overlap),
                    &cell(r.pixel_correlation),
                    &cell(r.mirrored),
                    &cell(r.inverted),
                ],
            )?;
        }
    }
    Ok(())
}

/// A field is quoted when it holds the separator, a quote or a line break,
/// with its quotes doubled — RFC 4180's rule, which is what every spreadsheet
/// reads. Only a path can need it.
fn csv_row(w: &mut dyn Write, fields: &[&str]) -> std::io::Result<()> {
    for (k, f) in fields.iter().enumerate() {
        if k > 0 {
            w.write_all(b";")?;
        }
        if f.contains([';', '"', '\n', '\r']) {
            write!(w, "\"{}\"", f.replace('"', "\"\""))?;
        } else {
            w.write_all(f.as_bytes())?;
        }
    }
    w.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(ia: usize, ib: usize) -> OutPair {
        OutPair {
            a: format!("/{ia}.jpg"),
            b: format!("/{ib}.jpg"),
            aligned_points: 42,
            frame_overlap: 0.987,
            pixel_correlation: 0.912,
            scale: 0.5,
            mirrored: false,
            inverted: false,
            identical: false,
            propagated: false,
            ia,
            ib,
        }
    }

    fn output() -> Output {
        let mut mirrored = pair(0, 2);
        mirrored.mirrored = true;
        let mut identical = pair(1, 0);
        identical.identical = true;
        Output {
            tool: "img-fp",
            config: serde_json::json!({}),
            files_enumerated: 3,
            files_analysed: 3,
            failures: vec![],
            runtime_seconds: 1.0,
            groups: vec![OutGroup {
                representative: "/0.jpg".into(),
                files: vec!["/0.jpg".into(), "/1.jpg".into(), "/2;x.jpg".into()],
                rep: 0,
                members: vec![0, 1, 2],
            }],
            pairs: vec![identical, mirrored],
        }
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
        let out = output();
        let facts: HashMap<usize, Facts> =
            [(0, Facts { dims: Some((4032, 3024)), bytes: Some(3_250_000) })].into_iter().collect();
        let mut buf = Vec::new();
        write_txt(&mut buf, &out, &facts).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "group_1: 3 files\n\
             \tREP,   4032x3024, 3.1MB, /0.jpg\n\
             \tMATCH, -, -, identical, /1.jpg\n\
             \tMATCH, -, -, 42 points, overlap 0.99, correlation 0.91, mirrored, /2;x.jpg\n\n"
        );
    }

    #[test]
    fn a_csv_row_quotes_only_what_needs_it() {
        let out = output();
        let mut buf = Vec::new();
        write_csv(&mut buf, &out, &HashMap::new()).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], CSV_HEADER.join(";"));
        assert_eq!(lines[1], "group_1;representative;/0.jpg;;;;;;;;;;");
        assert_eq!(lines[2], "group_1;match;/1.jpg;;;;;identical;;0.987;0.912;false;false");
        assert_eq!(lines[3], "group_1;match;\"/2;x.jpg\";;;;;direct;42;0.987;0.912;true;false");
    }

    #[test]
    fn the_json_is_one_document() {
        let out = output();
        let mut buf = Vec::new();
        let facts: HashMap<usize, Facts> =
            [(0, Facts { dims: Some((4032, 3024)), bytes: Some(3_250_000) })].into_iter().collect();
        write_json(&mut buf, &out, &facts).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        let g = &v["groups"][0];
        assert_eq!(g["group"], "group_1");
        assert_eq!(g["representative"], "/0.jpg");
        assert_eq!(
            g["files"][0],
            serde_json::json!({"path": "/0.jpg", "role": "representative", "width": 4032, "height": 3024,
                "size": "3.1MB", "size_bytes": 3_250_000, "relation": null, "aligned_points": null,
                "frame_overlap": null, "pixel_correlation": null, "mirrored": null, "inverted": null})
        );
        assert_eq!(g["files"][1]["relation"], "identical");
        assert_eq!(g["files"][1]["aligned_points"], serde_json::Value::Null);
        assert_eq!(g["files"][2]["path"], "/2;x.jpg");
        assert_eq!(g["files"][2]["aligned_points"], 42);
        assert_eq!(g["files"][2]["mirrored"], true);
        assert_eq!(v["pairs"][1]["mirrored"], true);
        assert!(v["pairs"][0].get("mirrored").is_none());
        assert!(v["pairs"][0].get("ia").is_none());
    }
}
