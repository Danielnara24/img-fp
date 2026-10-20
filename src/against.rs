//! `--against`: which of the images scanned a library already has.
//!
//! The question is asked of each file scanned, not of pairs or groups, and
//! has one of four answers:
//!
//! - **repeat**: a file that stays shows all of it — the library's copy, or,
//!   for a picture the scan holds twice, the other copy in the scan. This is
//!   the content rule's DELETE (`suggest.rs`), with the library pinned as
//!   kept, so it promises what DELETE promises: nothing is lost by leaving
//!   the file out.
//! - **better**: the library has the picture, matched cleanly, but nothing in
//!   it shows all of this file: it is larger, less cropped, or the original
//!   of an edit the library holds.
//! - **review**: matched to the library only weakly, but closely enough to be
//!   a copy (`suggest::REVIEW`): a tint, a watermark, a negative, a mirror
//!   image.
//! - **new**: nothing in the library matches, or nothing closely enough. A
//!   file that would not decode is new unless its bytes are a library file's:
//!   the safe answer for "what do I still have to import".
//!
//! The library's own duplicates are not reported: the run is not asked about
//! them. It finds those the scan reaches, since a chain from a scanned file
//! may run through them, and does not look for the rest.

use serde::Serialize;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::outcome::Evidence;
use crate::report::{self, Format, Sink, Target};
use crate::suggest::{self, Action};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, clap::ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
// The variants' doc comments are the `--help` text of `--list`'s values.
pub enum Verdict {
    /// Already there: the library, or another image scanned, shows all of it
    Repeat,
    /// The library has it, but this one shows more of it
    Better,
    /// A weak match to the library, close enough to be an edited copy
    Review,
    /// Not in the library
    New,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Repeat => "repeat",
            Verdict::Better => "better",
            Verdict::Review => "review",
            Verdict::New => "new",
        }
    }
}

/// One stated pair, as the verdicts read it.
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub ev: Evidence,
    /// No block of the overlap disagrees (`suggest::CLEAN`); the same bytes
    /// or pixels always are.
    pub clean: bool,
}

/// What the run says about one scanned file.
pub struct Answer {
    pub file: usize,
    pub verdict: Verdict,
    /// The file the verdict rests on: for a repeat, the one that shows all of
    /// it; for better and review, the library file it matches best.
    pub by: Option<usize>,
    /// The pair with `by`, as an index into the pairs.
    pub pair: Option<usize>,
    /// For a repeat whose `by` is itself a repeat, the file at the end of
    /// that chain, which is the one kept: deletion spreads from a kept file
    /// one holder at a time, so each link holds the next.
    pub kept: Option<usize>,
    /// How many library files it was matched to at all.
    pub library_matches: usize,
}

/// Every scanned file's answer, in path order. `asked` is the scanned files;
/// `actions` and `keeper` are `suggest::suggest_pinned`'s, with the library
/// pinned.
pub fn answer(asked: &[usize], library: &[bool], pairs: &[Pair], actions: &[Option<Action>], keeper: &[Option<usize>]) -> Vec<Answer> {
    let n = library.len();
    let mut of: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (k, p) in pairs.iter().enumerate() {
        of[p.a].push(k);
        of[p.b].push(k);
    }
    let other = |p: &Pair, f: usize| if p.a == f { p.b } else { p.a };
    asked
        .iter()
        .map(|&f| {
            let to_library: Vec<usize> = of[f].iter().copied().filter(|&k| library[other(&pairs[k], f)]).collect();
            let library_matches = to_library.len();
            if actions[f] == Some(Action::Delete) {
                if let Some(g) = keeper[f] {
                    let pair = of[f].iter().copied().find(|&k| other(&pairs[k], f) == g);
                    let mut end = g;
                    for _ in 0..n {
                        match keeper[end] {
                            Some(next) if actions[end] == Some(Action::Delete) => end = next,
                            _ => break,
                        }
                    }
                    let kept = (end != g).then_some(end);
                    return Answer { file: f, verdict: Verdict::Repeat, by: Some(g), pair, kept, library_matches };
                }
            }
            // The best library match: a clean one first, then the closest.
            // A mirror image, like a negative, is not the picture as the
            // library has it, whatever it shows: at best a copy to look at.
            let clean = |k: usize| pairs[k].clean && !pairs[k].ev.inverted && !pairs[k].ev.mirrored;
            let best = to_library
                .iter()
                .copied()
                .max_by(|&x, &y| clean(x).cmp(&clean(y)).then(pairs[x].ev.pixel_correlation.total_cmp(&pairs[y].ev.pixel_correlation)));
            let verdict = match best {
                Some(k) if clean(k) => Verdict::Better,
                Some(k) if pairs[k].ev.pixel_correlation >= suggest::REVIEW => Verdict::Review,
                _ => Verdict::New,
            };
            let best = best.filter(|_| verdict != Verdict::New);
            Answer { file: f, verdict, by: best.map(|k| other(&pairs[k], f)), pair: best, kept: None, library_matches }
        })
        .collect()
}

