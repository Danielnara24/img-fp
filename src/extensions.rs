//! `-x` / `--extensions`: which files a walk hands to the decoder.
//!
//! Ported from `vid-fp`'s `sources.rs`, and it means the same thing there and
//! here, so a user who knows one knows the other. The default is the list of
//! extensions img-fp can decode (`decode::EXTENSIONS`); `-x` *replaces* it.
//! Case-insensitive, a leading `.` or `*.` optional. `-x '*'` takes every file
//! whatever it is called, which is the only way to reach a file with no
//! extension at all; `-x '!gif'` is every file but those, and
//! `-x 'jpg,png,!png'` is a list with one taken back out.
//!
//! What a walk hands over under a wildcard is identified by its bytes, as
//! every file is. One that turns out not to be a picture is a *skip* rather
//! than a problem — it never claimed to be an image, so nothing was asked of
//! it and nothing failed — and it is left out of the byte-identical groups,
//! since two copies of a README are not a duplicate image. A `.jpg` that is
//! not a picture is still a problem.

use anyhow::{bail, Result};
use std::collections::HashSet;
use std::path::Path;

/// The guess a folder walk makes about which of its files are images.
///
/// Shapes rather than one set, because "every file" is not expressible as a
/// list of suffixes: a file with no extension has nothing for
/// `Path::extension` to return, so no entry could ever name it. The third is
/// that same absence with a hole in it — a set of what to refuse and a set of
/// what to accept are not the same question, so they are not the same variant.
#[derive(Debug, PartialEq)]
pub enum Wanted {
    /// `-x '*'`. Every regular file the walk finds, extension or not.
    Anything,
    /// `-x '!gif'`. Every file except the ones named — and a file with no
    /// extension is not named by anything, so it is still taken.
    AnythingBut(HashSet<String>),
    /// Files whose extension is in this set, lowercased and dot-free.
    OneOf(HashSet<String>),
}

/// The wildcard, spelled the way a shell user expects. Quoting is on them —
/// unquoted it is a glob, and one that expands to the directory's contents.
const WILDCARD: &str = "*";

/// What turns an entry into an exception. An interactive bash expands `!` as
/// history unless it is in single quotes.
const NOT: char = '!';

impl Wanted {
    pub fn accepts(&self, path: &Path) -> bool {
        let extension = || path.extension().and_then(|s| s.to_str()).map(|e| e.to_lowercase());
        match self {
            Wanted::Anything => true,
            Wanted::AnythingBut(refused) => extension().is_none_or(|e| !refused.contains(e.as_str())),
            Wanted::OneOf(wanted) => extension().is_some_and(|e| wanted.contains(e.as_str())),
        }
    }

    /// Whether the walk turned files away by name, so that what it kept can
    /// be called images before a byte of them is read. False for both
    /// wildcard shapes: what came back is then simply files.
    pub fn is_a_guess_at_images(&self) -> bool {
        matches!(self, Wanted::OneOf(_))
    }

    /// The header line saying what the walk will take.
    pub fn describe(&self) -> String {
        match self {
            Wanted::Anything => "Searching every file, whatever its extension (-x '*').".into(),
            Wanted::AnythingBut(set) => {
                format!("Searching every file except these extensions: {:?}", sorted(set))
            }
            Wanted::OneOf(set) => format!("Searching extensions: {:?}", sorted(set)),
        }
    }
}

