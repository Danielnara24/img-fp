//! The last finished scan, kept beside the cache, which `--last` writes out
//! again and the window opens on.
//!
//! One scan only, shared by both: a scan from the command line and a scan
//! from the window each replace it, so a scan run in a terminal can be marked
//! in the window, and a window's scan written out as CSV by `--last`. What is
//! kept is the scan's `Outcome` (the groups by number, without the pairs
//! between files no group puts side by side) and what the window says beside
//! it (`Notes`). A scan kept by a version before the `Outcome` is not read.
//!
//! The suggestions kept are always the content rule's, whatever `--suggest`
//! the scan was given: the other two rules read nothing but the groups and
//! are worked out again (`Outcome::actions`), and the content rule could not
//! be.
//!
//! `last-scan/` in the folder of the cache file the run used, so a sweep
//! handing itself a `--cache` of its own keeps its scans out of the default
//! one; the window always uses the default folder. Not kept under
//! `--no-cache` or `--no-last`, nor for a run `--against` a library, whose
//! report is another shape. The folder is this user's alone, since it lists
//! their files, and is written beside the old one and renamed into place, so
//! a scan read back is never one scan's report with another's notes.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::outcome::Outcome;
use crate::report;
use crate::suggest::{self, Lapse};

/// The kept `Outcome`. A scan kept as a report, before it, was `result.json`.
pub const REPORT: &str = "found.json";
pub const NOTES: &str = "scan.json";
pub const MARKS: &str = "marks.json";

/// What the window says about a scan besides its groups.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Notes {
    pub problems: bool,
    pub took: String,
    /// Seconds since 1970 when it finished.
    pub finished: u64,
    /// Nanoseconds since 1970 when it started, before anything was read: a
    /// file changed since then may have changed after the scan read it.
    pub started_ns: u64,
    /// Which scan this is, for its marks: nanoseconds since 1970 when it
    /// finished, which two scans do not share.
    pub id: u64,
    pub log: String,
    /// The folders the scan was given, which the window's tree view starts
    /// from.
    #[serde(with = "paths")]
    pub roots: Vec<PathBuf>,
}

impl Notes {
    /// Notes for a scan finishing now, which started at `started_ns`
    /// (`now_nanos`) and took `secs`.
    pub fn now(problems: bool, started_ns: u64, secs: u64, log: String, roots: Vec<PathBuf>) -> Notes {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        Notes {
            problems,
            took: format!("{}:{:02}", secs / 60, secs % 60),
            finished: now.as_secs(),
            started_ns,
            id: now.as_nanos() as u64,
            log,
            roots,
        }
    }

    /// The time, in nanoseconds since 1970, a file changed after is not the
    /// file this scan judged (`changed_since`): when the scan started.
    ///
    /// **Its start, not its end.** A scan reads each file once, somewhere in
    /// the middle, and a file written after that — an edit saved during a
    /// long scan — changed before the scan finished, so measured against the
    /// end it passed for the file the scan judged, and its length alone could
    /// tell it apart.
    pub fn changed_after(&self) -> u64 {
        self.started_ns
    }
}

/// Nanoseconds since 1970, now: what `Notes::started_ns` holds.
pub fn now_nanos() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

/// Whether a file is not the file a scan judged: its length is not the one
/// the scan found (`scanned`, `0` when it did not say), or it changed after
/// `after` (nanoseconds since 1970; `Notes::changed_after`). Its change time is the
/// kernel's, which `cp -p` and `rsync -a` cannot put back as they put back a
/// modification time; a rename or a `chmod` since counts too, which is the
/// side to err on. The window's Trash leaves such a file where it is, and
/// `--last` suggests deleting neither it nor anything resting on it.
///
/// **To the nanosecond, as the kernel keeps it.** It was to the second, so
/// a file written in the second a scan started — copied in just before it,
/// by a script that then scans — was taken for one changed since: never
/// suggested for deletion, and left behind by the Trash even when marked. A
/// change time with no fraction at all is one a filesystem keeps to the
/// second (FAT, some network shares), and is still read that way: changed
/// in the second the scan started, it may have changed after it.
pub fn changed_since(md: &std::fs::Metadata, scanned: u64, after: Option<u64>) -> bool {
    use std::os::unix::fs::MetadataExt;
    const NS: i64 = 1_000_000_000;
    let changed_after = |t: u64| match md.ctime_nsec() {
        0 => md.ctime() >= t as i64 / NS,
        ns => md.ctime().saturating_mul(NS).saturating_add(ns) > t as i64,
    };
    (scanned > 0 && md.len() != scanned) || after.is_some_and(changed_after)
}

