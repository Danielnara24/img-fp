//! What the window has been asked to scan, and how: kept between runs, and
//! turned into the img-fp command line the scan runs.
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub folders: Vec<PathBuf>,
    pub recursive: bool,
    pub work_size: usize,
    pub candidates: usize,
    pub min_aligned_points: u32,
    pub min_frame_overlap: f32,
    pub min_pixel_correlation: f32,

    pub report: bool,
    pub report_path: String,
    pub report_format: ReportFormat,

    pub threads: usize,
    /// As `-x` takes it: comma-separated.
    pub extensions: String,
    pub exclude: Vec<PathBuf>,
    pub follow_symlinks: bool,
    pub use_cache: bool,
    /// Empty for img-fp's own default.
    pub cache_path: String,
    /// One-off requests, never remembered: a cache emptied or pruned on every
    /// scan because a box was ticked once would be a surprise.
    #[serde(skip)]
    pub clear_cache: bool,
    #[serde(skip)]
    pub prune_cache: bool,
    pub log: bool,
    pub log_path: String,
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
            report: false,
            report_path: String::new(),
            report_format: ReportFormat::FromName,
            threads: 0,
            extensions: d.extensions.join(","),
            exclude: Vec::new(),
            follow_symlinks: false,
            use_cache: true,
            cache_path: String::new(),
            clear_cache: false,
            prune_cache: false,
            log: false,
            log_path: String::new(),
        }
    }
}

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
        if !self.use_cache {
            a.push("--no-cache".into());
        } else if !self.cache_path.trim().is_empty() {
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
        s.recursive = false;
        s.extensions = "*".into();
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        s.use_cache = true;
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
    }

    #[test]
    fn defaults_are_img_fps_own() {
        let d = img_fp::defaults();
        let s = Settings::default();
        assert_eq!((s.work_size, s.candidates, s.min_aligned_points), (d.work_size, d.candidates, d.min_aligned_points));
    }
}
