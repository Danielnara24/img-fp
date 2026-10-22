//! A finished scan, as everything after it reads it: the report, `--last`
//! and the window.
//!
//! **One shape, by number, and the rules on it are asked here.** The groups
//! are files by number, each member with its pair's evidence; the content
//! rule's suggestions come with them, since nothing after the scan could work
//! them out again. What the other two rules suggest (`actions`), and what a
//! file gone or changed since the scan takes back (`lapse`), are methods,
//! asked by whoever holds the scan.
//!
//! It used to be the JSON report, read back: `--last` and the window each
//! parsed the rows into a model of their own and each applied the group rules
//! and the gone and changed files in its own way, so the same rule was
//! written twice and the two drifted (`--last` fell back to the correlation
//! bar of 0.6 after the default had moved to 0.7), and keeping a scan meant
//! swapping the content rule's suggestions into the run's report and back.
//!
//! A run against a library is one too: its scanned files, each with its
//! pairs with the library (`Against`), in place of groups, and the verdicts
//! read from them as things stand (`against::answers`). It used to end in a
//! report of its own, its rows, layouts and holder chain written a second
//! time beside the groups', and the text had already dropped "corroborated";
//! and it could not be kept, so `--last` had nothing to write.
//!
//! A scan is kept (`save`) as this, not as a report: `--last` and the window
//! open it with `load`. Nothing else reads the file, and a file of another
//! `FORMAT` is not read: run a scan.

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::suggest::{self, Action, Lapse, Mode, Seat};

/// The kept file's shape. A file of another is not read.
const FORMAT: u32 = 1;

/// How a member came to be in its group: its pair with the representative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// The same bytes.
    Identical,
    /// Not the same bytes, the same picture; see `same::Kind`.
    SamePixels,
    /// A transform fitted to keypoints the two share.
    Direct,
    /// A transform composed along a path, then checked against the pixels.
    Propagated,
    /// A direct match that cleared only the lower bar a pair inside an
    /// existing cluster faces.
    Corroborated,
}

impl Relation {
    pub fn as_str(self) -> &'static str {
        match self {
            Relation::Identical => "identical",
            Relation::SamePixels => "same_pixels",
            Relation::Direct => "direct",
            Relation::Propagated => "propagated",
            Relation::Corroborated => "corroborated",
        }
    }

    /// The two are one file: the same bytes or the same pixels.
    pub fn one_file(self) -> bool {
        matches!(self, Relation::Identical | Relation::SamePixels)
    }
}

/// What one stated pair says.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub relation: Relation,
    /// `None` for a pair no keypoint was matched for: the same file, or a
    /// composed transform.
    pub aligned_points: Option<u32>,
    /// How much of one lies inside the other, the larger of the two ways.
    pub frame_overlap: f32,
    pub pixel_correlation: f32,
    /// A left-right flip.
    pub mirrored: bool,
    /// Tones inverted: a negative of the other.
    pub inverted: bool,
}

/// Where one picture lies in another: the affine map `[a, b, c, d, e, f]`
/// taking a point (x, y) of the first, as fractions of its width and height
/// as shown, to (a x + b y + c, d x + e y + f) in the second's. The window
/// draws from it what the two share; it is the verdict's transform, which is
/// fitted between working images, put in each picture's own frame.
///
/// Kept to four decimals, a tenth of a pixel on a 1,000-pixel side, since
/// it is drawn, not checked: written in full, a member's six numbers were
/// twice the length of everything else it says.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place(pub [f32; 6]);

