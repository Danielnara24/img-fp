//! Every mnemonic in the window, in one place.
//!
//! A mnemonic is the underlined letter Alt reaches, and two visible controls
//! sharing one make Alt cycle between them instead of going where it says. So
//! the labels live here, grouped by what is on screen together, and the test at
//! the bottom checks each group for clashes: the setup page's own controls with
//! either tab, the results page, and the preview.

// The setup page, whichever tab is showing. Scan and Cancel share a letter
// because only one of them is ever visible.
pub const SCAN: &str = "_Scan";
pub const CANCEL: &str = "Cancel _scan";
pub const TAB_GENERAL: &str = "_General";
pub const TAB_ADVANCED: &str = "Ad_vanced";
pub const SCAN_LOG: &str = "Scan _log";
pub const BACK_TO_RESULTS: &str = "_Back to results";
pub const DEFAULTS: &str = "Restore _defaults";

// The General tab.
pub const FOLDERS: &str = "_Folders to scan";
pub const ADD_FOLDER: &str = "_Add folder…";
pub const REMOVE_FOLDER: &str = "_Remove";
pub const RECURSIVE: &str = "Look _in subfolders";
pub const WORK_SIZE: &str = "_Work size";
pub const MIN_POINTS: &str = "Min aligned _points";
pub const MIN_OVERLAP: &str = "Min frame _overlap";
pub const MIN_CORRELATION: &str = "Min pi_xel correlation";
pub const CANDIDATES: &str = "Ca_ndidates per image";
pub const REPORT: &str = "Save a report _to";
pub const REPORT_CHOOSE: &str = "C_hoose…";
pub const REPORT_FORMAT: &str = "For_mat";

// The Advanced tab.
pub const THREADS: &str = "_Threads";
pub const EXTENSIONS: &str = "Extensi_ons";
pub const EXCLUDE: &str = "_Exclude";
pub const ADD_EXCLUDE: &str = "Add e_xcluded folder…";
pub const REMOVE_EXCLUDE: &str = "_Remove";
pub const SYMLINKS: &str = "Follow symlin_ks";
pub const HIDDEN: &str = "_Include hidden folders";
pub const USE_CACHE: &str = "_Use a cache";
pub const CACHE_FILE: &str = "_Cache file";
pub const CACHE_CHOOSE: &str = "C_hoose…";
pub const CLEAR_CACHE: &str = "Empt_y the cache before scanning";
pub const PRUNE_CACHE: &str = "_Prune what this scan does not use";
pub const LOG_FILE: &str = "Write a log _file";
pub const LOG_CHOOSE: &str = "Bro_wse…";

// The results page.
pub const GROUPS: &str = "_Groups";
pub const IMAGES: &str = "_Images";
pub const NEXT_GROUP: &str = "_Next group";
pub const PREV_GROUP: &str = "_Previous group";
pub const MARK_OTHERS: &str = "Mark all _except this";
pub const UNMARK_GROUP: &str = "_Unmark group";
pub const OPEN: &str = "_Open";
pub const SHOW_FOLDER: &str = "Show in _folder";
pub const TRASH: &str = "Move marked to _Trash…";
pub const NEW_SCAN: &str = "New _scan";
pub const RESULTS_LOG: &str = "Scan _log";

// The preview.
pub const PREVIEW_PREV: &str = "_Previous";
pub const PREVIEW_NEXT: &str = "_Next";
pub const PREVIEW_MARK: &str = "_Mark for Trash";
pub const PREVIEW_CLOSE: &str = "_Close";

// The Trash confirmation.
pub const CONFIRM_CANCEL: &str = "_Cancel";
pub const CONFIRM_TRASH: &str = "_Move to Trash";

#[cfg(test)]
mod tests {
    use super::*;

    fn letter(label: &str) -> char {
        let parts: Vec<&str> = label.split('_').collect();
        assert_eq!(parts.len(), 2, "{label:?} should have exactly one mnemonic");
        parts[1].chars().next().unwrap().to_ascii_lowercase()
    }

    fn distinct(scope: &str, labels: &[&str]) {
        let mut seen: Vec<(char, &str)> = Vec::new();
        for l in labels {
            let c = letter(l);
            if let Some((_, other)) = seen.iter().find(|(d, _)| *d == c) {
                panic!("{scope}: {l:?} and {other:?} both use Alt+{c}");
            }
            seen.push((c, l));
        }
    }

    #[test]
    fn no_two_visible_controls_share_a_mnemonic() {
        // CANCEL is left out because it replaces SCAN.
        let page = [SCAN, TAB_GENERAL, TAB_ADVANCED, SCAN_LOG, BACK_TO_RESULTS, DEFAULTS];
        let general = [
            FOLDERS, ADD_FOLDER, REMOVE_FOLDER, RECURSIVE, WORK_SIZE, MIN_POINTS, MIN_OVERLAP,
            MIN_CORRELATION, CANDIDATES, REPORT, REPORT_CHOOSE, REPORT_FORMAT,
        ];
        let advanced = [
            THREADS, EXTENSIONS, EXCLUDE, ADD_EXCLUDE, REMOVE_EXCLUDE, SYMLINKS, HIDDEN, USE_CACHE,
            CACHE_FILE, CACHE_CHOOSE, CLEAR_CACHE, PRUNE_CACHE, LOG_FILE, LOG_CHOOSE,
        ];
        distinct("general tab", &[&page[..], &general[..]].concat());
        distinct("advanced tab", &[&page[..], &advanced[..]].concat());
        assert_eq!(letter(SCAN), letter(CANCEL));
        distinct(
            "results",
            &[GROUPS, IMAGES, NEXT_GROUP, PREV_GROUP, MARK_OTHERS, UNMARK_GROUP, OPEN, SHOW_FOLDER, TRASH, NEW_SCAN, RESULTS_LOG],
        );
        distinct("preview", &[PREVIEW_PREV, PREVIEW_NEXT, PREVIEW_MARK, PREVIEW_CLOSE]);
        distinct("confirmation", &[CONFIRM_CANCEL, CONFIRM_TRASH]);
    }
}
