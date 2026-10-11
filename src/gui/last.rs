//! The last finished scan, kept on disk so that closing the window does not
//! cost it.
//!
//! One scan only, the one the results page shows: a scan that finishes with
//! results replaces it, and one that is cancelled or fails leaves it alone,
//! as it leaves the page alone. What is kept is the worker's own report (the
//! groups, without `pairs`) and what the page says beside it: whether the
//! scan had problems, how long it took, when it finished, and its log. And
//! the images marked for the Trash, written as they change (`save_marks`).
//!
//! `$XDG_CACHE_HOME/img-fp/last-scan/`, or `~/.cache/img-fp/last-scan/`, in a
//! folder only this user can open, since it lists their files. Written as a
//! folder of its own beside it and renamed into place, so a scan read back is
//! never one scan's report with another's notes.

use crate::results::{self, Prepared};
use crate::scan;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const REPORT: &str = "result.json";
const NOTES: &str = "scan.json";
const MARKS: &str = "marks.json";

/// The images marked, and the scan they were marked in (`Notes::id`): marks are read back only beside that scan. A new scan's folder
/// replaces the old one, but a mark saved in between, or a scan whose own
/// save failed, would otherwise put one scan's marks on another's files.
#[derive(serde::Serialize, serde::Deserialize)]
struct Marks {
    scan: u64,
    #[serde(with = "crate::settings::paths")]
    paths: Vec<PathBuf>,
}

/// What the page says about a scan besides its groups.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Notes {
    pub problems: bool,
    pub took: String,
    /// Seconds since 1970 when it finished.
    pub finished: u64,
    /// Which scan this is, for its marks: nanoseconds since 1970 when it
    /// finished, which two scans do not share.
    #[serde(default)]
    pub id: u64,
    pub log: String,
    /// The folders the scan was given, which the tree view starts from.
    #[serde(default, with = "crate::settings::paths")]
    pub roots: Vec<PathBuf>,
}

pub struct Last {
    /// The report, made ready for the page here, off the main thread.
    pub prepared: Prepared,
    pub notes: Notes,
    /// Files the report names that are no longer there: moved to the Trash
    /// before the window was closed, or since.
    pub gone: HashSet<PathBuf>,
    /// The images that were marked for the Trash, by number
    /// (`Prepared::paths`).
    pub marks: Vec<u32>,
}

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("img-fp").join("last-scan"))
}

/// Keep `report`, a finished scan's report, with `notes`, in place of the
/// scan kept before, and say whether it was. Failing to is not worth
/// interrupting anyone over: the results are on screen either way.
pub fn save(report: &Path, notes: &Notes) -> bool {
    dir().is_some_and(|dir| save_in(&dir, report, notes).is_ok())
}

/// Keep `paths` as the marks of scan `scan` (`Notes::id`), if that is the
/// scan kept. Written beside it and renamed over the last list, so a
/// window closed mid-write leaves the list before.
pub fn save_marks(scan: u64, paths: Vec<PathBuf>) {
    if let Some(dir) = dir() {
        let _ = save_marks_in(&dir, scan, paths);
    }
}

fn save_marks_in(dir: &Path, scan: u64, paths: Vec<PathBuf>) -> std::io::Result<()> {
    let notes: Notes = serde_json::from_slice(&std::fs::read(dir.join(NOTES))?).map_err(std::io::Error::other)?;
    if notes.id != scan {
        return Ok(());
    }
    let new = dir.join(format!("{MARKS}.{}.new", std::process::id()));
    std::fs::write(&new, serde_json::to_vec(&Marks { scan, paths }).map_err(std::io::Error::other)?)?;
    std::fs::rename(&new, dir.join(MARKS)).inspect_err(|_| {
        let _ = std::fs::remove_file(&new);
    })
}

