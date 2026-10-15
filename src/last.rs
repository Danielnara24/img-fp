//! The last finished scan, kept beside the cache, which `--last` writes out
//! again and the window opens on.
//!
//! One scan only, shared by both: a scan from the command line and a scan
//! from the window each replace it, so a scan run in a terminal can be marked
//! in the window, and a window's scan written out as CSV by `--last`. What is
//! kept is the report without its `pairs` (the groups, one row a file, which
//! is what the text, the CSV and the window read; on 27,000 pictures 15 MB of
//! a 303 MB report) and what the window says beside it (`Notes`).
//!
//! The suggestions kept are always the content rule's, whatever `--suggest`
//! the scan was given: the other two rules read nothing but the groups and
//! are worked out again here, and the content rule could not be.
//!
//! `last-scan/` in the folder of the cache file the run used, so a sweep
//! handing itself a `--cache` of its own keeps its scans out of the default
//! one; the window always uses the default folder. Not kept under
//! `--no-cache` or `--no-last`, nor for a run `--against` a library, whose
//! report is another shape. The folder is this user's alone, since it lists
//! their files, and is written beside the old one and renamed into place, so
//! a scan read back is never one scan's report with another's notes.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::report::{self, OutGroup, OutPair, Output};
use crate::suggest::{self, Action};

pub const REPORT: &str = "result.json";
pub const NOTES: &str = "scan.json";
pub const MARKS: &str = "marks.json";

/// What the window says about a scan besides its groups.
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
    /// The folders the scan was given, which the window's tree view starts
    /// from.
    #[serde(default, with = "paths")]
    pub roots: Vec<PathBuf>,
}

impl Notes {
    /// Notes for a scan finishing now, which took `secs`.
    pub fn now(problems: bool, secs: u64, log: String, roots: Vec<PathBuf>) -> Notes {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        Notes { problems, took: format!("{}:{:02}", secs / 60, secs % 60), finished: now.as_secs(), id: now.as_nanos() as u64, log, roots }
    }
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

/// A path list in JSON: a name as text, or as its bytes when it is not UTF-8.
/// serde writes a `PathBuf` as a string and refuses one that is not UTF-8, and
/// one such folder made a whole save fail, silently.
pub mod paths {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Text(String),
        Bytes(Vec<u8>),
    }

    pub fn serialize<S: Serializer>(v: &[PathBuf], s: S) -> Result<S::Ok, S::Error> {
        let stored: Vec<Stored> = v
            .iter()
            .map(|p| match p.to_str() {
                Some(t) => Stored::Text(t.to_string()),
                None => Stored::Bytes(p.as_os_str().as_bytes().to_vec()),
            })
            .collect();
        stored.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PathBuf>, D::Error> {
        let stored: Vec<Stored> = Vec::deserialize(d)?;
        Ok(stored
            .into_iter()
            .map(|x| match x {
                Stored::Text(t) => PathBuf::from(t),
                Stored::Bytes(b) => PathBuf::from(OsString::from_vec(b)),
            })
            .collect())
    }
}

// ---------------------------------------------------------------- --last

/// The kept report as written (`report::write_json` without pairs).
#[derive(Deserialize)]
struct Kept {
    config: serde_json::Value,
    #[serde(default)]
    files_enumerated: usize,
    #[serde(default)]
    files_analysed: usize,
    #[serde(default)]
    failures: Vec<serde_json::Value>,
    #[serde(default)]
    runtime_seconds: f64,
    groups: Vec<KeptGroup>,
}

#[derive(Deserialize)]
struct KeptGroup {
    files: Vec<KeptRow>,
}

/// One row as `report::Row` writes it.
#[derive(Deserialize)]
struct KeptRow {
    path: String,
    path_bytes: Option<Vec<u8>>,
    role: String,
    action: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    size_bytes: Option<u64>,
    relation: Option<String>,
    aligned_points: Option<u32>,
    frame_overlap: Option<f32>,
    pixel_correlation: Option<f32>,
    mirrored: Option<bool>,
    inverted: Option<bool>,
    #[serde(default)]
    damaged: bool,
    kept_copy: Option<String>,
    kept_copy_bytes: Option<Vec<u8>>,
}

impl KeptRow {
    fn path(&self) -> PathBuf {
        use std::os::unix::ffi::OsStringExt;
        match &self.path_bytes {
            Some(b) => PathBuf::from(std::ffi::OsString::from_vec(b.clone())),
            None => PathBuf::from(&self.path),
        }
    }

