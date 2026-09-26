//! Turning the roots on the command line into the list of files to analyse.
//! The roots are the paths named, plus any read from a list — `--from-file`,
//! or `-` for stdin — one a line, or NUL-separated under `-0`; a listed path
//! is exactly a named one from then on.
//!
//! A root that is a file is taken whatever it is called: naming it is asking
//! for it. A root that is a directory is walked — the images directly inside
//! it, or everything below it under `-r` — and a walk has to *guess* which of
//! its files are images, which is what `-x` is for.
//!
//! **What comes back is one path per set of bytes, not one per name.** Files
//! are deduplicated on (device, inode), because this tool's first pass groups
//! byte-identical files, and two names for one file are byte-identical by
//! construction: a symlink and its target, two hard links, two overlapping
//! roots, a path typed twice. Reading both would report a duplicate pair that
//! is really one file, and deleting either "copy" frees nothing. Which name is
//! kept is decided in [`settle`], and it is not simply the first.
//!
//! **Symlinks met during a walk are not followed unless `--follow-symlinks`
//! asks.** A path *named* on the command line is followed either way. Under
//! the flag a link to a file is that file and a link to a directory is walked,
//! a link back into a folder already being walked is a loop and is skipped
//! (everything behind it is being walked already, so nothing is missing), and a
//! link to nothing is a problem, because it is the shape of a link into a drive
//! that is not mounted — which is also why it stops `--prune-cache`.
//!
//! **`--exclude` is about bytes, not spellings.** It takes a folder or a file,
//! and applies to a root named outright as much as to anything a walk finds.
//! Both sides of the comparison are canonical, because that is the only name
//! every route to a file agrees on: under `--follow-symlinks`,
//! `scan/linkdir/a.jpg` and `keep/a.jpg` are the same file and share not one
//! path component, so a test against the path the walk spelled could never
//! fire — and it fails in both directions, since excluding the link path the
//! user can see in the report canonicalizes into the real folder and matches
//! nothing either. `vid-fp` shipped exactly that bug and paid for it with a
//! deleted file; see `is_excluded_target` in its `sources.rs`. What it costs
//! is kept off the default path: a walk that follows no links produces paths
//! whose canonical form is the root's plus what follows it, so only a walk
//! that follows links ever asks the filesystem where a path leads.

use crate::extensions::Wanted;
use crate::problems::Problems;
use anyhow::Context;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Everything about the request that decides which files are analysed.
pub struct Request<'a> {
    pub roots: &'a [PathBuf],
    pub exclude: &'a [PathBuf],
    pub wanted: &'a Wanted,
    pub recursive: bool,
    pub follow_symlinks: bool,
}

/// What one set of bytes is known by: its inode where there is one.
#[derive(PartialEq, Eq, Hash)]
enum Identity {
    #[cfg_attr(not(unix), allow(dead_code))]
    Inode(u64, u64),
    #[cfg_attr(unix, allow(dead_code))]
    Path(PathBuf),
}

#[cfg(unix)]
fn identity(_: &Path, meta: &std::fs::Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt;
    Identity::Inode(meta.dev(), meta.ino())
}

