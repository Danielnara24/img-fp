//! What the window has been asked to scan, and how: the paths kept between
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
/// **Only the paths are kept: the folders, the exclusions and the three file
/// names typed in.** Every option starts where the command line's does, each
/// time the window opens, because a remembered option is a frozen default. The
/// window used to save every option on every scan, whether or not anyone had
/// touched it, so a default the command line changed never reached anyone who
/// had used the window before: `.dds` left the extension list in 0.19 and a
/// window from 0.17 went on asking for it, and exiting 2 on every `.dds` it
/// met. Within a session the options stay as set; the Defaults button puts
/// them back.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(with = "paths")]
    pub folders: Vec<PathBuf>,
    #[serde(skip)]
    pub recursive: bool,
    #[serde(skip)]
    pub work_size: usize,
    #[serde(skip)]
    pub candidates: usize,
    #[serde(skip)]
    pub min_aligned_points: u32,
    #[serde(skip)]
    pub min_frame_overlap: f32,
    #[serde(skip)]
    pub min_pixel_correlation: f32,

    #[serde(skip)]
    pub report: bool,
    pub report_path: String,
    #[serde(skip)]
    pub report_format: ReportFormat,

    #[serde(skip)]
    pub threads: usize,
    /// As `-x` takes it: comma-separated.
    #[serde(skip)]
    pub extensions: String,
    #[serde(with = "paths")]
    pub exclude: Vec<PathBuf>,
    #[serde(skip)]
    pub follow_symlinks: bool,
    #[serde(skip)]
    pub use_cache: bool,
    /// Empty for img-fp's own default.
    pub cache_path: String,
    #[serde(skip)]
    pub clear_cache: bool,
    #[serde(skip)]
    pub prune_cache: bool,
    #[serde(skip)]
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

/// A list of folders as the settings file keeps it: each one as text, or as
/// its bytes when its name is not UTF-8.
///
/// serde writes a `PathBuf` as a string and refuses one that is not UTF-8,
/// and one such folder in the list made the whole save fail — silently, since
/// a save that fails says nothing — so nothing at all was remembered.
mod paths {
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

    /// Options are not remembered, so a default the command line changes
    /// reaches the window: a file saved by an older window, holding the
    /// extension list and thresholds of its day, opens at today's defaults.
    #[test]
    fn a_saved_option_does_not_outlive_its_default() {
        let saved = r#"{"folders": ["/x"], "work_size": 640, "min_pixel_correlation": 0.5, "use_cache": false,
            "extensions": "jpg,png,dds", "threads": 3, "recursive": false, "report_path": "/r.csv"}"#;
        let s: Settings = serde_json::from_str(saved).unwrap();
        let d = Settings::default();
        assert_eq!(s.folders, vec![PathBuf::from("/x")]);
        assert_eq!(s.report_path, "/r.csv");
        assert_eq!((s.work_size, s.min_pixel_correlation, s.use_cache), (d.work_size, d.min_pixel_correlation, d.use_cache));
        assert_eq!((s.extensions.as_str(), s.threads, s.recursive), (d.extensions.as_str(), d.threads, d.recursive));
        assert!(!s.extensions.split(',').any(|e| e == "dds"));
        // And nothing but the paths is written.
        let v: serde_json::Value = serde_json::to_value(&s).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["cache_path", "exclude", "folders", "log_path", "report_path"]);
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
