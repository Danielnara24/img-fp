//! What the window has been asked to scan, and how: what of it is kept between
//! runs, and all of it turned into the img-fp command line the scan runs.
//!
//! The window never passes the scan anything the command line could not: a
//! scan from here is the scan those flags would run from a shell, and the log
//! says which flags they were.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReportFormat {
    /// Whatever the file name's extension says, as `-o` alone does.
    #[default]
    FromName,
    Txt,
    Csv,
    Json,
}

impl ReportFormat {
    pub const ALL: [ReportFormat; 4] = [ReportFormat::FromName, ReportFormat::Txt, ReportFormat::Csv, ReportFormat::Json];

    pub fn label(self) -> &'static str {
        match self {
            ReportFormat::FromName => "From the file name",
            ReportFormat::Txt => "Text",
            ReportFormat::Csv => "CSV",
            ReportFormat::Json => "JSON",
        }
    }

    fn flag(self) -> Option<&'static str> {
        match self {
            ReportFormat::FromName => None,
            ReportFormat::Txt => Some("txt"),
            ReportFormat::Csv => Some("csv"),
            ReportFormat::Json => Some("json"),
        }
    }
}

/// What the window shows, and what of it is kept between runs.
///
/// The paths are kept as they are: the folders, the exclusions and the three
/// file names typed in. **An option is kept only where it differs from the
/// command line's default** (`Kept`), so one never touched follows the default
/// when the default moves. Saving every option as it stood, which the window
/// did until 0.19, froze defaults nobody had chosen: `.dds` left the extension
/// list then and a window from 0.17 went on asking for it, exiting 2 on every
/// `.dds` it met. An option set by hand stays set until changed or put back
/// with the Defaults button. The one-off cache requests (clear, prune) are
/// never kept: they are carried out once.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "Saved", into = "Saved")]
pub struct Settings {
    pub folders: Vec<PathBuf>,
    pub recursive: bool,
    pub work_size: usize,
    pub candidates: usize,
    pub min_aligned_points: u32,
    pub min_frame_overlap: f32,
    pub min_pixel_correlation: f32,
    /// The second look; off is `--skip-mirrored`.
    pub mirrored: bool,

    pub report: bool,
    pub report_path: String,
    pub report_format: ReportFormat,

    pub threads: usize,
    /// As `-x` takes it: comma-separated.
    pub extensions: String,
    pub exclude: Vec<PathBuf>,
    pub follow_symlinks: bool,
    pub hidden: bool,
    pub use_cache: bool,
    /// Empty for img-fp's own default.
    pub cache_path: String,
    pub clear_cache: bool,
    pub prune_cache: bool,
    pub log: bool,
    pub log_path: String,
}

/// `Settings` as the file holds them: the paths, and under `options` each
/// option that differs from its default. The options are a key of their own
/// because the windows before 0.19 wrote every option at the top level: read
/// as choices now, a file one of them left would bring back the defaults of
/// its day.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved {
    #[serde(with = "paths")]
    folders: Vec<PathBuf>,
    report_path: String,
    #[serde(with = "paths")]
    exclude: Vec<PathBuf>,
    cache_path: String,
    log_path: String,
    #[serde(skip_serializing_if = "Kept::is_empty")]
    options: Kept,
}

/// The options set away from their defaults; `None` is "the default".
#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct Kept {
    #[serde(skip_serializing_if = "Option::is_none")]
    recursive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    work_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_aligned_points: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_frame_overlap: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_pixel_correlation: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mirrored: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_format: Option<ReportFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    threads: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extensions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    follow_symlinks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_cache: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<bool>,
}

impl Kept {
    fn is_empty(&self) -> bool {
        *self == Kept::default()
    }
}

/// `v` if it is not `d`. The thresholds come back from a spin button through
/// f64, so they are compared to its last digit, not to the bit.
fn changed<T: PartialEq>(v: T, d: T) -> Option<T> {
    (v != d).then_some(v)
}

fn changed_f32(v: f32, d: f32) -> Option<f32> {
    ((v - d).abs() > 1e-4).then_some(v)
}