    /// The kept file a DELETE rests on, if the row names one.
    fn kept_copy(&self) -> Option<PathBuf> {
        use std::os::unix::ffi::OsStringExt;
        match (&self.kept_copy_bytes, &self.kept_copy) {
            (Some(b), _) => Some(PathBuf::from(std::ffi::OsString::from_vec(b.clone()))),
            (None, Some(p)) => Some(PathBuf::from(p)),
            (None, None) => None,
        }
    }
}

/// What `--last` was asked for.
pub struct Rewrite<'a> {
    pub cache: Option<&'a Path>,
    pub output: Option<&'a Path>,
    pub format: Option<report::Format>,
    pub suggest: suggest::Mode,
}

/// `--last`: write the kept scan's report again, where `-o` says, in the
/// layout `--format` says, with `--suggest`'s rule. Files gone since the scan
/// are left out, as the window leaves them out: out of their groups, and a
/// group left with one file gone. Returns the lines to say about it.
pub fn rewrite(asked: &Rewrite) -> Result<Vec<String>> {
    let dir = dir(asked.cache);
    let notes: Notes = match std::fs::read(dir.join(NOTES)) {
        Ok(b) => serde_json::from_slice(&b).with_context(|| format!("could not read the kept scan in {}", dir.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("no scan is kept in {}: run a scan first (one run with --no-cache or --no-last is not kept)", dir.display())
        }
        Err(e) => return Err(e).with_context(|| format!("could not read the kept scan in {}", dir.display())),
    };
    let bytes = std::fs::read(dir.join(REPORT)).with_context(|| format!("could not read the kept scan in {}", dir.display()))?;
    let kept: Kept = serde_json::from_slice(&bytes).with_context(|| format!("could not read the kept scan in {}", dir.display()))?;
    let made_by = kept.config.get("suggest").and_then(|v| v.as_str()).unwrap_or("content");
    if asked.suggest == suggest::Mode::Content && made_by != "content" {
        anyhow::bail!("the kept scan's suggestions are the {made_by} rule's; --suggest content needs a new scan");
    }
    let min_correlation = kept.config.get("min_pixel_correlation").and_then(|v| v.as_f64()).map_or(0.6, |v| v as f32);
    let (mut out, files, facts, Rebuilt { gone, unheld }) = rebuild(kept, asked.suggest, min_correlation);
    out.config["suggest"] = format!("{:?}", asked.suggest).to_lowercase().into();

    let mut target = report::Target::of(asked.output, asked.format);
    target.pairs = false;
    report::write_with(&target, &out, &files, &facts)?;

    let when = ago(notes.finished);
    let mut lines = vec![match &target.sink {
        report::Sink::File(p) => format!("{} from the scan {when} -> {}", groups_said(out.groups.len()), p.display()),
        report::Sink::Stdout => format!("{} from the scan {when}", groups_said(out.groups.len())),
    }];
    if gone > 0 {
        lines.push(format!("{gone} {} gone since the scan {} left out", if gone == 1 { "file" } else { "files" }, if gone == 1 { "is" } else { "are" }));
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

/// What `rebuild` left out or changed: files gone since the scan, and the
/// deletions whose kept copy was among them, which are now REVIEW.
struct Rebuilt {
    gone: usize,
    unheld: usize,
}

/// The kept report as the run held it: the files numbered as they first
/// appear, each group's members with their pair with the representative, the
/// facts the rows stated, and the actions `mode` gives. The actions are
/// worked out over every file, and only then are the gone ones left out, so
/// that what a rule said of the rest is what it said with them (as in the
/// window) — except a deletion resting on a file that has gone, which no
/// longer has anything to rest on: the content rule's is REVIEW, and the
/// group rules judge only by representatives still there.
fn rebuild(kept: Kept, mode: suggest::Mode, min_correlation: f32) -> (Output, Vec<PathBuf>, report::FileFacts, Rebuilt) {
    let mut index: HashMap<PathBuf, usize> = HashMap::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stated: Vec<(usize, Option<(u32, u32)>, Option<u64>, bool)> = Vec::new();
    let mut content: Vec<Option<Action>> = Vec::new();
    let mut pairs: HashMap<(usize, usize), OutPair> = HashMap::new();
    let mut groups: Vec<OutGroup> = Vec::new();
    // Each file's place in each group, beside the group's representative.
    let mut seats: Vec<(usize, suggest::Seat)> = Vec::new();
    // Each DELETE's kept file, by path: it is numbered once every row is.
    let mut rests_on: Vec<(usize, PathBuf)> = Vec::new();
    for g in &kept.groups {
        let members: Vec<usize> = g
            .files
            .iter()
            .map(|r| {
                let p = r.path();
                *index.entry(p.clone()).or_insert_with(|| {
                    files.push(p);
                    stated.push((files.len() - 1, r.width.zip(r.height), r.size_bytes, r.damaged));
                    content.push(r.action.as_deref().and_then(action_of));
                    files.len() - 1
                })
            })
            .collect();
        let Some(rk) = g.files.iter().position(|r| r.role == "representative") else { continue };
        let rep = members[rk];
        for (r, &i) in g.files.iter().zip(&members) {
            if let Some(k) = r.kept_copy() {
                rests_on.push((i, k));
            }
            let correlation = match r.relation.as_deref() {
                Some("identical" | "same_pixels") => 1.0,
                _ => r.pixel_correlation.unwrap_or(0.0),
            };
            seats.push((rep, suggest::Seat { file: i, representative: i == rep, correlation }));
            if i == rep {
                continue;
            }
            let how = r.relation.as_deref().unwrap_or("direct");
            pairs.entry((i.min(rep), i.max(rep))).or_insert_with(|| OutPair {
                a: files[rep].display().to_string().into(),
                b: files[i].display().to_string().into(),
                a_bytes: None,
                b_bytes: None,
                aligned_points: r.aligned_points.unwrap_or(0),
                frame_overlap: r.frame_overlap.unwrap_or(0.0),
                pixel_correlation: r.pixel_correlation.unwrap_or(0.0),
                scale: 0.0,
                mirrored: r.mirrored.unwrap_or(false),
                inverted: r.inverted.unwrap_or(false),
                identical: how == "identical",
                same_pixels: how == "same_pixels",
                propagated: how == "propagated",
                corroborated: how == "corroborated",
                ia: rep,
                ib: i,
            });
        }
        groups.push(OutGroup {
            representative: files[rep].display().to_string(),
            files: members.iter().map(|&i| files[i].display().to_string()).collect(),
            rep,
            members,
        });
    }
    let gone: HashSet<usize> = (0..files.len())
        .filter(|&i| std::fs::symlink_metadata(&files[i]).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound))
        .collect();
    let mut unheld = 0;
    let (actions, kept_by) = match mode {
        suggest::Mode::Content => {
            // A kept file is in a group with what it holds, so it has a number.
            let mut kept_by = vec![None; files.len()];
            for (i, k) in rests_on {
                kept_by[i] = index.get(&k).copied();
            }
            // To the end of each chain: a scan kept by an earlier version names the
            // nearest holder, which may be a deletion too.
            let mut kept_by = suggest::kept_at_end(&content, &kept_by);
            // A deletion whose kept copy has gone since the scan rests on
            // nothing: carried out, it would take the last copy of what it
            // shows. It is left for a person to look at.
            for i in 0..files.len() {
                if content[i] == Some(Action::Delete) && kept_by[i].is_some_and(|k| gone.contains(&k)) {
                    content[i] = Some(Action::Review);
                    kept_by[i] = None;
                    unheld += 1;
                }
            }
            (content, kept_by)
        }
        // The other two rules judge a file by the representatives it was
        // matched to, so a group whose representative has gone says nothing:
        // its files were never compared with each other, and judged by it, a
        // group's every remaining file was DELETE, resting on a file no
        // longer there. A file it alone held has no suggestion.
        _ => {
            let seats = seats.into_iter().filter(|(rep, _)| !gone.contains(rep)).map(|(_, s)| s);
            (suggest::by_group(mode, files.len(), seats, min_correlation), Vec::new())
        }
    };
    // A group keeps its files that are still there, and is left out once it
    // holds fewer than two, or no longer holds the file every other was
    // compared with: its rows would claim nothing about each other.
    groups.retain_mut(|g| {
        let kept: Vec<usize> = (0..g.members.len()).filter(|&k| !gone.contains(&g.members[k])).collect();
        g.files = kept.iter().map(|&k| g.files[k].clone()).collect();
        g.members = kept.iter().map(|&k| g.members[k]).collect();
        g.members.len() >= 2 && !gone.contains(&g.rep)
    });
    let out = Output {
        tool: "img-fp",
        config: kept.config,
        files_enumerated: kept.files_enumerated,
        files_analysed: kept.files_analysed,
        failures: kept.failures,
        runtime_seconds: kept.runtime_seconds,
        groups,
        pairs: pairs.into_values().collect(),
        actions,
        kept: kept_by,
    };
    (out, files, report::stated_facts(stated), Rebuilt { gone: gone.len(), unheld })
}

fn action_of(s: &str) -> Option<Action> {
    match s {
        "KEEP" => Some(Action::Keep),
        "DELETE" => Some(Action::Delete),
        "REVIEW" => Some(Action::Review),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    fn pair(ia: usize, ib: usize, files: &[PathBuf]) -> OutPair {
        OutPair {
            a: files[ia].display().to_string().into(),
            b: files[ib].display().to_string().into(),
            a_bytes: None,
            b_bytes: None,
            aligned_points: 42,
            frame_overlap: 0.987,
            pixel_correlation: 0.912,
            scale: 0.5,
            mirrored: false,
            inverted: false,
            identical: false,
            same_pixels: false,
            propagated: false,
            corroborated: false,
            ia,
            ib,
        }
    }

    /// The kept report, read back, writes the text and the CSV the scan
    /// wrote, byte for byte, with every relation, a name that is not UTF-8,
    /// a damaged file and a file in two groups; and a file deleted since is
    /// left out, and with it a group it left with one file.
    #[test]
    fn a_kept_scan_writes_the_reports_the_scan_wrote() {
        let root = std::env::temp_dir().join(format!("img-fp-keep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut files: Vec<PathBuf> = ["a.jpg", "b;x.jpg", "c.png", "d.png", "e.png"].iter().map(|n| root.join(n)).collect();
        files.push(root.join(std::ffi::OsString::from_vec(b"f\xff.jpg".to_vec())));
        for f in &files {
            std::fs::write(f, b"x").unwrap();
        }
        let mut pairs = vec![pair(0, 1, &files), pair(0, 2, &files), pair(0, 3, &files), pair(4, 3, &files), pair(4, 5, &files)];
        pairs[0].identical = true;
        pairs[1].propagated = true;
        pairs[1].mirrored = true;
        pairs[2].corroborated = true;
        pairs[2].inverted = true;
        pairs[3].same_pixels = true;
        let group = |rep: usize, members: Vec<usize>| OutGroup {
            representative: files[rep].display().to_string(),
            files: members.iter().map(|&i| files[i].display().to_string()).collect(),
            rep,
            members,
        };
        let out = Output {
            tool: "img-fp",
            config: serde_json::json!({"suggest": "content", "min_pixel_correlation": 0.6}),
            files_enumerated: 6,
            files_analysed: 6,
            failures: vec![],
            runtime_seconds: 1.5,
            groups: vec![group(0, vec![0, 1, 2, 3]), group(4, vec![3, 4, 5])],
            pairs,
            actions: vec![Some(Action::Keep), Some(Action::Delete), Some(Action::Review), Some(Action::Delete), Some(Action::Keep), None],
            kept: vec![None, Some(0), None, Some(4), None, None],
        };
        let dims = [(640, 480), (640, 480), (320, 240), (0, 0), (10, 10), (8, 8)];
        let facts = report::read_facts(&out, &files, &dims, &[false, false, true, false, false, false], &[1, 2, 3, 4, 5, 6]);
        let write = |out: &Output, files: &[PathBuf], facts: &report::FileFacts, name: &str, format| {
            let p = root.join(name);
            report::write_with(&report::Target { sink: report::Sink::File(p.clone()), format, pairs: false }, out, files, facts).unwrap();
            std::fs::read(p).unwrap()
        };
        let kept_json = write(&out, &files, &facts, "kept.json", report::Format::Json);
        let kept: Kept = serde_json::from_slice(&kept_json).unwrap();
        let (again, again_files, again_facts, done) = rebuild(kept, suggest::Mode::Content, 0.6);
        assert_eq!((done.gone, done.unheld), (0, 0));
        for (name, format) in [("txt", report::Format::Txt), ("csv", report::Format::Csv), ("json", report::Format::Json)] {
            assert_eq!(
                write(&again, &again_files, &again_facts, &format!("again.{name}"), format),
                write(&out, &files, &facts, &format!("first.{name}"), format),
                "{name}"
            );
        }

        // The other rules, from the groups alone.
        let kept: Kept = serde_json::from_slice(&kept_json).unwrap();
        let (again, again_files, _, _) = rebuild(kept, suggest::Mode::Representative, 0.6);
        let of = |p: &PathBuf| again.actions[again_files.iter().position(|f| f == p).unwrap()];
        assert_eq!(of(&files[0]), Some(Action::Keep));
        assert_eq!(of(&files[2]), Some(Action::Delete));
        assert_eq!(of(&files[5]), Some(Action::Delete));

        // `e.png` deleted: its group is left with two files, but not the one
        // they were compared with, so it goes; `d.png` stays in the first.
        std::fs::remove_file(&files[4]).unwrap();
        let kept: Kept = serde_json::from_slice(&kept_json).unwrap();
        let (again, again_files, _, done) = rebuild(kept, suggest::Mode::Content, 0.6);
        assert_eq!(done.gone, 1);
        assert_eq!(again.groups.len(), 1);
        assert_eq!(again.groups[0].members.len(), 4);
        // `d.png` was deleted on `e.png`'s strength, and `e.png` is gone: it
        // rests on nothing now, and is for a person to look at.
        assert_eq!(done.unheld, 1);
        let d = again_files.iter().position(|f| f == &files[3]).unwrap();
        assert_eq!((again.actions[d], again.kept[d]), (Some(Action::Review), None));
        // `b;x.jpg` rests on `a.jpg`, which is still there.
        let b = again_files.iter().position(|f| f == &files[1]).unwrap();
        assert_eq!(again.actions[b], Some(Action::Delete));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A kept scan from before the kept copy was the end of its chain names
    /// the nearest holder: read back, the end is found, and a deletion whose
    /// end has gone is REVIEW even though the nearest holder is still there.
    #[test]
    fn a_deletion_whose_kept_copy_is_gone_is_left_for_review() {
        let root = std::env::temp_dir().join(format!("img-fp-unheld-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let files: Vec<PathBuf> = ["keep.jpg", "mid.jpg", "far.jpg"].iter().map(|n| root.join(n)).collect();
        for f in &files {
            std::fs::write(f, b"x").unwrap();
        }
        let out = Output {
            tool: "img-fp",
            config: serde_json::json!({"suggest": "content", "min_pixel_correlation": 0.6}),
            files_enumerated: 3,
            files_analysed: 3,
            failures: vec![],
            runtime_seconds: 1.0,
            groups: vec![OutGroup {
                representative: files[0].display().to_string(),
                files: files.iter().map(|f| f.display().to_string()).collect(),
                rep: 0,
                members: vec![0, 1, 2],
            }],
            pairs: vec![pair(0, 1, &files), pair(0, 2, &files)],
            actions: vec![Some(Action::Keep), Some(Action::Delete), Some(Action::Delete)],
            // `far` rests on `mid`, which rests on `keep`.
            kept: vec![None, Some(0), Some(1)],
        };
        let facts = report::read_facts(&out, &files, &[(8, 8); 3], &[false; 3], &[1; 3]);
        let p = root.join("kept.json");
        report::write_with(&report::Target { sink: report::Sink::File(p.clone()), format: report::Format::Json, pairs: false }, &out, &files, &facts).unwrap();
        let read = || serde_json::from_slice::<Kept>(&std::fs::read(&p).unwrap()).unwrap();

        let (again, again_files, _, done) = rebuild(read(), suggest::Mode::Content, 0.6);
        let at = |f: &PathBuf| again_files.iter().position(|g| g == f).unwrap();
        assert_eq!(done.unheld, 0);
        assert_eq!(again.kept[at(&files[2])], Some(at(&files[0])), "the end of the chain, not the nearest holder");

        std::fs::remove_file(&files[0]).unwrap();
        let (again, again_files, _, done) = rebuild(read(), suggest::Mode::Content, 0.6);
        let at = |f: &PathBuf| again_files.iter().position(|g| g == f).unwrap();
        assert_eq!(done.unheld, 2);
        for f in &files[1..] {
            assert_eq!(again.actions[at(f)], Some(Action::Review), "{}", f.display());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The group rules judge a file only by representatives still there: a
    /// file that agreed closely with one now gone and loosely with another
    /// is judged by the other, and one only a gone representative held has
    /// no suggestion.
    #[test]
    fn the_group_rules_judge_by_representatives_still_there() {
        let root = std::env::temp_dir().join(format!("img-fp-groupgone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let files: Vec<PathBuf> = ["gone.jpg", "both.jpg", "only.jpg", "stays.jpg"].iter().map(|n| root.join(n)).collect();
        for f in &files {
            std::fs::write(f, b"x").unwrap();
        }
        let corr = |ia, ib, c| OutPair { pixel_correlation: c, ..pair(ia, ib, &files) };
        let group = |rep: usize, members: Vec<usize>| OutGroup {
            representative: files[rep].display().to_string(),
            files: members.iter().map(|&i| files[i].display().to_string()).collect(),
            rep,
            members,
        };
        let out = Output {
            tool: "img-fp",
            config: serde_json::json!({"suggest": "content", "min_pixel_correlation": 0.6}),
            files_enumerated: 4,
            files_analysed: 4,
            failures: vec![],
            runtime_seconds: 1.0,
            groups: vec![group(0, vec![0, 1, 2]), group(3, vec![1, 3])],
            pairs: vec![corr(0, 1, 0.99), corr(0, 2, 0.99), corr(1, 3, 0.7)],
            actions: vec![Some(Action::Keep), Some(Action::Delete), Some(Action::Delete), Some(Action::Keep)],
            kept: vec![None, Some(0), Some(0), None],
        };
        let facts = report::read_facts(&out, &files, &[(8, 8); 4], &[false; 4], &[1; 4]);
        let p = root.join("kept.json");
        report::write_with(&report::Target { sink: report::Sink::File(p.clone()), format: report::Format::Json, pairs: false }, &out, &files, &facts).unwrap();
        let read = || serde_json::from_slice::<Kept>(&std::fs::read(&p).unwrap()).unwrap();
        let (again, again_files, _, _) = rebuild(read(), suggest::Mode::Correlation, 0.6);
        let at = |f: &PathBuf| again_files.iter().position(|g| g == f).unwrap();
        assert_eq!(again.actions[at(&files[1])], Some(Action::Delete), "with its representative, by its best agreement");

        std::fs::remove_file(&files[0]).unwrap();
        for mode in [suggest::Mode::Correlation, suggest::Mode::Representative] {
            let (again, again_files, _, _) = rebuild(read(), mode, 0.6);
            let at = |f: &PathBuf| again_files.iter().position(|g| g == f).unwrap();
            let want = if mode == suggest::Mode::Correlation { Some(Action::Keep) } else { Some(Action::Delete) };
            assert_eq!(again.actions[at(&files[1])], want, "{mode:?}: judged by the representative still there");
            assert_eq!(again.actions[at(&files[2])], None, "{mode:?}: only the gone one held it");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_scan_is_kept_beside_the_cache_it_used() {
        assert_eq!(dir(Some(Path::new("/x/y/c.bin"))), Path::new("/x/y/last-scan"));
        assert_eq!(dir(Some(Path::new("/x/y/"))), Path::new("/x/y/last-scan"));
    }
}