/// What the report states about a file beside its path.
pub struct Facts<'a> {
    pub dims: &'a [(u32, u32)],
    pub sizes: &'a [u64],
    pub damaged: &'a [bool],
}

/// One scanned file, as all three layouts state it.
#[derive(Serialize)]
struct Row<'a> {
    verdict: &'static str,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_bytes: Option<Vec<u8>>,
    #[serde(skip)]
    raw: &'a Path,
    width: Option<u32>,
    height: Option<u32>,
    size_bytes: u64,
    /// The file the verdict rests on, and which side it is from.
    #[serde(rename = "match")]
    by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    match_bytes: Option<Vec<u8>>,
    #[serde(skip)]
    by_raw: Option<&'a Path>,
    match_in: Option<&'static str>,
    match_width: Option<u32>,
    match_height: Option<u32>,
    match_size_bytes: Option<u64>,
    relation: Option<&'static str>,
    aligned_points: Option<u32>,
    frame_overlap: Option<f32>,
    pixel_correlation: Option<f32>,
    mirrored: Option<bool>,
    inverted: Option<bool>,
    /// For a repeat of a repeat, the file kept at the end of the chain.
    kept: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    kept_bytes: Option<Vec<u8>>,
    #[serde(skip)]
    kept_raw: Option<&'a Path>,
    kept_in: Option<&'static str>,
    library_matches: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    damaged: bool,
}

fn rows<'a>(answers: &'a [Answer], pairs: &'a [Pair], library: &'a [bool], files: &'a [PathBuf], facts: &Facts) -> Vec<Row<'a>> {
    let dims = |i: usize| facts.dims.get(i).copied().filter(|d| d.0 > 0 && d.1 > 0);
    let mut rows: Vec<Row> = answers
        .iter()
        .map(|a| {
            let p = a.pair.map(|k| &pairs[k].ev);
            Row {
                verdict: a.verdict.as_str(),
                path: files[a.file].display().to_string(),
                path_bytes: report::raw_bytes(&files[a.file]),
                raw: &files[a.file],
                width: dims(a.file).map(|d| d.0),
                height: dims(a.file).map(|d| d.1),
                size_bytes: facts.sizes[a.file],
                by: a.by.map(|g| files[g].display().to_string()),
                match_bytes: a.by.and_then(|g| report::raw_bytes(&files[g])),
                by_raw: a.by.map(|g| files[g].as_path()),
                match_in: a.by.map(|g| if library[g] { "library" } else { "scan" }),
                match_width: a.by.and_then(dims).map(|d| d.0),
                match_height: a.by.and_then(dims).map(|d| d.1),
                match_size_bytes: a.by.map(|g| facts.sizes[g]),
                relation: p.map(|p| p.relation.as_str()),
                aligned_points: p.and_then(|p| p.aligned_points),
                frame_overlap: p.map(|p| p.frame_overlap),
                pixel_correlation: p.map(|p| p.pixel_correlation),
                mirrored: p.map(|p| p.mirrored),
                inverted: p.map(|p| p.inverted),
                kept: a.kept.map(|g| files[g].display().to_string()),
                kept_bytes: a.kept.and_then(|g| report::raw_bytes(&files[g])),
                kept_raw: a.kept.map(|g| files[g].as_path()),
                kept_in: a.kept.map(|g| if library[g] { "library" } else { "scan" }),
                library_matches: a.library_matches,
                damaged: facts.damaged[a.file],
            }
        })
        .collect();
    rows.sort_by(|x, y| verdict_rank(x.verdict).cmp(&verdict_rank(y.verdict)).then_with(|| x.raw.cmp(y.raw)));
    rows
}

fn verdict_rank(v: &str) -> usize {
    ["repeat", "better", "review", "new"].iter().position(|&x| x == v).unwrap_or(4)
}

/// The run's settings, counts and failures, as the JSON states them.
pub struct Header<'a> {
    pub config: serde_json::Value,
    pub files_enumerated: usize,
    pub files_analysed: usize,
    pub library_files: usize,
    pub failures: &'a [serde_json::Value],
    pub runtime_seconds: f64,
}