/// Where the scan is kept for a run whose `--cache` is `cache`: `last-scan/`
/// beside the cache file.
pub fn dir(cache: Option<&Path>) -> PathBuf {
    let file = crate::cache::locate(cache);
    match file.parent().filter(|d| !d.as_os_str().is_empty()) {
        Some(d) => d.join("last-scan"),
        None => PathBuf::from("last-scan"),
    }
}

/// Keep a scan in `dir` in place of the one kept before: `write` puts its
/// report at the path it is given, and `notes` go beside it.
pub fn save_with(dir: &Path, write: impl FnOnce(&Path) -> std::io::Result<()>, notes: &Notes) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let parent = dir.parent().ok_or_else(|| std::io::Error::other("no parent"))?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
    let new = parent.join(format!("last-scan.{}.new", std::process::id()));
    let _ = std::fs::remove_dir_all(&new);
    std::fs::DirBuilder::new().mode(0o700).create(&new)?;
    let written = (|| {
        write(&new.join(REPORT))?;
        std::fs::write(new.join(NOTES), serde_json::to_vec(notes).map_err(std::io::Error::other)?)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&new);
        return Err(e);
    }
    // A folder cannot be renamed over one that holds anything, so the old one
    // goes first; between the two there is no scan kept, never a mixed one.
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&new, dir).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&new);
    })
}

/// A path in JSON: a name as text, or as its bytes when it is not UTF-8.
/// serde writes a `PathBuf` as a string and refuses one that is not UTF-8, and
/// one such folder made a whole save fail, silently.
pub mod path {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    pub(super) enum Stored {
        Text(String),
        Bytes(Vec<u8>),
    }

    impl Stored {
        pub(super) fn of(p: &Path) -> Stored {
            match p.to_str() {
                Some(t) => Stored::Text(t.to_string()),
                None => Stored::Bytes(p.as_os_str().as_bytes().to_vec()),
            }
        }

        pub(super) fn path(self) -> PathBuf {
            match self {
                Stored::Text(t) => PathBuf::from(t),
                Stored::Bytes(b) => PathBuf::from(OsString::from_vec(b)),
            }
        }
    }

    pub fn serialize<S: Serializer>(p: &Path, s: S) -> Result<S::Ok, S::Error> {
        Stored::of(p).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PathBuf, D::Error> {
        Ok(Stored::deserialize(d)?.path())
    }
}

/// A list of paths, each as `path` writes one.
pub mod paths {
    use super::path::Stored;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::path::PathBuf;

    pub fn serialize<S: Serializer>(v: &[PathBuf], s: S) -> Result<S::Ok, S::Error> {
        v.iter().map(|p| Stored::of(p)).collect::<Vec<_>>().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PathBuf>, D::Error> {
        Ok(Vec::<Stored>::deserialize(d)?.into_iter().map(Stored::path).collect())
    }
}

// ---------------------------------------------------------------- --last

/// What `--last` was asked for.
pub struct Rewrite<'a> {
    pub cache: Option<&'a Path>,
    pub output: Option<&'a Path>,
    pub format: Option<report::Format>,
    pub suggest: suggest::Mode,
}

/// `--last`: write the kept scan's report again, where `-o` says, in the
/// layout `--format` says, with `--suggest`'s rule. Files gone or changed
/// since the scan are taken in as the window takes them in
/// (`Outcome::lapse`); a file gone is left out of its groups
/// (`Outcome::shown`). Returns the lines to say about it.
pub fn rewrite(asked: &Rewrite) -> Result<Vec<String>> {
    let dir = dir(asked.cache);
    let unread = || format!("could not read the kept scan in {}", dir.display());
    let notes: Notes = match std::fs::read(dir.join(NOTES)) {
        Ok(b) => serde_json::from_slice(&b).with_context(unread)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("no scan is kept in {}: run a scan first (one run with --no-cache or --no-last is not kept)", dir.display())
        }
        Err(e) => return Err(e).with_context(unread),
    };
    if !dir.join(REPORT).exists() {
        anyhow::bail!("the scan kept in {} was kept by an earlier version of img-fp: run a scan", dir.display());
    }
    let mut scan = Outcome::load(&dir.join(REPORT)).with_context(unread)?;
    if asked.suggest == suggest::Mode::Content && !scan.has_content() {
        anyhow::bail!("the kept scan has no suggestions by content; --suggest content needs a new scan");
    }
    let (gone, changed) = scan.look(Some(notes.changed_after()));
    scan.lapse(gone.iter().copied(), Lapse::Gone);
    scan.lapse(changed.iter().copied(), Lapse::Changed);
    // The content rule's deletions taken back; the group rules name no kept
    // copy.
    let unheld = if asked.suggest == suggest::Mode::Content { (0..scan.files.len()).filter(|&i| scan.unheld(i) == Some(Lapse::Gone)).count() } else { 0 };
    let (gone, changed) = (gone.len(), changed.len());