#[cfg(not(unix))]
fn identity(path: &Path, _: &std::fs::Metadata) -> Identity {
    Identity::Path(std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

/// A name the walk offered, before the names for one file are settled.
struct Found {
    path: PathBuf,
    id: Identity,
    /// The entry itself is a symlink — not merely reached through a linked
    /// folder, which names the file just as well. See [`settle`].
    link: bool,
}

/// Every file the request reaches, one path per set of bytes, sorted.
///
/// Everything it passes over it counts, in one of two senses that must not be
/// confused. What it cannot *read* is a problem: a root that does not exist, a
/// directory refused part-way through, a link to nothing under
/// `--follow-symlinks`. What it was never going to read is a skip: a file `-x`
/// does not take, a symlink it does not follow, a loop, a second name for a
/// file already listed, a root `--exclude` covers. None of the skips touches
/// the exit code — which is the whole reason the summary keeps two lists.
pub fn walk(req: &Request, problems: &mut Problems) -> Vec<PathBuf> {
    let excludes = resolve_excludes(req.exclude, problems);
    let depth = if req.recursive { usize::MAX } else { 1 };
    let mut found = Vec::new();
    for root in req.roots {
        let shown = root.display().to_string();
        // A root that is not there, or a link to nothing, is loud: a mistyped
        // path must not be a run that quietly scans one directory of the two
        // it was given.
        let (meta, canon) = match std::fs::metadata(root).and_then(|m| Ok((m, std::fs::canonicalize(root)?))) {
            Ok(found) => found,
            Err(e) => {
                problems.unscannable(&shown, &e);
                continue;
            }
        };
        if is_excluded(&canon, &excludes) {
            problems.excluded(&shown);
            continue;
        }
        if meta.is_file() {
            let link = std::fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink());
            found.push(Found {
                id: identity(root, &meta),
                path: root.clone(),
                link,
            });
        } else if meta.is_dir() {
            walk_dir(root, &canon, depth, req, &excludes, &mut found, problems);
        } else {
            // A socket, a fifo, a device node: naming one is a mistake worth
            // hearing about rather than a file worth trying to decode.
            problems.unscannable(&shown, &"not a file or a folder");
        }
    }
    settle(found, problems)
}

fn walk_dir(
    root: &Path,
    canon: &Path,
    depth: usize,
    req: &Request,
    excludes: &[PathBuf],
    found: &mut Vec<Found>,
    problems: &mut Problems,
) {
    let follow = req.follow_symlinks;
    // The path's canonical form when no link lies between it and the root,
    // which without `--follow-symlinks` is always: the root was canonicalized
    // and every component below it is a real directory the walk descended.
    let beneath = |p: &Path| canon.join(p.strip_prefix(root).unwrap_or(p));
    // A directory is tested where the walk meets it, so an excluded subtree
    // is pruned whole rather than rejected file by file. Without the flag a
    // link to a directory is a symlink entry, not a directory, and is skipped
    // below before anything is asked of it.
    let entries = WalkDir::new(root)
        .follow_links(follow)
        .max_depth(depth)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_type().is_dir() || !leads_into(e.path(), &beneath(e.path()), excludes, follow));
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                let at = e.path().unwrap_or(root).display().to_string();
                if e.loop_ancestor().is_some() {
                    problems.symlink_loop(&at);
                } else {
                    problems.unscannable(&at, &e);
                }
                continue;
            }
        };
        // Under `--follow-symlinks` this is the target's type, so a link is
        // only ever seen here when links are not being followed.
        let kind = entry.file_type();
        if kind.is_symlink() {
            problems.symlink(&entry.path().display().to_string());
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        if !req.wanted.accepts(entry.path()) {
            problems.not_an_image(&entry.path().display().to_string());
            continue;
        }
        // Again for the file itself, which is what catches a link to a file
        // under an excluded folder, and an `--exclude` naming one file. Not
        // counted: `excluded` says "named", a file the walk found was not, and
        // an excluded subtree pruned above contributes nothing to any count
        // however many files are behind it.
        if leads_into(entry.path(), &beneath(entry.path()), excludes, follow) {
            continue;
        }
        // One stat, after the filters have thinned the list: it follows a
        // followed link, so the identity is the target's.
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                problems.unscannable(&entry.path().display().to_string(), &e);
                continue;
            }
        };
        found.push(Found {
            id: identity(entry.path(), &meta),
            link: entry.path_is_symlink(),
            path: entry.into_path(),
        });
    }
}