/// Whether a file's name says it is a picture — its extension is one img-fp
/// decodes — whatever `-x` asked for. A wildcard walk's file that is not a
/// picture is a skip only when this is false; a `.jpg` that is not one is
/// still a problem, because it claimed to be.
pub fn names_an_image(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| crate::decode::EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

/// HashSet iteration order is unspecified; sort for a stable line.
fn sorted(set: &HashSet<String>) -> Vec<&str> {
    let mut shown: Vec<&str> = set.iter().map(|s| s.as_str()).collect();
    shown.sort_unstable();
    shown
}

/// Read a `-x` list. The `String` alongside is a warning for the console,
/// present when an exception in a positive list took nothing away.
pub fn normalize(requested: &[String]) -> Result<(Wanted, Option<String>)> {
    let mut wanted: HashSet<String> = HashSet::new();
    let mut refused: HashSet<String> = HashSet::new();

    for entry in requested {
        let entry = entry.trim();
        let (into, entry) = match entry.strip_prefix(NOT) {
            Some(rest) => (&mut refused, rest.trim()),
            None => (&mut wanted, entry),
        };
        // `*.jpg` is how a shell user spells `jpg`, and taken literally it is
        // a suffix no file has — matching nothing, silently.
        let entry = entry.strip_prefix("*.").unwrap_or(entry);
        let entry = entry.trim_start_matches('.').to_lowercase();
        if !entry.is_empty() {
            into.insert(entry);
        }
    }

    // "Everything except everything" is the empty walk, and no user means it.
    if refused.contains(WILDCARD) {
        bail!("--extensions excludes every file (-x '!*' matches nothing).");
    }

    // A list that only says what it does not want means "every file but
    // those"; requiring the `*` beside it would be a spelling rule rather than
    // a distinction. And the wildcard wins over anything written beside it.
    if wanted.contains(WILDCARD) || (wanted.is_empty() && !refused.is_empty()) {
        return Ok((
            if refused.is_empty() { Wanted::Anything } else { Wanted::AnythingBut(refused) },
            None,
        ));
    }

    // `-x` REPLACES the default list, so someone meaning "the defaults minus
    // gif" who writes `-x 'jpg,!gif'` gets a one-extension walk, told only
    // that it is searching `["jpg"]`. The exception is the half of the request
    // that was silently dropped, so it is the half worth saying out loud.
    let mut inert: Vec<String> = refused.difference(&wanted).map(|e| format!("{NOT}{e}")).collect();
    inert.sort_unstable();
    let warning = (!inert.is_empty()).then(|| {
        format!(
            "Note: --extensions {:?} matched nothing in {:?} and took nothing away. -x REPLACES \
             the default list rather than narrowing it, so this walk is exactly that list; '{}' \
             on its own means every file except that one.",
            inert,
            sorted(&wanted),
            inert[0]
        )
    });

    wanted.retain(|e| !refused.contains(e));
    if wanted.is_empty() {
        bail!(
            "No extensions to search for (--extensions was empty, or every extension in it was \
             excluded)."
        );
    }
    Ok((Wanted::OneOf(wanted), warning))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(entries: &[&str]) -> Wanted {
        normalize(&entries.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap().0
    }

    fn takes(w: &Wanted, name: &str) -> bool {
        w.accepts(Path::new(name))
    }

    #[test]
    fn the_default_list_takes_images_and_nothing_bare() {
        let w = of(&crate::decode::EXTENSIONS);
        assert!(takes(&w, "a/photo.JPG"));
        assert!(takes(&w, "a/photo.heic"));
        assert!(!takes(&w, "a/notes.txt"));
        assert!(!takes(&w, "a/beach"));
    }

    #[test]
    fn spellings_of_one_extension_agree() {
        for spelt in ["png", ".png", "PNG", "*.png", " png "] {
            assert_eq!(of(&[spelt]), of(&["png"]), "{spelt}");
        }
    }

    #[test]
    fn the_wildcard_reaches_a_file_with_no_extension() {
        let w = of(&["*"]);
        assert_eq!(w, Wanted::Anything);
        assert!(takes(&w, "a/beach"));
        assert!(takes(&w, "a/notes.txt"));
        assert!(!w.is_a_guess_at_images());
    }

    #[test]
    fn the_wildcard_widens_whatever_it_is_written_beside() {
        assert_eq!(of(&["jpg", "*"]), Wanted::Anything);
    }

    #[test]
    fn an_exception_alone_is_every_file_but_that_one() {
        let w = of(&["!gif"]);
        assert!(takes(&w, "a/beach"));
        assert!(takes(&w, "a/photo.jpg"));
        assert!(!takes(&w, "a/anim.GIF"));
    }

    #[test]
    fn an_exception_takes_one_back_out_of_a_list() {
        let w = of(&["jpg", "png", "!png"]);
        assert!(takes(&w, "a.jpg"));
        assert!(!takes(&w, "a.png"));
    }

    #[test]
    fn an_exception_that_takes_nothing_away_is_said() {
        let (_, warning) = normalize(&["jpg".into(), "!gif".into()]).unwrap();
        assert!(warning.unwrap().contains("!gif"));
    }

    #[test]
    fn only_an_image_extension_names_an_image() {
        assert!(names_an_image(Path::new("a/b.JPEG")));
        assert!(!names_an_image(Path::new("a/beach")));
        assert!(!names_an_image(Path::new("a/notes.txt")));
    }

    #[test]
    fn walks_that_match_nothing_are_refused() {
        assert!(normalize(&["!*".into()]).is_err());
        assert!(normalize(&["jpg".into(), "!jpg".into()]).is_err());
        assert!(normalize(&["".into()]).is_err());
    }
}