    let target = report::Target::of(asked.output, asked.format);
    report::write_with(&target, &scan, asked.suggest, None)?;

    let when = ago(notes.finished);
    let groups = scan.shown().count();
    let mut lines = vec![match &target.sink {
        report::Sink::File(p) => format!("{} from the scan {when} -> {}", groups_said(groups), p.display()),
        report::Sink::Stdout => format!("{} from the scan {when}", groups_said(groups)),
    }];
    if gone > 0 {
        lines.push(format!("{gone} {} gone since the scan {} left out", if gone == 1 { "file" } else { "files" }, if gone == 1 { "is" } else { "are" }));
    }
    if changed > 0 {
        lines.push(format!(
            "{changed} {} changed since the scan: none of {} is suggested for deletion, nor is anything resting on {}",
            if changed == 1 { "file has" } else { "files have" },
            if changed == 1 { "it" } else { "them" },
            if changed == 1 { "it" } else { "them" }
        ));
    }
    if unheld > 0 {
        lines.push(format!(
            "{unheld} {} whose kept copy is gone {} REVIEW instead",
            if unheld == 1 { "deletion" } else { "deletions" },
            if unheld == 1 { "is" } else { "are" }
        ));
    }
    if target.format == report::Format::Json {
        lines.push("The JSON has the groups and no \"pairs\" list, which only a scan writes.".into());
    }
    Ok(lines)
}

fn groups_said(n: usize) -> String {
    format!("{n} {}", if n == 1 { "group" } else { "groups" })
}