/// One name per set of bytes, and which one.
///
/// **A real name is preferred to a symlink, whatever the order.** `vid-fp`
/// took the first name the walk offered, so a folder holding `a.mp4` and a
/// link to it offered two names in readdir order — the link could win, and a
/// user deleting the "duplicate" the report named removed a shortcut and
/// freed nothing. Here the preference is the same and the order is not even
/// readdir's: between two equally good names (two hard links, two links, a
/// file and the same file through a linked folder) the smaller path wins,
/// which makes the list, and so the run, independent of directory order.
///
/// Exactly one name is counted per collision either way.
fn settle(mut found: Vec<Found>, problems: &mut Problems) -> Vec<PathBuf> {
    found.sort_by(|a, b| a.path.cmp(&b.path));
    // (path, is a link) per set of bytes, and where each set is in it.
    let mut kept: Vec<(PathBuf, bool)> = Vec::with_capacity(found.len());
    let mut held: HashMap<Identity, usize> = HashMap::with_capacity(found.len());
    for f in found {
        let Some(&at) = held.get(&f.id) else {
            held.insert(f.id, kept.len());
            kept.push((f.path, f.link));
            continue;
        };
        let dropped = if kept[at].1 && !f.link {
            std::mem::replace(&mut kept[at], (f.path, false)).0
        } else {
            f.path
        };
        problems.listed_twice(&dropped.display().to_string());
    }
    // A later real name may have displaced an earlier link.
    let mut files: Vec<PathBuf> = kept.into_iter().map(|(path, _)| path).collect();
    files.sort();
    files
}

/// Canonicalize the `--exclude` list, so the prefix test is a test of bytes.
///
/// A path that will not resolve excludes nothing, and that is a problem rather
/// than a detail: `vid-fp` once swallowed it, so a typo in `-e` scanned the
/// folder the user was protecting.
fn resolve_excludes(requested: &[PathBuf], problems: &mut Problems) -> Vec<PathBuf> {
    let mut excludes = Vec::with_capacity(requested.len());
    for p in requested {
        match std::fs::canonicalize(p) {
            Ok(real) => excludes.push(real),
            Err(e) => problems.unresolved_exclude(&p.display().to_string(), &e),
        }
    }
    excludes
}

/// Component-wise, so `-e photos/take` is not `photos/take2.jpg`.
fn is_excluded(path: &Path, excludes: &[PathBuf]) -> bool {
    excludes.iter().any(|ex| path.starts_with(ex))
}

/// Is this walk path, or what it leads to, under an `--exclude`?
///
/// `canonical_guess` answers when no link was followed to get here; a walk
/// that follows links asks the filesystem, which is the one case where the
/// guess can be wrong. A path that will not canonicalize is not excluded, and
/// needs no protecting: it cannot be opened, so it cannot be analysed.
fn leads_into(path: &Path, canonical_guess: &Path, excludes: &[PathBuf], through_links: bool) -> bool {
    if excludes.is_empty() {
        return false;
    }
    if is_excluded(canonical_guess, excludes) {
        return true;
    }
    through_links && std::fs::canonicalize(path).is_ok_and(|real| is_excluded(&real, excludes))
}

/// Where the roots come from: the command line, and the lists it points at.
pub struct Sources<'a> {
    /// The positional paths. `-` among them reads a list from stdin.
    pub named: &'a [PathBuf],
    /// `--from-file`: a list of paths, `-` for stdin.
    pub from_file: Option<&'a Path>,
    /// `-0`: the lists are NUL-separated rather than one path a line.
    pub null_separated: bool,
}

