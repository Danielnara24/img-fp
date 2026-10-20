//! The last finished scan, kept on disk so that closing the window does not
//! cost it: `img_fp::last` keeps it, here it is read back for the page.
//!
//! One scan only, the one the results page shows, and the command line's
//! last scan as well: a scan that finishes with results replaces it, from
//! here or from a terminal, and one that is cancelled or fails leaves it
//! alone, as it leaves the page alone. So does one run without the cache or
//! one that found no pictures, as on the command line (`img_fp::last`). Beside it, the images marked for the
//! Trash, written as they change (`save_marks`).
//!
//! `$XDG_CACHE_HOME/img-fp/last-scan/`, or `~/.cache/img-fp/last-scan/`: the
//! default cache's folder, whatever cache a scan from the window used.

use crate::results::{self, Prepared};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use img_fp::last::{MARKS, NOTES, REPORT};
pub use img_fp::last::Notes;

/// The images marked, and the scan they were marked in (`Notes::id`): marks are read back only beside that scan. A new scan's folder
/// replaces the old one, but a mark saved in between, or a scan whose own
/// save failed, would otherwise put one scan's marks on another's files.
#[derive(serde::Serialize, serde::Deserialize)]
struct Marks {
    scan: u64,
    #[serde(with = "img_fp::last::paths")]
    paths: Vec<PathBuf>,
}

pub struct Last {
    /// The report, made ready for the page here, off the main thread.
    pub prepared: Prepared,
    pub notes: Notes,
    /// Files the scan names that are no longer there: moved to the Trash
    /// before the window was closed, or since.
    pub gone: HashSet<PathBuf>,
    /// The images that were marked for the Trash, by number.
    pub marks: Vec<u32>,
}

fn dir() -> Option<PathBuf> {
    Some(img_fp::last::dir(None))
}

/// Keep `report`, a finished scan's result, with `notes`, in place of the
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
    img_fp::last::save_with(dir, |to| std::fs::copy(report, to).map(|_| ()), notes)
}

/// Whether a scan is kept, without reading it: whether to open the window on
/// the results page.
pub fn exists() -> bool {
    dir().is_some_and(|d| d.join(NOTES).is_file())
}

/// The scan kept last time, if there is one that can still be read. One that
/// cannot — kept by a build that wrote another shape — is deleted, so that it
/// is not tried on every start.
pub fn load() -> Option<Last> {
    load_in(&dir()?)
}

fn load_in(dir: &Path) -> Option<Last> {
    let notes = std::fs::read(dir.join(NOTES)).ok()?;
    let read = serde_json::from_slice::<Notes>(&notes).ok().and_then(|notes| {
        // Not `read_result`, which takes a missing file for a scan that found
        // nothing.
        img_fp::Outcome::load(&dir.join(REPORT)).ok().map(|found| (found, notes))
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
    let files = &prepared.scan().files;
    let gone = prepared.scan().look(None).0.into_iter().map(|i| files[i].path.clone()).collect();
    let marks = std::fs::read(dir.join(MARKS))
        .ok()
        .and_then(|b| serde_json::from_slice::<Marks>(&b).ok())
        .filter(|m| m.scan == notes.id)
        .map_or_else(Vec::new, |m| {
            let marked: HashSet<PathBuf> = m.paths.into_iter().collect();
            (0..files.len() as u32).filter(|&id| marked.contains(&files[id as usize].path)).collect()
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
            use img_fp::outcome::{File, Group, Head, Member};
            let ev = img_fp::Evidence { relation: img_fp::Relation::Identical, aligned_points: None, frame_overlap: 1.0, pixel_correlation: 1.0, mirrored: false, inverted: false };
            let files: Vec<File> = files.iter().map(|p| File { path: p.to_path_buf(), dims: None, bytes: None, damaged: false }).collect();
            let members = (0..files.len()).map(|i| Member { file: i, with: (i > 0).then_some(ev) }).collect();
            let head = Head { files_analysed: 7, ..Default::default() };
            let p = root.join(name);
            img_fp::Outcome::new(head, 0.7, files, vec![Group { members }], Vec::new(), Vec::new()).save(&p).unwrap();
            p
        };
        let paths = |l: &Last| l.prepared.scan().files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        let dir = root.join("cache").join("img-fp").join("last-scan");
        let notes = |id: u64, log: &str| Notes { problems: true, took: "1:02".into(), finished: 5, started_ns: 0, id, log: log.into(), roots: vec![root.clone()] };

        save_in(&dir, &report("one.json", &[&a, &b]), &notes(5, "first")).unwrap();
        let last = load_in(&dir).unwrap();
        assert_eq!(last.prepared.analysed(), 7);
        assert_eq!(paths(&last).len(), 2);
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

        save_in(&dir, &report("two.json", &[&b, &a]), &notes(7, "second")).unwrap();
        let last = load_in(&dir).unwrap();
        assert_eq!(paths(&last), [b.clone(), a.clone()]);
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