impl Place {
    /// The same map: two pictures that are one picture.
    pub const SAME: Place = Place([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);

    /// A transform between working images `a` (`aw` x `ah`) and `b`.
    pub fn of(m: &[f32; 6], (aw, ah): (u32, u32), (bw, bh): (u32, u32)) -> Option<Place> {
        let (aw, ah, bw, bh) = (aw as f32, ah as f32, bw as f32, bh as f32);
        if aw <= 0.0 || ah <= 0.0 || bw <= 0.0 || bh <= 0.0 {
            return None;
        }
        let n = [m[0] * aw / bw, m[1] * ah / bw, m[2] / bw, m[3] * aw / bh, m[4] * ah / bh, m[5] / bh];
        Place::kept(n)
    }

    /// A verdict's transform from `a`'s working image to `b`'s, or the same
    /// map for two that are one picture (`None`).
    pub(crate) fn between(m: Option<&[f32; 6]>, a: &crate::Item, b: &crate::Item) -> Option<Place> {
        match m {
            Some(m) => Place::of(m, (a.feats.w, a.feats.h), (b.feats.w, b.feats.h)),
            None => Some(Place::SAME),
        }
    }

    /// The map back, from the second picture to the first.
    pub fn reversed(&self) -> Option<Place> {
        Place::kept(crate::verify::invert_affine(&self.0)?)
    }

    fn kept(m: [f32; 6]) -> Option<Place> {
        m.iter().all(|v| v.is_finite()).then(|| Place(m.map(|v| (v * 1e4).round() / 1e4)))
    }

    /// Where a point of the first picture lands in the second.
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let m = &self.0;
        (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
    }
}

impl Evidence {
    /// How well the two agree, as the group rules read it: 1 for one file.
    pub fn agreement(&self) -> f32 {
        if self.relation.one_file() {
            1.0
        } else {
            self.pixel_correlation
        }
    }
}

/// One file the groups name, and what the rows say of it beside its path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct File {
    #[serde(with = "crate::last::path")]
    pub path: PathBuf,
    /// Its size as shown, orientation applied; `None` for a header nothing
    /// could read.
    pub dims: Option<(u32, u32)>,
    /// Its length, as the walk found it.
    pub bytes: Option<u64>,
    /// Decoded only in part; see `problems::Note::Damaged`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub damaged: bool,
    /// One of `--against`'s library, not one of the files scanned.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
}

/// One file of a group, and its pair with the representative: `None` for
/// the representative itself.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Member {
    pub file: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with: Option<Evidence>,
    /// Where the representative lies in this file, for the window to show
    /// what the two share. A group's alone: a holder or a library match
    /// has none, nor a scan kept before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Place>,
}

/// A representative, first, and the files that matched it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Group {
    pub members: Vec<Member>,
}

impl Group {
    pub fn rep(&self) -> usize {
        self.members[0].file
    }
}

/// A file scanned against a library, and the pairs its verdict is read from
/// (`against::answers`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asked {
    pub file: usize,
    /// For a file the content rule deletes, the file nearest it in its chain
    /// of holders, and their pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<Member>,
    /// Its pairs with the library's files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub library: Vec<Match>,
}

/// One pair with a library file.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Match {
    pub file: usize,
    pub with: Evidence,
    /// No block of the overlap disagrees (`suggest::CLEAN`); one file always.
    pub clean: bool,
}

/// What a run against a library found: each file it scanned, in place of
/// groups.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Against {
    /// How many library files the run read.
    pub library_files: usize,
    pub asked: Vec<Asked>,
}

/// What the report states above the groups.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Head {
    pub config: serde_json::Value,
    pub files_enumerated: usize,
    pub files_analysed: usize,
    pub failures: Vec<serde_json::Value>,
    pub runtime_seconds: f64,
}

/// A finished scan: its groups, by number, and the content rule's
/// suggestions; and, once it is read back, what has become of its files
/// since.
#[derive(Default, Deserialize)]
pub struct Outcome {
    format: u32,
    pub head: Head,
    /// The correlation bar the scan ran at, which the correlation rule cuts
    /// from.
    pub min_correlation: f32,
    pub files: Vec<File>,
    /// Largest first. They overlap: a file matched by two representatives
    /// that did not match each other is in both.
    pub groups: Vec<Group>,
    /// A run against a library: its scanned files, and no groups.
    #[serde(default)]
    pub against: Option<Against>,
    /// The content rule's suggestion for each file, and for each it deletes
    /// the kept file at the end of its chain of holders
    /// (`suggest::kept_at_end`). Empty when the run did not work it out.
    content: Vec<Option<Action>>,
    kept: Vec<Option<usize>>,
    /// Each file gone or changed since the scan (`lapse`).
    #[serde(skip)]
    lapsed: Vec<Option<Lapse>>,
    /// Each deletion the content rule took back because its kept copy
    /// lapsed, and why it did.
    #[serde(skip)]
    unheld: Vec<Option<Lapse>>,
}

/// What `save` writes: an `Outcome` with every path absolute.
#[derive(Serialize)]
struct Saved<'a> {
    format: u32,
    head: &'a Head,
    min_correlation: f32,
    files: Vec<File>,
    groups: &'a [Group],
    #[serde(skip_serializing_if = "Option::is_none")]
    against: &'a Option<Against>,
    content: &'a [Option<Action>],
    kept: &'a [Option<usize>],
}