/// Write the report where `target` says. `out_pairs` are the stated pairs as
/// the JSON lists them, for `score.py`.
#[allow(clippy::too_many_arguments)]
pub fn write(
    target: &Target,
    header: &Header,
    answers: &[Answer],
    pairs: &[Pair],
    out_pairs: &[report::OutPair],
    library: &[bool],
    files: &[PathBuf],
    facts: &Facts,
) -> anyhow::Result<()> {
    use anyhow::Context;
    let rows = rows(answers, pairs, library, files, facts);
    let body = |w: &mut dyn Write| -> std::io::Result<()> {
        match target.format {
            Format::Txt => write_txt(w, &rows),
            Format::Csv => write_csv(w, &rows),
            Format::Json => write_json(w, header, &rows, out_pairs),
        }
    };
    match &target.sink {
        Sink::File(path) => report::write_whole(path, body),
        Sink::Stdout => {
            let mut w = std::io::BufWriter::new(std::io::stdout().lock());
            match body(&mut w).and_then(|()| w.flush()) {
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                other => other.context("could not write the results to stdout"),
            }
        }
    }
}

/// `--list`: the paths with the verdicts asked for, in the report's order,
/// one a line or NUL-separated, on stdout.
pub fn list(answers: &[Answer], files: &[PathBuf], wanted: &[Verdict], nul: bool) -> anyhow::Result<()> {
    let mut chosen: Vec<&Answer> = answers.iter().filter(|a| wanted.contains(&a.verdict)).collect();
    chosen.sort_by(|x, y| x.verdict.cmp(&y.verdict).then_with(|| files[x.file].cmp(&files[y.file])));
    let mut w = std::io::BufWriter::new(std::io::stdout().lock());
    let go = || -> std::io::Result<()> {
        for a in chosen {
            w.write_all(files[a.file].as_os_str().as_bytes())?;
            w.write_all(if nul { b"\0" } else { b"\n" })?;
        }
        w.flush()
    };
    match go() {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

fn dims_text(w: Option<u32>, h: Option<u32>) -> String {
    match (w, h) {
        (Some(w), Some(h)) => format!("{w}x{h}"),
        _ => "-".into(),
    }
}

/// One block per verdict. Each file is a line, the verdict leading at a fixed
/// width and the path trailing, as in the groups report; the file it rests on
/// follows on a line of its own, with the evidence.
fn write_txt(w: &mut dyn Write, rows: &[Row]) -> std::io::Result<()> {
    let mut at = 0;
    while at < rows.len() {
        let v = rows[at].verdict;
        let end = at + rows[at..].iter().take_while(|r| r.verdict == v).count();
        writeln!(w, "{v}: {} {}", end - at, if end - at == 1 { "file" } else { "files" })?;
        for r in &rows[at..end] {
            let damaged = if r.damaged { "DAMAGED, " } else { "" };
            write!(
                w,
                "\t{:<7} {}, {}, {damaged}",
                format!("{},", v.to_uppercase()),
                dims_text(r.width, r.height),
                report::format_size(r.size_bytes)
            )?;
            w.write_all(r.raw.as_os_str().as_bytes())?;
            writeln!(w)?;
            if let (Some(by), Some(side)) = (r.by_raw, r.match_in) {
                let mut ev = match (r.relation, r.aligned_points) {
                    (Some("identical"), _) => "identical".to_string(),
                    (Some("same_pixels"), _) => "same pixels".to_string(),
                    (Some(how), n) => {
                        let pts = n.map(|n| format!("{n} points, ")).unwrap_or_else(|| format!("{how}, "));
                        format!("{pts}overlap {:.2}, correlation {:.2}", r.frame_overlap.unwrap_or(0.0), r.pixel_correlation.unwrap_or(0.0))
                    }
                    (None, _) => String::new(),
                };
                if r.mirrored == Some(true) {
                    ev.push_str(", mirrored");
                }
                if r.inverted == Some(true) {
                    ev.push_str(", inverted");
                }
                let size = r.match_size_bytes.map(report::format_size).unwrap_or_else(|| "-".into());
                write!(w, "\t        in {side}: {}, {size}, {ev}, ", dims_text(r.match_width, r.match_height))?;
                w.write_all(by.as_os_str().as_bytes())?;
                writeln!(w)?;
                if let (Some(kept), Some(side)) = (r.kept_raw, r.kept_in) {
                    write!(w, "\t        which repeats, in {side}: ")?;
                    w.write_all(kept.as_os_str().as_bytes())?;
                    writeln!(w)?;
                }
            }
        }
        writeln!(w)?;
        at = end;
    }
    Ok(())
}

/// The CSV's columns, a `Row`'s fields in its order.
const CSV_HEADER: [&str; 20] = [
    "verdict",
    "path",
    "width",
    "height",
    "size_bytes",
    "match",
    "match_in",
    "match_width",
    "match_height",
    "match_size_bytes",
    "relation",
    "aligned_points",
    "frame_overlap",
    "pixel_correlation",
    "mirrored",
    "inverted",
    "kept",
    "kept_in",
    "library_matches",
    "damaged",
];

fn write_csv(w: &mut dyn Write, rows: &[Row]) -> std::io::Result<()> {
    fn cell<T: ToString>(x: Option<T>) -> String {
        x.map(|x| x.to_string()).unwrap_or_default()
    }
    report::csv_row_with(w, &CSV_HEADER.map(str::as_bytes), b';')?;
    for r in rows {
        let figures = [cell(r.width), cell(r.height), r.size_bytes.to_string()];
        let after = [
            r.match_in.unwrap_or_default().to_string(),
            cell(r.match_width),
            cell(r.match_height),
            cell(r.match_size_bytes),
            r.relation.unwrap_or_default().to_string(),
            cell(r.aligned_points),
            cell(r.frame_overlap),
            cell(r.pixel_correlation),
            cell(r.mirrored),
            cell(r.inverted),
        ];
        let tail = [r.kept_in.unwrap_or_default().to_string(), r.library_matches.to_string(), if r.damaged { "true".into() } else { String::new() }];
        let mut fields: Vec<&[u8]> = vec![r.verdict.as_bytes(), r.raw.as_os_str().as_bytes()];
        fields.extend(figures.iter().map(|f| f.as_bytes()));
        fields.push(r.by_raw.map(|p| p.as_os_str().as_bytes()).unwrap_or_default());
        fields.extend(after.iter().map(|f| f.as_bytes()));
        fields.push(r.kept_raw.map(|p| p.as_os_str().as_bytes()).unwrap_or_default());
        fields.extend(tail.iter().map(|f| f.as_bytes()));
        report::csv_row_with(w, &fields, b';')?;
    }
    Ok(())
}

fn write_json(w: &mut dyn Write, h: &Header, rows: &[Row], out_pairs: &[report::OutPair]) -> std::io::Result<()> {
    writeln!(w, "{{")?;
    writeln!(w, "  \"tool\": \"img-fp\",")?;
    writeln!(w, "  \"config\": {},", serde_json::to_string(&h.config)?)?;
    writeln!(w, "  \"files_enumerated\": {},", h.files_enumerated)?;
    writeln!(w, "  \"files_analysed\": {},", h.files_analysed)?;
    writeln!(w, "  \"library_files\": {},", h.library_files)?;
    report::list(w, "failures", h.failures, false)?;
    writeln!(w, "  \"runtime_seconds\": {},", serde_json::to_string(&h.runtime_seconds)?)?;
    report::list(w, "files", rows, false)?;
    report::list_par(w, "pairs", out_pairs, true)?;
    writeln!(w, "}}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Relation;

    fn pair(a: usize, b: usize, corr: f32, clean: bool) -> Pair {
        let ev = Evidence { relation: Relation::Direct, aligned_points: Some(40), frame_overlap: 1.0, pixel_correlation: corr, mirrored: false, inverted: false };
        Pair { a, b, ev, clean }
    }

    #[test]
    fn each_scanned_file_gets_one_verdict() {
        // 0..4 scanned, 4..7 library.
        let library = [false, false, false, false, false, true, true, true];
        let pairs = vec![
            pair(0, 5, 0.99, true), // deleted by 5: repeat
            pair(1, 6, 0.97, true), // kept: better
            pair(2, 7, 0.93, false), // weak and close: review
            pair(3, 7, 0.70, false), // weak and far: new
            pair(4, 0, 0.99, true), // repeat of a scanned file
        ];
        let d = Some(Action::Delete);
        let actions = [d, Some(Action::Keep), Some(Action::Review), Some(Action::Keep), d, Some(Action::Keep), Some(Action::Keep), Some(Action::Keep)];
        let keeper = [Some(5), None, None, None, Some(0), None, None, None];
        let got: Vec<(Verdict, Option<usize>)> =
            answer(&[0, 1, 2, 3, 4], &library, &pairs, &actions, &keeper).iter().map(|a| (a.verdict, a.by)).collect();
        assert_eq!(
            got,
            vec![
                (Verdict::Repeat, Some(5)),
                (Verdict::Better, Some(6)),
                (Verdict::Review, Some(7)),
                (Verdict::New, None),
                (Verdict::Repeat, Some(0)),
            ]
        );
    }
}