impl From<Settings> for Saved {
    fn from(s: Settings) -> Saved {
        let d = Settings::default();
        Saved {
            options: Kept {
                recursive: changed(s.recursive, d.recursive),
                work_size: changed(s.work_size, d.work_size),
                candidates: changed(s.candidates, d.candidates),
                min_aligned_points: changed(s.min_aligned_points, d.min_aligned_points),
                min_frame_overlap: changed_f32(s.min_frame_overlap, d.min_frame_overlap),
                min_pixel_correlation: changed_f32(s.min_pixel_correlation, d.min_pixel_correlation),
                mirrored: changed(s.mirrored, d.mirrored),
                report: changed(s.report, d.report),
                report_format: changed(s.report_format, d.report_format),
                threads: changed(s.threads, d.threads),
                extensions: changed(s.extensions.trim().to_string(), d.extensions),
                follow_symlinks: changed(s.follow_symlinks, d.follow_symlinks),
                hidden: changed(s.hidden, d.hidden),
                use_cache: changed(s.use_cache, d.use_cache),
                log: changed(s.log, d.log),
            },
            folders: s.folders,
            report_path: s.report_path,
            exclude: s.exclude,
            cache_path: s.cache_path,
            log_path: s.log_path,
        }
    }
}

impl From<Saved> for Settings {
    fn from(f: Saved) -> Settings {
        let d = Settings::default();
        let o = f.options;
        Settings {
            folders: f.folders,
            recursive: o.recursive.unwrap_or(d.recursive),
            work_size: o.work_size.unwrap_or(d.work_size),
            candidates: o.candidates.unwrap_or(d.candidates),
            min_aligned_points: o.min_aligned_points.unwrap_or(d.min_aligned_points),
            min_frame_overlap: o.min_frame_overlap.unwrap_or(d.min_frame_overlap),
            min_pixel_correlation: o.min_pixel_correlation.unwrap_or(d.min_pixel_correlation),
            mirrored: o.mirrored.unwrap_or(d.mirrored),
            report: o.report.unwrap_or(d.report),
            report_path: f.report_path,
            report_format: o.report_format.unwrap_or(d.report_format),
            threads: o.threads.unwrap_or(d.threads),
            extensions: o.extensions.unwrap_or(d.extensions),
            exclude: f.exclude,
            follow_symlinks: o.follow_symlinks.unwrap_or(d.follow_symlinks),
            hidden: o.hidden.unwrap_or(d.hidden),
            use_cache: o.use_cache.unwrap_or(d.use_cache),
            cache_path: f.cache_path,
            clear_cache: false,
            prune_cache: false,
            log: o.log.unwrap_or(d.log),
            log_path: f.log_path,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let d = img_fp::defaults();
        Settings {
            folders: Vec::new(),
            // The command line's default is the top folder only; a person
            // pointing a window at their photos means all of them.
            recursive: true,
            work_size: d.work_size,
            candidates: d.candidates,
            min_aligned_points: d.min_aligned_points,
            min_frame_overlap: d.min_frame_overlap,
            min_pixel_correlation: d.min_pixel_correlation,
            mirrored: !d.skip_mirrored,
            report: false,
            report_path: String::new(),
            report_format: ReportFormat::FromName,
            threads: 0,
            extensions: d.extensions.join(","),
            exclude: Vec::new(),
            follow_symlinks: false,
            hidden: false,
            use_cache: true,
            cache_path: String::new(),
            clear_cache: false,
            prune_cache: false,
            log: false,
            log_path: String::new(),
        }
    }
}

// A list of folders as the settings file keeps it: each one as text, or as
// its bytes when its name is not UTF-8 (one such folder made the whole save
// fail, silently). Shared with the kept scan's notes.
pub(crate) use img_fp::last::paths;

/// `$XDG_CONFIG_HOME/img-fp/gui.json`, or `~/.config/img-fp/gui.json`.
fn file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("img-fp").join("gui.json"))
}