impl Outcome {
    /// `content` and `kept` are by file, or empty when the content rule was
    /// not worked out.
    pub fn new(head: Head, min_correlation: f32, files: Vec<File>, groups: Vec<Group>, content: Vec<Option<Action>>, kept: Vec<Option<usize>>) -> Outcome {
        Outcome { format: FORMAT, head, min_correlation, files, groups, against: None, content, kept, lapsed: Vec::new(), unheld: Vec::new() }
    }

    /// A run against a library: `against` in place of groups, and the
    /// content rule's suggestions, which its verdicts are read from.
    pub fn of_against(head: Head, min_correlation: f32, files: Vec<File>, against: Against, content: Vec<Option<Action>>, kept: Vec<Option<usize>>) -> Outcome {
        Outcome { against: Some(against), ..Outcome::new(head, min_correlation, files, Vec::new(), content, kept) }
    }

    /// Whether the content rule's suggestions are here to be asked.
    pub fn has_content(&self) -> bool {
        !self.content.is_empty()
    }

    /// Why file `i` is not the file the scan judged, if it is not.
    pub fn lapsed(&self, i: usize) -> Option<Lapse> {
        self.lapsed.get(i).copied().flatten()
    }

    /// Why the content rule no longer deletes file `i`, if it did and its
    /// kept copy has lapsed.
    pub fn unheld(&self, i: usize) -> Option<Lapse> {
        self.unheld.get(i).copied().flatten()
    }

    /// The kept file that holds file `i`, which the content rule deletes.
    pub fn kept_copy(&self, i: usize) -> Option<usize> {
        if self.content.get(i).copied().flatten() != Some(Action::Delete) {
            return None;
        }
        self.kept.get(i).copied().flatten()
    }

    /// What `mode` suggests for each file, as things stand.
    ///
    /// The content rule's are the scan's, less each deletion that has
    /// lapsed. The other two read nothing but the groups, and judge a file
    /// by the representatives it was matched to, so a group whose
    /// representative has gone or changed says nothing: its files were never
    /// compared with each other, and judged by it every one of them stayed
    /// DELETE, resting on a file that is not the one the scan saw. A file
    /// only such a group held has no suggestion. No rule deletes a file that
    /// has lapsed, and one gone has none.
    pub fn actions(&self, mode: Mode) -> Vec<Option<Action>> {
        let n = self.files.len();
        let mut actions = if mode == Mode::Content {
            let mut a = self.content.clone();
            a.resize(n, None);
            a
        } else {
            let seats = self.groups.iter().filter(|g| self.lapsed(g.rep()).is_none()).flat_map(|g| {
                g.members.iter().filter(|m| self.lapsed(m.file) != Some(Lapse::Gone)).map(|m| Seat {
                    file: m.file,
                    representative: m.with.is_none(),
                    correlation: m.with.map_or(1.0, |e| e.agreement()),
                })
            });
            suggest::by_group(mode, n, seats, self.min_correlation)
        };
        suggest::revoke(&mut actions, &mut [], |i| self.lapsed(i));
        for (i, a) in actions.iter_mut().enumerate() {
            if self.lapsed(i) == Some(Lapse::Gone) {
                *a = None;
            }
        }
        actions
    }

    /// Take in that files `ids` have gone or changed since the scan: none is
    /// suggested for deletion, nor is a file the content rule deleted on the
    /// strength of one, which it may no longer hold (`suggest::revoke`).
    /// Gone outlasts changed. Whether anything new lapsed.
    pub fn lapse(&mut self, ids: impl IntoIterator<Item = usize>, why: Lapse) -> bool {
        let n = self.files.len();
        self.lapsed.resize(n, None);
        self.unheld.resize(n, None);
        let mut any = false;
        for i in ids {
            if self.lapsed[i] != Some(Lapse::Gone) && self.lapsed[i] != Some(why) {
                self.lapsed[i] = Some(why);
                any = true;
            }
        }
        let lapsed = &self.lapsed;
        for (i, why) in suggest::revoke(&mut self.content, &mut self.kept, |i| lapsed[i]) {
            self.unheld[i] = Some(why);
        }
        any
    }