/// How long ago a scan finished `secs` after 1970, as a person says it:
/// Rust's std has no time zones, and an hour of the clock in UTC reads as the
/// wrong one.
fn ago(secs: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let d = now.saturating_sub(secs);
    let (n, unit) = match d {
        0..60 => return "less than a minute ago".into(),
        60..3600 => (d / 60, "minute"),
        3600..86_400 => (d / 3600, "hour"),
        _ => (d / 86_400, "day"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{Evidence, File, Group, Head, Member, Relation};
    use crate::suggest::Action;
    use std::os::unix::ffi::OsStringExt;

    fn ev(relation: Relation, corr: f32) -> Option<Evidence> {
        let aligned_points = matches!(relation, Relation::Direct | Relation::Corroborated).then_some(42);
        Some(Evidence { relation, aligned_points, frame_overlap: 0.987, pixel_correlation: corr, mirrored: false, inverted: false })
    }

    /// A scan of files under `root`, kept beside a cache there, and `--last`
    /// asked to write it as `name`.
    fn last(root: &Path, name: &str, suggest: suggest::Mode) -> (Vec<String>, Vec<u8>) {
        let out = root.join(name);
        let asked = Rewrite { cache: Some(&root.join("c.bin")), output: Some(&out), format: None, suggest };
        let said = rewrite(&asked).unwrap();
        (said, std::fs::read(out).unwrap())
    }

    fn keep(root: &Path, scan: &Outcome, started_ns: u64) {
        let notes = Notes::now(false, started_ns, 1, String::new(), vec![]);
        save_with(&dir(Some(&root.join("c.bin"))), |p| scan.save(p).map_err(std::io::Error::other), &notes).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("img-fp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// The kept scan writes the text, the CSV and the JSON the scan wrote,
    /// byte for byte, with every relation, a name that is not UTF-8, a
    /// damaged file and a file in two groups; and a file deleted since is
    /// left out, and with it a group it left with one file, and what rested
    /// on it is left for a person to look at.
    #[test]
    fn a_kept_scan_writes_the_reports_the_scan_wrote() {
        let root = scratch("keep");
        let mut paths: Vec<PathBuf> = ["a.jpg", "b;x.jpg", "c.png", "d.png", "e.png"].iter().map(|n| root.join(n)).collect();
        paths.push(root.join(std::ffi::OsString::from_vec(b"f\xff.jpg".to_vec())));
        // Each as long as the scan says it is, so that none reads as changed
        // since.
        for (k, f) in paths.iter().enumerate() {
            std::fs::write(f, vec![b'x'; k + 1]).unwrap();
        }
        let dims = [Some((640, 480)), Some((640, 480)), Some((320, 240)), None, Some((10, 10)), Some((8, 8))];
        let files: Vec<File> = paths.iter().enumerate().map(|(k, p)| File { path: p.clone(), dims: dims[k], bytes: Some(k as u64 + 1), damaged: k == 2 }).collect();
        let m = |file, with| Member { file, with };
        let mirrored = Some(Evidence { mirrored: true, ..ev(Relation::Propagated, 0.9).unwrap() });
        let inverted = Some(Evidence { inverted: true, ..ev(Relation::Corroborated, 0.6).unwrap() });
        let groups = vec![
            Group { members: vec![m(0, None), m(1, ev(Relation::Identical, 1.0)), m(2, mirrored), m(3, inverted)] },
            Group { members: vec![m(4, None), m(3, ev(Relation::SamePixels, 1.0)), m(5, ev(Relation::Direct, 0.95))] },
        ];
        let (k, d, r) = (Some(Action::Keep), Some(Action::Delete), Some(Action::Review));
        let head = Head { config: serde_json::json!({"suggest": "content", "min_pixel_correlation": 0.7}), files_enumerated: 6, files_analysed: 6, failures: vec![], runtime_seconds: 1.5 };
        let scan = Outcome::new(head, 0.7, files, groups, vec![k, d, r, d, k, None], vec![None, Some(0), None, Some(4), None, None]);
        keep(&root, &scan, now_nanos() + 60_000_000_000);
        for (name, format) in [("txt", report::Format::Txt), ("csv", report::Format::Csv), ("json", report::Format::Json)] {
            for mode in [suggest::Mode::Content, suggest::Mode::Correlation, suggest::Mode::Representative] {
                let first = root.join(format!("first.{name}"));
                report::write_with(&report::Target { sink: report::Sink::File(first.clone()), format }, &scan, mode, None).unwrap();
                let (said, again) = last(&root, &format!("again.{name}"), mode);
                assert_eq!(again, std::fs::read(first).unwrap(), "{name} {mode:?}");
                assert_eq!(said.len(), 1 + (format == report::Format::Json) as usize, "{said:?}");
            }
        }

        // `e.png` deleted: its group is left with two files, but not the one
        // they were compared with, so it goes; `d.png` stays in the first.
        // `d.png` was deleted on `e.png`'s strength: it rests on nothing now.
        std::fs::remove_file(&paths[4]).unwrap();
        let (said, txt) = last(&root, "gone.txt", suggest::Mode::Content);
        assert_eq!(&said[1..], ["1 file gone since the scan is left out", "1 deletion whose kept copy is gone is REVIEW instead"]);
        let txt = String::from_utf8_lossy(&txt);
        assert_eq!(txt.lines().filter(|l| l.starts_with("group_")).collect::<Vec<_>>(), ["group_1: 4 files"]);
        assert!(txt.contains("\tMATCH, REVIEW, -, 4B, corroborated, 42 points"), "{txt}");
        assert!(txt.contains("\tMATCH, DELETE, 640x480, 2B, identical, "), "`b;x.jpg` rests on `a.jpg`, still there");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file changed since the scan — another length, or touched after it
    /// started — is not suggested for deletion, nor is a file whose deletion
    /// rests on it; a file that has not changed is as the scan left it.
    #[test]
    fn a_file_changed_since_the_scan_is_not_deleted_nor_relied_on() {
        let root = scratch("changed");
        let paths: Vec<PathBuf> = ["keep.jpg", "copy.jpg", "other.jpg"].iter().map(|n| root.join(n)).collect();
        for f in &paths {
            std::fs::write(f, b"x").unwrap();
        }
        let files = paths.iter().map(|p| File { path: p.clone(), dims: Some((8, 8)), bytes: Some(1), damaged: false }).collect();
        let m = |file, with| Member { file, with };
        let groups = vec![Group { members: vec![m(0, None), m(1, ev(Relation::Direct, 0.99)), m(2, ev(Relation::Direct, 0.99))] }];
        let d = Some(Action::Delete);
        let scan = Outcome::new(Head::default(), 0.7, files, groups, vec![Some(Action::Keep), d, d], vec![None, Some(0), Some(0)]);
        let later = now_nanos() + 60_000_000_000;
        let action_of = |name: &str| {
            let csv = String::from_utf8(std::fs::read(root.join("r.csv")).unwrap()).unwrap();
            csv.lines().find(|l| l.contains(name)).unwrap().split(';').nth(2).unwrap().to_string()
        };

        // Unchanged: as the scan said.
        keep(&root, &scan, later);
        let (said, _) = last(&root, "r.csv", suggest::Mode::Content);
        assert_eq!(said.len(), 1);
        assert_eq!(action_of("copy.jpg"), "DELETE");

        // `copy.jpg` replaced by another picture of another length.
        std::fs::write(&paths[1], b"another picture").unwrap();
        let (said, _) = last(&root, "r.csv", suggest::Mode::Content);
        assert!(said[1].starts_with("1 file has changed since the scan"), "{said:?}");
        assert_eq!(action_of("copy.jpg"), "REVIEW");
        assert_eq!(action_of("other.jpg"), "DELETE", "an unchanged file is as it was");

        // `keep.jpg` touched after the scan started, at the same length:
        // what rested on it rests on nothing, by every rule.
        keep(&root, &scan, now_nanos() - 60_000_000_000);
        last(&root, "r.csv", suggest::Mode::Content);
        assert_eq!(action_of("other.jpg"), "REVIEW");
        last(&root, "r.csv", suggest::Mode::Representative);
        assert_eq!(action_of("other.jpg"), "", "judged by its representative, which changed");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A scan kept by a version before the `Outcome` is not read.
    #[test]
    fn a_scan_kept_by_an_earlier_version_asks_for_a_scan() {
        let root = scratch("earlier");
        let dir = dir(Some(&root.join("c.bin")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NOTES), serde_json::to_vec(&Notes::now(false, 0, 1, String::new(), vec![])).unwrap()).unwrap();
        std::fs::write(dir.join("result.json"), b"{}").unwrap();
        let asked = Rewrite { cache: Some(&root.join("c.bin")), output: None, format: None, suggest: suggest::Mode::Content };
        let e = rewrite(&asked).unwrap_err().to_string();
        assert!(e.contains("earlier version"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file changed while the scan ran — after it was read, before the
    /// scan finished — is changed since the scan, at the length it had.
    /// The file's change time, in nanoseconds since 1970.
    fn ctime_ns(md: &std::fs::Metadata) -> u64 {
        use std::os::unix::fs::MetadataExt;
        (md.ctime() * 1_000_000_000 + md.ctime_nsec()) as u64
    }

    #[test]
    fn a_file_changed_during_the_scan_is_changed_since_it() {
        let f = std::env::temp_dir().join(format!("img-fp-during-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        let md = std::fs::metadata(&f).unwrap();
        let now = ctime_ns(&md);
        let ns = 1_000_000_000;
        let notes = |started_ns: u64| Notes { problems: false, took: "1:00".into(), finished: now / ns + 30, started_ns, id: 1, log: String::new(), roots: vec![] };
        assert!(changed_since(&md, md.len(), Some(notes(now - 1).changed_after())), "written during the scan");
        assert!(!changed_since(&md, md.len(), Some(notes(now + 1).changed_after())), "written before it, if only just");
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn a_scan_is_kept_beside_the_cache_it_used() {
        assert_eq!(dir(Some(Path::new("/x/y/c.bin"))), Path::new("/x/y/last-scan"));
        assert_eq!(dir(Some(Path::new("/x/y/"))), Path::new("/x/y/last-scan"));
    }
}
