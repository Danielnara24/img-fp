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

use crate::outcome::{Asked, Evidence, Match, Member, Outcome};
use crate::stated::Pair;
use crate::suggest::{self, Action, Lapse, Mode};

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

/// What the run says about one scanned file, as things stand.
pub struct Answer {
    pub file: usize,
    pub verdict: Verdict,
    /// The file the verdict rests on: for a repeat, the one that shows all of
    /// it; for better and review, the library file it matches best.
    pub by: Option<usize>,
    /// Their pair.
    pub with: Option<Evidence>,
    /// For a repeat whose `by` is itself a repeat, the file at the end of
    /// that chain, which is the one kept: deletion spreads from a kept file
    /// one holder at a time, so each link holds the next.
    pub kept: Option<usize>,
    /// How many library files it was matched to at all.
    pub library_matches: usize,
}

/// Each scanned file's pairs, as its verdict will read them. `asked` are
/// the scanned files, `pairs` the stated ones, and `actions` and `keeper`
/// `suggest::suggest_pinned`'s with the library pinned; all by the run's
/// numbers.
pub fn asked(asked: &[usize], library: &[bool], pairs: &[Pair], actions: &[Option<Action>], keeper: &[Option<usize>]) -> Vec<Asked> {
    let mut of: Vec<Vec<&Pair>> = vec![Vec::new(); library.len()];
    for p in pairs {
        of[p.a as usize].push(p);
        of[p.b as usize].push(p);
    }
    asked
        .iter()
        .map(|&f| {
            let holder = keeper[f].filter(|_| actions[f] == Some(Action::Delete)).map(|g| Member {
                file: g,
                with: of[f].iter().find(|p| p.other(f) == g).map(|p| p.ev),
                at: None,
            });
            let library = of[f].iter().filter(|p| library[p.other(f)]).map(|p| Match { file: p.other(f), with: p.ev, clean: p.clean }).collect();
            Asked { file: f, holder, library }
        })
        .collect()
}

/// Every scanned file's answer, as things stand, in the report's order:
/// by verdict, then by path. A file gone since the scan has none, and a
/// file gone is not one the library has. A repeat rests on the file that
/// holds it and on the file kept at the end of its chain: if either has gone
/// or changed, the file is answered by the library alone (`Outcome::lapse`).
pub fn answers(scan: &Outcome) -> Vec<Answer> {
    let Some(against) = &scan.against else { return Vec::new() };
    let content = scan.actions(Mode::Content);
    let there = |i: usize| scan.lapsed(i) != Some(Lapse::Gone);
    let mut answers: Vec<Answer> = against
        .asked
        .iter()
        .filter(|a| there(a.file))
        .map(|a| {
            let f = a.file;
            let library: Vec<&Match> = a.library.iter().filter(|m| there(m.file)).collect();
            let library_matches = library.len();
            let holder = a.holder.filter(|h| content[f] == Some(Action::Delete) && scan.lapsed(h.file).is_none());
            if let Some(h) = holder {
                let kept = scan.kept_copy(f).filter(|&k| k != h.file);
                return Answer { file: f, verdict: Verdict::Repeat, by: Some(h.file), with: h.with, kept, library_matches };
            }
            // The best library match: a clean one first, then the closest.
            // A mirror image, like a negative, is not the picture as the
            // library has it, whatever it shows: at best a copy to look at.
            let clean = |m: &Match| m.clean && !m.with.inverted && !m.with.mirrored;
            let best = library
                .into_iter()
                .max_by(|x, y| clean(x).cmp(&clean(y)).then(x.with.pixel_correlation.total_cmp(&y.with.pixel_correlation)));
            let verdict = match best {
                Some(m) if clean(m) => Verdict::Better,
                Some(m) if m.with.pixel_correlation >= suggest::REVIEW => Verdict::Review,
                _ => Verdict::New,
            };
            let best = best.filter(|_| verdict != Verdict::New);
            Answer { file: f, verdict, by: best.map(|m| m.file), with: best.map(|m| m.with), kept: None, library_matches }
        })
        .collect();
    answers.sort_by(|x, y| x.verdict.cmp(&y.verdict).then_with(|| scan.files[x.file].path.cmp(&scan.files[y.file].path)));
    answers
}

/// How many of each verdict, as the summary line says it.
pub fn tally(answers: &[Answer]) -> String {
    let n = |v: Verdict| answers.iter().filter(|a| a.verdict == v).count();
    format!("{} repeat, {} better, {} to review, {} new", n(Verdict::Repeat), n(Verdict::Better), n(Verdict::Review), n(Verdict::New))
}