    /// The files, of those not yet known to have lapsed, that are gone (not
    /// found: one that cannot be looked at for another reason is still there,
    /// as the scan left it) and that have changed since the scan started
    /// (`last::changed_since`, against `after`). A `stat` each, on every
    /// thread.
    pub fn look(&self, after: Option<u64>) -> (Vec<usize>, Vec<usize>) {
        let seen: Vec<Option<Lapse>> = (0..self.files.len())
            .into_par_iter()
            .map(|i| {
                if self.lapsed(i).is_some() {
                    return None;
                }
                let f = &self.files[i];
                match std::fs::metadata(&f.path) {
                    Ok(md) => crate::last::changed_since(&md, f.bytes.unwrap_or(0), after).then_some(Lapse::Changed),
                    Err(_) if std::fs::symlink_metadata(&f.path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) => Some(Lapse::Gone),
                    Err(_) => None,
                }
            })
            .collect();
        let of = |why| seen.iter().enumerate().filter(|(_, s)| **s == Some(why)).map(|(i, _)| i).collect();
        (of(Lapse::Gone), of(Lapse::Changed))
    }

    /// The groups as a report states them: without the files gone since,
    /// and none left with fewer than two files, or without the file every
    /// other was compared with, whose rows would claim nothing about each
    /// other. Representative first.
    pub fn shown(&self) -> impl Iterator<Item = Vec<Member>> + '_ {
        self.groups.iter().filter(|g| self.lapsed(g.rep()) != Some(Lapse::Gone)).filter_map(|g| {
            let left: Vec<Member> = g.members.iter().copied().filter(|m| self.lapsed(m.file) != Some(Lapse::Gone)).collect();
            (left.len() >= 2).then_some(left)
        })
    }

    /// Write the scan to `path`, whole or not at all, every path absolute:
    /// read back from another folder, or by the window from the home folder,
    /// a relative one names another file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let files = self
            .files
            .iter()
            .map(|f| File { path: std::path::absolute(&f.path).unwrap_or_else(|_| f.path.clone()), ..f.clone() })
            .collect();
        let saved = Saved {
            format: FORMAT,
            head: &self.head,
            min_correlation: self.min_correlation,
            files,
            groups: &self.groups,
            against: &self.against,
            content: &self.content,
            kept: &self.kept,
        };
        crate::report::write_whole(path, |w| serde_json::to_writer(&mut *w, &saved).map_err(std::io::Error::other))
    }

    /// Read a scan `save` wrote.
    pub fn load(path: &Path) -> Result<Outcome> {
        // Read whole and parsed from memory: serde_json reading through a
        // reader takes each byte through a call of its own.
        let bytes = std::fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
        let o: Outcome = serde_json::from_slice(&bytes).with_context(|| format!("could not read {}", path.display()))?;
        anyhow::ensure!(o.format == FORMAT, "{} was written by another version of img-fp", path.display());
        Ok(o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(relation: Relation, corr: f32) -> Option<Evidence> {
        Some(Evidence { relation, aligned_points: None, frame_overlap: 1.0, pixel_correlation: corr, mirrored: false, inverted: false })
    }

    /// A crop: B is the right half of A, at A's scale, working images 512
    /// x 256 and 256 x 256 (the files' own sizes do not enter: the map is in
    /// fractions of each picture).
    #[test]
    fn a_place_is_in_fractions_of_each_picture_and_reverses() {
        let m = [1.0, 0.0, -256.0, 0.0, 1.0, 0.0];
        let p = Place::of(&m, (512, 256), (256, 256)).unwrap();
        assert_eq!(p.apply(0.5, 0.0), (0.0, 0.0), "A's middle top is B's corner");
        assert_eq!(p.apply(1.0, 1.0), (1.0, 1.0));
        let back = p.reversed().unwrap();
        assert_eq!(back.apply(0.0, 0.0), (0.5, 0.0));
        assert_eq!(back.apply(1.0, 1.0), (1.0, 1.0));
        // A mirror stays one: B's left edge is A's right.
        let flip = Place::of(&[-1.0, 0.0, 512.0, 0.0, 1.0, 0.0], (512, 256), (512, 256)).unwrap();
        assert_eq!(flip.apply(0.0, 0.5), (1.0, 0.5));
        assert_eq!(flip.reversed().unwrap(), flip);
        assert_eq!(Place::of(&m, (0, 256), (256, 256)), None);
    }

    fn file(i: usize) -> File {
        File { path: PathBuf::from(format!("/p/{i}.jpg")), dims: Some((8, 8)), bytes: Some(1), damaged: false, library: false }
    }

    /// 0 holds 1 and 2; 3 holds 4. The content rule deletes 1 and 2 for 0,
    /// and 4 for 3.
    fn outcome() -> Outcome {
        let m = |file, with| Member { file, with, at: None };
        let groups = vec![
            Group { members: vec![m(0, None), m(1, ev(Relation::Identical, 1.0)), m(2, ev(Relation::Direct, 0.95))] },
            Group { members: vec![m(3, None), m(4, ev(Relation::Direct, 0.75))] },
        ];
        let d = Some(Action::Delete);
        let k = Some(Action::Keep);
        Outcome::new(Head::default(), 0.7, (0..5).map(file).collect(), groups, vec![k, d, d, k, d], vec![None, Some(0), Some(0), None, Some(3)])
    }

    #[test]
    fn the_group_rules_read_the_groups() {
        let o = outcome();
        let (k, d) = (Some(Action::Keep), Some(Action::Delete));
        assert_eq!(o.actions(Mode::Representative), [k, d, d, k, d]);
        // 0.9 is the cut at a bar of 0.7: deleted at 1 and 0.95, kept at 0.75.
        assert_eq!(o.actions(Mode::Correlation), [k, d, d, k, k]);
    }

    #[test]
    fn a_lapsed_kept_copy_takes_back_what_rests_on_it() {
        let mut o = outcome();
        assert!(o.lapse([0], Lapse::Changed));
        assert!(!o.lapse([0], Lapse::Changed), "nothing new");
        let (k, d, r) = (Some(Action::Keep), Some(Action::Delete), Some(Action::Review));
        assert_eq!(o.actions(Mode::Content), [k, r, r, k, d]);
        assert_eq!((o.unheld(1), o.unheld(4)), (Some(Lapse::Changed), None));
        assert_eq!(o.kept_copy(1), None);
        assert_eq!(o.kept_copy(4), Some(3));
        // The first group's representative changed: it says nothing.
        assert_eq!(o.actions(Mode::Representative), [None, None, None, k, d]);
        assert!(o.lapse([0, 4], Lapse::Gone));
        assert_eq!(o.lapsed(0), Some(Lapse::Gone), "gone outlasts changed");
        assert_eq!(o.actions(Mode::Content), [None, r, r, k, None]);
        assert_eq!(o.shown().count(), 0, "one group lost its representative, the other is one file");
    }

    /// The group rules judge a file only by representatives still there: a
    /// file that agreed closely with one now gone and loosely with another
    /// is judged by the other, and one only a gone representative held has
    /// no suggestion.
    #[test]
    fn the_group_rules_judge_by_representatives_still_there() {
        let m = |file, with| Member { file, with, at: None };
        // 0 holds 1 and 2 closely; 3 holds 1 loosely.
        let groups = vec![
            Group { members: vec![m(0, None), m(1, ev(Relation::Direct, 0.99)), m(2, ev(Relation::Direct, 0.99))] },
            Group { members: vec![m(3, None), m(1, ev(Relation::Direct, 0.75))] },
        ];
        let mut o = Outcome::new(Head::default(), 0.7, (0..4).map(file).collect(), groups, Vec::new(), Vec::new());
        assert_eq!(o.actions(Mode::Correlation)[1], Some(Action::Delete), "by its best agreement");
        o.lapse([0], Lapse::Gone);
        for (mode, want) in [(Mode::Correlation, Action::Keep), (Mode::Representative, Action::Delete)] {
            let a = o.actions(mode);
            assert_eq!(a[1], Some(want), "{mode:?}: judged by the representative still there");
            assert_eq!(a[2], None, "{mode:?}: only the gone one held it");
        }
    }

    #[test]
    fn a_saved_scan_is_read_back_whole() {
        let dir = std::env::temp_dir().join(format!("img-fp-outcome-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut o = outcome();
        use std::os::unix::ffi::OsStringExt;
        o.files[2].path = PathBuf::from(std::ffi::OsString::from_vec(b"rel/\xff.jpg".to_vec()));
        o.save(&dir.join("s.json")).unwrap();
        let back = Outcome::load(&dir.join("s.json")).unwrap();
        assert_eq!(back.files[2].path, std::path::absolute(&o.files[2].path).unwrap(), "absolute, and the name's own bytes");
        assert_eq!(back.files[0].path, o.files[0].path);
        for mode in [Mode::Content, Mode::Correlation, Mode::Representative] {
            assert_eq!(back.actions(mode), o.actions(mode));
        }
        assert_eq!(back.groups[1].members[1].with, o.groups[1].members[1].with);
        assert_eq!(back.kept_copy(4), Some(3));
        std::fs::write(dir.join("old.json"), br#"{"format": 0}"#).unwrap();
        assert!(Outcome::load(&dir.join("old.json")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