/// Every root the user asked for, with `-` and `--from-file` expanded, and
/// where each list came from with how many paths it held, for the header.
///
/// Stdin is read at most once however many times it is asked for: `img-fp - -`,
/// or `-` with `--from-file -`, is a typo rather than a request to read the
/// pipe twice. A list that cannot be opened or read is fatal — it is the whole
/// of what the run was asked to scan, and a run over nothing would exit 0.
pub fn requested_roots(sources: &Sources, problems: &mut Problems) -> anyhow::Result<(Vec<PathBuf>, Vec<(String, usize)>)> {
    let stdin = Path::new("-");
    let mut roots = Vec::new();
    let mut lists = Vec::new();
    let mut stdin_taken = false;
    let mut take_stdin = |roots: &mut Vec<PathBuf>, lists: &mut Vec<(String, usize)>, problems: &mut Problems| -> anyhow::Result<()> {
        if std::mem::replace(&mut stdin_taken, true) {
            return Ok(());
        }
        let paths = read_stdin(sources.null_separated, problems)?;
        lists.push(("stdin".to_string(), paths.len()));
        roots.extend(paths);
        Ok(())
    };
    for path in sources.named {
        if path == stdin {
            take_stdin(&mut roots, &mut lists, problems)?;
        } else {
            roots.push(path.clone());
        }
    }
    if let Some(list) = sources.from_file {
        if list == stdin {
            take_stdin(&mut roots, &mut lists, problems)?;
        } else {
            let file = std::fs::File::open(list)
                .with_context(|| format!("could not open the path list {}", list.display()))?;
            let paths = read_path_list(file, sources.null_separated, problems)
                .with_context(|| format!("could not read the path list {}", list.display()))?;
            lists.push((list.display().to_string(), paths.len()));
            roots.extend(paths);
        }
    }
    Ok((roots, lists))
}

fn read_stdin(null_separated: bool, problems: &mut Problems) -> anyhow::Result<Vec<PathBuf>> {
    use std::io::IsTerminal;
    // Without this, `img-fp -` at a prompt looks exactly like a hang.
    if std::io::stdin().is_terminal() {
        anyhow::bail!(
            "asked to read paths from stdin, but stdin is a terminal. \
             Pipe a list in (e.g. `fd -e jpg | img-fp -`), or name folders as arguments."
        );
    }
    read_path_list(std::io::stdin().lock(), null_separated, problems).context("could not read the path list from stdin")
}

/// Read a whole path list.
///
/// Read as bytes, and on Unix a path is its bytes: a filename that is not
/// UTF-8 is still a filename, and the roots are paths rather than strings, so
/// nothing here has to turn one away. (`vid-fp` holds its paths as strings and
/// skips such an entry; img-fp never had to.)
fn read_path_list<R: std::io::Read>(mut reader: R, null_separated: bool, problems: &mut Problems) -> std::io::Result<Vec<PathBuf>> {
    let mut raw = Vec::new();
    reader.read_to_end(&mut raw)?;
    Ok(split_path_list(&raw, null_separated).into_iter().filter_map(|entry| path_of(entry, problems)).collect())
}

#[cfg(unix)]
fn path_of(entry: &[u8], _: &mut Problems) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(entry)))
}

#[cfg(not(unix))]
fn path_of(entry: &[u8], problems: &mut Problems) -> Option<PathBuf> {
    match std::str::from_utf8(entry) {
        Ok(s) => Some(PathBuf::from(s)),
        Err(e) => {
            problems.unscannable(&String::from_utf8_lossy(entry), &e);
            None
        }
    }
}