fn save_in(dir: &Path, report: &Path, notes: &Notes) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let parent = dir.parent().ok_or_else(|| std::io::Error::other("no parent"))?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
    let new = parent.join(format!("last-scan.{}.new", std::process::id()));
    let _ = std::fs::remove_dir_all(&new);
    std::fs::DirBuilder::new().mode(0o700).create(&new)?;
    let written = (|| {
        std::fs::copy(report, new.join(REPORT))?;
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

/// Whether a scan is kept, without reading it: whether to open the window on
/// the results page.
pub fn exists() -> bool {
    dir().is_some_and(|d| d.join(NOTES).is_file())
}

/// The scan kept last time, if there is one that can still be read. One that
/// cannot — a report from a build that wrote another shape — is deleted, so
/// that it is not tried on every start.
pub fn load() -> Option<Last> {
    load_in(&dir()?)
}

fn load_in(dir: &Path) -> Option<Last> {
    let notes = std::fs::read(dir.join(NOTES)).ok()?;
    let read = serde_json::from_slice::<Notes>(&notes).ok().and_then(|notes| {
        let report = dir.join(REPORT);
        // `read_report` takes a missing report for a scan that found nothing.
        if !report.exists() {
            return None;
        }
        scan::read_report(&report).ok().map(|found| (found, notes))
    });
    let Some((found, notes)) = read else {
        let _ = std::fs::remove_dir_all(dir);
        return None;
    };
    let mut prepared = results::prepare(found);
    prepared.set_roots(notes.roots.clone());
    // Gone as the Trash counts it: not found. A file that cannot be looked at
    // for another reason is still shown, as the scan left it. Once a file,
    // not once a group it is in, and on every thread: a `stat` each.
    let gone = prepared
        .paths()
        .par_iter()
        .filter(|p| std::fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound))
        .cloned()
        .collect();
    let marks = std::fs::read(dir.join(MARKS))
        .ok()
        .and_then(|b| serde_json::from_slice::<Marks>(&b).ok())
        .filter(|m| m.scan == notes.id)
        .map_or_else(Vec::new, |m| {
            let marked: HashSet<PathBuf> = m.paths.into_iter().collect();
            (0..prepared.paths().len() as u32).filter(|&id| marked.contains(&prepared.paths()[id as usize])).collect()
        });
    Some(Last { prepared, notes, gone, marks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_scan_is_read_back_and_the_next_replaces_it() {
        let root = std::env::temp_dir().join(format!("img-fp-last-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (a, b) = (root.join("a.png"), root.join("b.png"));
        std::fs::write(&a, b"x").unwrap();
        let report = |name: &str, files: &[&Path]| {
            let files: Vec<serde_json::Value> = files
                .iter()
                .enumerate()
                .map(|(i, p)| serde_json::json!({"path": p, "role": if i == 0 { "representative" } else { "match" }}))
                .collect();
            let p = root.join(name);
            std::fs::write(&p, serde_json::json!({"groups": [{"files": files}], "files_analysed": 7}).to_string()).unwrap();
            p
        };
        let dir = root.join("cache").join("img-fp").join("last-scan");
        let notes = |id: u64, log: &str| Notes { problems: true, took: "1:02".into(), finished: 5, id, log: log.into(), roots: vec![root.clone()] };

        save_in(&dir, &report("one.json", &[&a, &b]), &notes(5, "first")).unwrap();
        let last = load_in(&dir).unwrap();
        assert_eq!(last.prepared.analysed(), 7);
        assert_eq!(last.prepared.paths().len(), 2);
        assert!(last.notes.problems);
        assert_eq!(last.notes.log, "first");
        assert_eq!(last.notes.roots, [root.clone()]);
        // `b.png` was never there: the page is told it is gone.
        assert_eq!(last.gone, HashSet::from([b.clone()]));
        assert!(last.marks.is_empty());

        // Marks are kept for the scan they were made in, and only for it.
        save_marks_in(&dir, 5, vec![a.clone()]).unwrap();
        assert_eq!(load_in(&dir).unwrap().marks, vec![0]);
        save_marks_in(&dir, 6, vec![b.clone()]).unwrap();
        assert_eq!(load_in(&dir).unwrap().marks, vec![0]);

        save_in(&dir, &report("two.json", &[&b, &a, &a]), &notes(7, "second")).unwrap();
        let last = load_in(&dir).unwrap();
        assert_eq!(last.prepared.paths(), [b.clone(), a.clone()]);
        assert_eq!(last.notes.log, "second");
        // The next scan starts with none.
        assert!(last.marks.is_empty());
        // Nothing of the save is left beside it.
        let left: Vec<_> = std::fs::read_dir(dir.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("last-scan")]);

        // One that will not read is dropped rather than tried again.
        std::fs::write(dir.join(REPORT), b"{").unwrap();
        assert!(load_in(&dir).is_none());
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