impl Settings {
    /// What was used last time, or the defaults. A file that will not parse
    /// is the defaults too: it is a convenience, not a record.
    pub fn load() -> Settings {
        file()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Kept for next time. Failing to is not worth interrupting anyone over.
    pub fn save(&self) {
        let Some(f) = file() else { return };
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let tmp = f.with_extension(format!("json.{}", std::process::id()));
            if std::fs::write(&tmp, b).is_ok() && std::fs::rename(&tmp, &f).is_err() {
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }

    /// The same settings with every file path the scan will write made
    /// absolute: `~/` is the home folder, and so is the start of any other
    /// relative path.
    ///
    /// A relative path was left for the worker to resolve against its working
    /// directory, which is whatever the window was started from — the home
    /// folder from a desktop menu, anywhere at all from a terminal — so
    /// `results.txt` went somewhere the window never said. Home is where a
    /// person typing a bare file name would look for it, and the window shows
    /// the path it chose in the field before the scan starts.
    pub fn with_absolute_paths(mut self) -> Settings {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        for p in [&mut self.report_path, &mut self.log_path, &mut self.cache_path] {
            *p = absolute(p, home.as_deref());
        }
        self
    }

    /// The img-fp command line these settings are, program name first.
    ///
    /// Every option the window shows is passed, default or not, so the log
    /// records the whole of what the scan was asked. The folders go last,
    /// after `--`, so that no folder name can be read as a flag.
    pub fn argv(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["img-fp".into()];
        let mut flag = |f: &str, v: String| {
            a.push(f.into());
            a.push(v.into());
        };
        flag("--work-size", self.work_size.to_string());
        flag("--candidates", self.candidates.to_string());
        flag("--min-aligned-points", self.min_aligned_points.to_string());
        flag("--min-frame-overlap", format!("{}", self.min_frame_overlap));
        flag("--min-pixel-correlation", format!("{}", self.min_pixel_correlation));
        flag("--threads", self.threads.to_string());
        let ext = self.extensions.trim();
        if !ext.is_empty() {
            flag("--extensions", ext.to_string());
        }
        if self.report && !self.report_path.trim().is_empty() {
            flag("--output", self.report_path.trim().to_string());
            if let Some(f) = self.report_format.flag() {
                flag("--format", f.to_string());
            }
        }
        // The named file goes with `--no-cache` too when it is to be cleared:
        // left out, `--clear-cache` deleted the default cache, not the one in
        // the field.
        if !self.use_cache {
            a.push("--no-cache".into());
        }
        if !self.cache_path.trim().is_empty() && (self.use_cache || self.clear_cache) {
            a.push("--cache".into());
            a.push(self.cache_path.trim().into());
        }
        if self.log && !self.log_path.trim().is_empty() {
            a.push("--log-file".into());
            a.push(self.log_path.trim().into());
        }
        for e in &self.exclude {
            a.push("--exclude".into());
            a.push(e.into());
        }
        if self.recursive {
            a.push("--recursive".into());
        }
        if self.follow_symlinks {
            a.push("--follow-symlinks".into());
        }
        if self.hidden {
            a.push("--hidden".into());
        }
        if !self.mirrored {
            a.push("--skip-mirrored".into());
        }
        if self.clear_cache {
            a.push("--clear-cache".into());
        }
        if self.prune_cache && self.use_cache {
            a.push("--prune-cache".into());
        }
        a.push("--".into());
        a.extend(self.folders.iter().map(|f| f.into()));
        a
    }
}

/// `p` made absolute against `home`, as `with_absolute_paths` describes. An
/// empty path is left empty: it means "not set".
fn absolute(p: &str, home: Option<&Path>) -> String {
    let t = p.trim();
    let Some(home) = home.filter(|h| h.is_absolute()) else { return t.to_string() };
    if t.is_empty() || Path::new(t).is_absolute() {
        return t.to_string();
    }
    let rest = if t == "~" { "" } else { t.strip_prefix("~/").unwrap_or(t) };
    home.join(rest).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder whose name is not UTF-8 is remembered, and does not stop the
    /// rest of the settings from being remembered with it.
    #[test]
    fn a_folder_that_is_not_utf8_is_saved_and_read_back() {
        use std::os::unix::ffi::OsStringExt;
        let odd = PathBuf::from(std::ffi::OsString::from_vec(b"/p/b\xff".to_vec()));
        let s = Settings { folders: vec!["/p/a".into(), odd.clone()], exclude: vec![odd.clone()], log_path: "/l".into(), ..Default::default() };
        let text = serde_json::to_string(&s).expect("saved");
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.folders, vec![PathBuf::from("/p/a"), odd.clone()]);
        assert_eq!(back.exclude, vec![odd]);
        assert_eq!(back.log_path, "/l");
        // A file written before this still reads.
        let old: Settings = serde_json::from_str(r#"{"folders": ["/x"], "exclude": []}"#).unwrap();
        assert_eq!(old.folders, vec![PathBuf::from("/x")]);
    }

    /// An option set by hand is kept; one left at its default is not, so a
    /// default the command line changes reaches it. A file saved by a window
    /// before 0.19, holding every option of its day at the top level, opens
    /// at today's defaults.
    #[test]
    fn only_options_set_by_hand_are_kept() {
        let d = Settings::default();
        let s = Settings { folders: vec!["/x".into()], work_size: 640, use_cache: false, log: true, recursive: false,
            report_format: ReportFormat::Csv, min_pixel_correlation: 0.55, clear_cache: true, prune_cache: true, ..Default::default() };
        let v: serde_json::Value = serde_json::to_value(&s).unwrap();
        let mut keys: Vec<&str> = v["options"].as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["log", "min_pixel_correlation", "recursive", "report_format", "use_cache", "work_size"]);
        let back: Settings = serde_json::from_value(v).unwrap();
        assert_eq!((back.work_size, back.use_cache, back.log, back.recursive), (640, false, true, false));
        assert_eq!((back.report_format, back.min_pixel_correlation), (ReportFormat::Csv, 0.55));
        assert_eq!((back.candidates, back.extensions.as_str()), (d.candidates, d.extensions.as_str()));
        assert!(!back.clear_cache && !back.prune_cache, "one-off requests are not repeated");

        // Nothing set by hand: nothing but the paths is written.
        let v: serde_json::Value = serde_json::to_value(Settings { folders: vec!["/x".into()], ..Default::default() }).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["cache_path", "exclude", "folders", "log_path", "report_path"]);

        let old = r#"{"folders": ["/x"], "work_size": 640, "min_pixel_correlation": 0.5, "use_cache": false,
            "extensions": "jpg,png,dds", "threads": 3, "recursive": false, "report_path": "/r.csv"}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert_eq!(s.folders, vec![PathBuf::from("/x")]);
        assert_eq!(s.report_path, "/r.csv");
        assert_eq!((s.work_size, s.min_pixel_correlation, s.use_cache), (d.work_size, d.min_pixel_correlation, d.use_cache));
        assert_eq!((s.extensions.as_str(), s.threads, s.recursive), (d.extensions.as_str(), d.threads, d.recursive));
    }

    #[test]
    fn a_relative_path_is_made_absolute_against_home() {
        let home = Some(Path::new("/home/u"));
        assert_eq!(absolute("results.txt", home), "/home/u/results.txt");
        assert_eq!(absolute(" out/r.csv ", home), "/home/u/out/r.csv");
        assert_eq!(absolute("~/r.json", home), "/home/u/r.json");
        assert_eq!(absolute("~", home), "/home/u/");
        assert_eq!(absolute("/tmp/r.txt", home), "/tmp/r.txt");
        assert_eq!(absolute("", home), "");
        assert_eq!(absolute("r.txt", None), "r.txt", "with no home there is nothing better to do");
    }

    /// Whatever the window can be set to, the command line it makes is one
    /// img-fp accepts: the scan must never fail on its own arguments.
    #[test]
    fn every_setting_makes_a_command_line_img_fp_accepts() {
        let mut s = Settings { folders: vec!["/tmp".into(), "/-odd name".into()], ..Default::default() };
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        s.use_cache = false;
        s.clear_cache = true;
        s.prune_cache = true;
        s.cache_path = "/tmp/x".into();
        s.report = true;
        s.report_path = "/tmp/r.csv".into();
        s.report_format = ReportFormat::Json;
        s.log = true;
        s.log_path = "/tmp/l.txt".into();
        s.exclude = vec!["/tmp/a".into()];
        s.follow_symlinks = true;
        s.hidden = true;
        s.mirrored = false;
        s.recursive = false;
        s.extensions = "*".into();
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        s.use_cache = true;
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        // Not using the cache and not clearing it: the named file is left
        // out, since the run would neither read nor write it.
        s.use_cache = false;
        s.clear_cache = false;
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        assert!(!s.argv().iter().any(|a| a == "--cache"));
    }

    /// Clearing the cache with the cache off clears the file named in the
    /// field, not the default one.
    #[test]
    fn clearing_without_using_clears_the_named_file() {
        let s = Settings { folders: vec!["/tmp".into()], use_cache: false, clear_cache: true, cache_path: "/x/mine.bin".into(), ..Default::default() };
        let a = s.argv();
        let at = a.iter().position(|x| x == "--cache").expect("the named cache is passed");
        assert_eq!(a[at + 1], "/x/mine.bin");
        assert!(a.iter().any(|x| x == "--no-cache") && a.iter().any(|x| x == "--clear-cache"));
        assert_eq!(img_fp::check_args(a), Ok(()));
    }

    #[test]
    fn defaults_are_img_fps_own() {
        let d = img_fp::defaults();
        let s = Settings::default();
        assert_eq!((s.work_size, s.candidates, s.min_aligned_points), (d.work_size, d.candidates, d.min_aligned_points));
        assert_eq!(s.mirrored, !d.skip_mirrored);
    }
}