/// Split a list on newlines, or on NUL bytes when asked. Blank entries are
/// dropped.
///
/// A trailing carriage return is trimmed in newline mode. A list authored on
/// Windows would otherwise fail every single path with "No such file", and the
/// byte responsible is invisible in the message — the worst kind of failure to
/// debug. `-0` exists for anyone who needs the bytes untouched, and it is the
/// only way to pass a filename containing a newline.
fn split_path_list(raw: &[u8], null_separated: bool) -> Vec<&[u8]> {
    let separator = if null_separated { b'\0' } else { b'\n' };
    raw.split(|&b| b == separator)
        .map(|entry| if null_separated { entry } else { entry.strip_suffix(b"\r").unwrap_or(entry) })
        .filter(|entry| !entry.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problems::Log;
    use std::fs;
    use std::os::unix::fs::symlink;

    /// A directory that removes itself, since the crate has no `tempfile`.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let p = std::env::temp_dir().join(format!("img-fp-walk-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            // Canonical, so the expected paths below compare equal to the
            // walked ones on a machine whose temp dir is itself a link.
            Scratch(fs::canonicalize(&p).unwrap())
        }

        fn dir(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(&p).unwrap();
            p
        }

        fn file(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, rel.as_bytes()).unwrap();
            p
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct Run {
        files: Vec<PathBuf>,
        problems: usize,
        walk_complete: bool,
    }

    fn run(roots: &[PathBuf], exclude: &[PathBuf], recursive: bool, follow: bool) -> Run {
        let (wanted, _) = crate::extensions::normalize(&["jpg".to_string()]).unwrap();
        let log = Log::default();
        let mut problems = Problems::new(&log);
        let files = walk(
            &Request {
                roots,
                exclude,
                wanted: &wanted,
                recursive,
                follow_symlinks: follow,
            },
            &mut problems,
        );
        Run {
            files,
            problems: problems.count(),
            walk_complete: problems.walk_was_complete(),
        }
    }

    #[test]
    fn a_symlink_is_not_followed_unless_asked() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        let b = s.file("elsewhere/b.jpg");
        symlink(s.0.join("elsewhere"), s.0.join("scan/linkdir")).unwrap();
        symlink(&b, s.0.join("scan/link.jpg")).unwrap();
        let roots = [s.0.join("scan")];

        assert_eq!(run(&roots, &[], true, false).files, vec![a.clone()]);
        // Both links lead to `b`, which is one file and is listed once, under
        // the name that is not itself a link.
        assert_eq!(run(&roots, &[], true, true).files, vec![a, s.0.join("scan/linkdir/b.jpg")]);
    }

    #[test]
    fn a_link_and_its_target_are_one_file_not_a_duplicate_pair() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        // Named so that the link sorts first: the order that lost in vid-fp.
        symlink(&a, s.0.join("scan/0-link.jpg")).unwrap();
        let r = run(&[s.0.join("scan")], &[], false, true);
        assert_eq!(r.files, vec![a], "the real name, however the names sort");
        assert_eq!(r.problems, 0, "a second name is a skip");
    }

    #[test]
    fn a_named_link_does_not_stand_in_for_the_file_a_walk_finds() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        let link = s.0.join("0-link.jpg");
        symlink(&a, &link).unwrap();
        // Followed because it was named, and then settled against the walk:
        // without identity it was a byte-identical "pair" of one file.
        let r = run(&[link.clone(), s.0.join("scan")], &[], false, false);
        assert_eq!(r.files, vec![a]);
        // Alone, it is the only name there is and it is kept.
        assert_eq!(run(&[link.clone()], &[], false, false).files, vec![link]);
    }

    #[test]
    fn hard_links_and_overlapping_roots_are_listed_once() {
        let s = Scratch::new();
        let a = s.file("scan/sub/a.jpg");
        fs::hard_link(&a, s.0.join("scan/b.jpg")).unwrap();
        let r = run(&[s.0.join("scan"), s.0.join("scan/sub"), a.clone()], &[], true, false);
        assert_eq!(r.files, vec![s.0.join("scan/b.jpg")], "the smaller of two equally real names");
    }

    #[test]
    fn a_symlink_loop_is_a_skip_and_everything_else_is_still_found() {
        let s = Scratch::new();
        let a = s.file("scan/sub/a.jpg");
        symlink(s.0.join("scan"), s.0.join("scan/sub/up")).unwrap();
        let r = run(&[s.0.join("scan")], &[], true, true);
        assert_eq!(r.files, vec![a]);
        assert_eq!(r.problems, 0, "nothing behind a loop is missing");
        assert!(r.walk_complete);
    }

    #[test]
    fn a_dangling_link_is_a_problem_when_followed_and_stops_a_prune() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        symlink(s.0.join("unmounted/drive"), s.0.join("scan/photos")).unwrap();
        let followed = run(&[s.0.join("scan")], &[], true, true);
        assert_eq!(followed.files, vec![a.clone()]);
        assert_eq!(followed.problems, 1);
        assert!(!followed.walk_complete, "--prune-cache must not prune against it");
        let not_followed = run(&[s.0.join("scan")], &[], true, false);
        assert_eq!((not_followed.files, not_followed.problems), (vec![a], 0));
    }

    #[test]
    fn an_exclude_prunes_a_folder_and_can_name_one_file() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        s.file("scan/keep/b.jpg");
        let c = s.file("scan/c.jpg");
        let r = run(&[s.0.join("scan")], &[s.0.join("scan/keep"), c], true, false);
        assert_eq!(r.files, vec![a]);
        assert_eq!(r.problems, 0);
    }

    #[test]
    fn an_exclude_matches_whole_components() {
        let s = Scratch::new();
        let take = s.file("scan/take.jpg");
        s.file("scan/take/x.jpg");
        let r = run(&[s.0.join("scan")], &[s.0.join("scan/take")], true, false);
        assert_eq!(r.files, vec![take]);
    }

    #[test]
    fn an_exclude_outranks_a_root_named_outright() {
        let s = Scratch::new();
        let a = s.file("keep/a.jpg");
        let r = run(&[a.clone(), s.0.join("keep")], &[s.0.join("keep")], false, false);
        assert!(r.files.is_empty());
        assert_eq!(r.problems, 0, "doing what it was told is not a failure");
    }

    #[test]
    fn an_exclude_holds_for_relative_roots_and_relative_excludes() {
        // Roots are never canonicalized for the report, so the walk spells
        // `scan/keep/b.jpg` wherever the exclude resolved to.
        let s = Scratch::new();
        s.file("scan/a.jpg");
        s.file("scan/keep/b.jpg");
        let here = std::env::current_dir().unwrap();
        let rel = |p: &Path| pathdiff(p, &here);
        let r = run(&[rel(&s.0.join("scan"))], &[rel(&s.0.join("scan/./keep"))], true, false);
        assert_eq!(r.files, vec![rel(&s.0.join("scan/a.jpg"))]);
    }

    /// `a` relative to `base`, both absolute, through `..` as needed.
    fn pathdiff(a: &Path, base: &Path) -> PathBuf {
        let a: Vec<_> = a.components().collect();
        let b: Vec<_> = base.components().collect();
        let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let mut out = PathBuf::new();
        for _ in common..b.len() {
            out.push("..");
        }
        for c in &a[common..] {
            out.push(c);
        }
        out
    }

    #[test]
    fn an_exclude_protects_a_file_reached_through_a_linked_folder() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        s.file("keep/precious.jpg");
        symlink(s.0.join("keep"), s.0.join("scan/linkdir")).unwrap();
        let r = run(&[s.0.join("scan")], &[s.0.join("keep")], true, true);
        assert_eq!(r.files, vec![a]);
    }

    #[test]
    fn excluding_the_link_path_works_as_well_as_excluding_the_real_one() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        s.file("keep/precious.jpg");
        symlink(s.0.join("keep"), s.0.join("scan/linkdir")).unwrap();
        let r = run(&[s.0.join("scan")], &[s.0.join("scan/linkdir")], true, true);
        assert_eq!(r.files, vec![a]);
    }

    #[test]
    fn an_exclude_protects_a_file_reached_through_a_link_to_it() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        let precious = s.file("keep/precious.jpg");
        symlink(&precious, s.0.join("scan/link.jpg")).unwrap();
        let r = run(&[s.0.join("scan")], &[s.0.join("keep")], true, true);
        assert_eq!(r.files, vec![a]);
    }

    #[test]
    fn an_exclude_naming_one_file_reaches_it_through_a_link_too() {
        let s = Scratch::new();
        let precious = s.file("elsewhere/precious.jpg");
        s.file("elsewhere/other.jpg");
        symlink(s.0.join("elsewhere"), s.dir("scan").join("linkdir")).unwrap();
        let r = run(&[s.0.join("scan")], &[precious], true, true);
        assert_eq!(r.files, vec![s.0.join("scan/linkdir/other.jpg")]);
    }

    #[test]
    fn an_exclude_that_does_not_resolve_is_a_problem() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        let r = run(&[s.0.join("scan")], &[s.0.join("scan/kepe")], false, false);
        assert_eq!(r.files, vec![a], "it excluded nothing");
        assert_eq!(r.problems, 1, "and the run says so");
        assert!(r.walk_complete, "a scan that read more than meant to is still complete");
    }

    #[test]
    fn a_missing_root_is_a_problem() {
        let s = Scratch::new();
        let r = run(&[s.0.join("nope")], &[], false, false);
        assert!(r.files.is_empty());
        assert_eq!(r.problems, 1);
        assert!(!r.walk_complete);
    }

    #[test]
    fn subfolders_and_links_to_them_wait_for_recursive() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        s.file("scan/sub/b.jpg");
        s.file("elsewhere/c.jpg");
        symlink(s.0.join("elsewhere"), s.0.join("scan/linkdir")).unwrap();
        assert_eq!(run(&[s.0.join("scan")], &[], false, true).files, vec![a]);
    }

    #[test]
    fn a_list_is_split_on_newlines_and_blanks_are_ignored() {
        assert_eq!(split_path_list(b"/imgs/a.jpg\n\n/imgs/b.png\n", false), vec![&b"/imgs/a.jpg"[..], &b"/imgs/b.png"[..]]);
    }

    #[test]
    fn a_carriage_return_is_trimmed_rather_than_kept_in_the_path() {
        // A list authored on Windows. Keeping the \r fails every path with "No
        // such file", and the byte responsible does not show up in the message.
        assert_eq!(split_path_list(b"/imgs/a.jpg\r\n/imgs/b.png\r\n", false), vec![&b"/imgs/a.jpg"[..], &b"/imgs/b.png"[..]]);
    }

    #[test]
    fn a_null_separated_list_keeps_every_byte_of_the_filename() {
        // The reason -0 exists: both of these are legal Linux filenames.
        let raw = b"/imgs/two\nlines.jpg\0/imgs/trailing\r.jpg\0";
        assert_eq!(split_path_list(raw, true), vec![&b"/imgs/two\nlines.jpg"[..], &b"/imgs/trailing\r.jpg"[..]]);
    }

    #[test]
    fn a_path_that_is_not_utf8_is_still_a_path() {
        use std::os::unix::ffi::OsStrExt;
        let log = Log::default();
        let mut problems = Problems::new(&log);
        let paths = read_path_list(&b"/imgs/good.jpg\n/imgs/\xFF\xFEodd.jpg\n"[..], false, &mut problems).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[1].as_os_str().as_bytes(), b"/imgs/\xFF\xFEodd.jpg");
        assert_eq!(problems.count(), 0);
    }

    #[test]
    fn a_listed_file_is_walked_as_if_it_were_named() {
        let s = Scratch::new();
        let a = s.file("scan/a.jpg");
        let b = s.file("other/b.jpg");
        let odd = s.file("other/new\nline.jpg");
        let list = s.0.join("list");
        let raw = [a.as_os_str(), s.0.join("other").as_os_str(), odd.as_os_str()]
            .map(|p| p.to_str().unwrap().to_string())
            .join("\0");
        fs::write(&list, raw).unwrap();

        let log = Log::default();
        let mut problems = Problems::new(&log);
        let named = [a.clone()];
        let (roots, lists) = requested_roots(
            &Sources { named: &named, from_file: Some(&list), null_separated: true },
            &mut problems,
        )
        .unwrap();
        assert_eq!(lists, vec![(list.display().to_string(), 3)]);
        // `a` is named and listed: one file, listed once.
        let mut want = vec![a, b, odd];
        want.sort();
        assert_eq!(run(&roots, &[], false, false).files, want);
    }

    #[test]
    fn a_list_that_cannot_be_opened_is_fatal() {
        let log = Log::default();
        let mut problems = Problems::new(&log);
        let missing = Path::new("/nonexistent/img-fp/list");
        let err = requested_roots(&Sources { named: &[], from_file: Some(missing), null_separated: false }, &mut problems);
        assert!(err.is_err(), "a run over nothing would exit 0 and say nothing was found");
    }
}