/// `--list`: the paths with the verdicts asked for, in the report's order,
/// one a line or NUL-separated, on stdout.
pub fn list(scan: &Outcome, answers: &[Answer], wanted: &[Verdict], nul: bool) -> anyhow::Result<()> {
    let mut w = std::io::BufWriter::new(std::io::stdout().lock());
    let mut go = || -> std::io::Result<()> {
        for a in answers.iter().filter(|a| wanted.contains(&a.verdict)) {
            w.write_all(scan.files[a.file].path.as_os_str().as_bytes())?;
            w.write_all(if nul { b"\0" } else { b"\n" })?;
        }
        w.flush()
    };
    match go() {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        other => Ok(other?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{Against, File, Head, Relation};
    use std::path::PathBuf;

    fn pair(a: usize, b: usize, corr: f32, clean: bool) -> Pair {
        let ev = Evidence { relation: Relation::Direct, aligned_points: Some(40), frame_overlap: 1.0, pixel_correlation: corr, mirrored: false, inverted: false };
        Pair { a: a as u32, b: b as u32, ev, scale: 1.0, place: None, clean }
    }

    /// A scan against `library`, its files numbered as the run's.
    fn scan(library: &[bool], pairs: &[Pair], actions: &[Option<Action>], keeper: &[Option<usize>]) -> Outcome {
        let n = library.len();
        let scanned: Vec<usize> = (0..n).filter(|&i| !library[i]).collect();
        let files = (0..n).map(|i| File { path: PathBuf::from(format!("/p/{i}.jpg")), dims: None, bytes: Some(1), damaged: false, library: library[i] }).collect();
        let against = Against { library_files: n - scanned.len(), asked: asked(&scanned, library, pairs, actions, keeper) };
        Outcome::of_against(Head::default(), 0.7, files, against, actions.to_vec(), suggest::kept_at_end(actions, keeper))
    }

    fn said(scan: &Outcome) -> Vec<(usize, Verdict, Option<usize>, Option<usize>)> {
        let mut v: Vec<_> = answers(scan).iter().map(|a| (a.file, a.verdict, a.by, a.kept)).collect();
        v.sort();
        v
    }

    // 0..5 scanned, 5..8 library.
    const LIBRARY: [bool; 8] = [false, false, false, false, false, true, true, true];
    const K: Option<Action> = Some(Action::Keep);
    const D: Option<Action> = Some(Action::Delete);

    fn example() -> Outcome {
        let pairs = vec![
            pair(0, 5, 0.99, true),  // deleted by 5: repeat
            pair(1, 6, 0.97, true),  // kept: better
            pair(2, 7, 0.93, false), // weak and close: review
            pair(3, 7, 0.70, false), // weak and far: new
            pair(4, 0, 0.99, true),  // repeat of a scanned file
            pair(4, 6, 0.95, true),  // and clean to the library
        ];
        let actions = [D, K, Some(Action::Review), K, D, K, K, K];
        let keeper = [Some(5), None, None, None, Some(0), None, None, None];
        scan(&LIBRARY, &pairs, &actions, &keeper)
    }

    #[test]
    fn each_scanned_file_gets_one_verdict() {
        assert_eq!(
            said(&example()),
            vec![
                (0, Verdict::Repeat, Some(5), None),
                (1, Verdict::Better, Some(6), None),
                (2, Verdict::Review, Some(7), None),
                (3, Verdict::New, None, None),
                (4, Verdict::Repeat, Some(0), Some(5)),
            ]
        );
    }

    /// Read back by `--last`: a file gone is not answered, nor is it one the
    /// library has; a repeat resting on a file gone or changed, as the one
    /// that holds it or the one kept, is answered by the library alone.
    #[test]
    fn a_repeat_rests_on_its_holder_and_on_the_file_kept() {
        let mut s = example();
        s.lapse([0], Lapse::Changed);
        assert_eq!(
            said(&s),
            vec![
                (0, Verdict::Better, Some(5), None),
                (1, Verdict::Better, Some(6), None),
                (2, Verdict::Review, Some(7), None),
                (3, Verdict::New, None, None),
                (4, Verdict::Better, Some(6), None),
            ]
        );
        let mut s = example();
        s.lapse([5, 3], Lapse::Gone);
        let got = said(&s);
        assert_eq!(got[0], (0, Verdict::New, None, None), "its only library match is gone");
        assert!(got.iter().all(|a| a.0 != 3), "a file gone is not answered");
        assert_eq!(got.last(), Some(&(4, Verdict::Better, Some(6), None)), "the file kept is gone");
    }

    /// A chain of holders that loops names no kept file.
    #[test]
    fn a_chain_that_loops_names_no_kept_file() {
        let library = [false, false, false, true];
        let pairs = vec![pair(0, 1, 0.99, true), pair(1, 2, 0.99, true), pair(2, 1, 0.99, true)];
        let s = scan(&library, &pairs, &[D, D, D, K], &[Some(1), Some(2), Some(1), None]);
        assert!(answers(&s).iter().all(|a| a.kept.is_none()));
    }
}
