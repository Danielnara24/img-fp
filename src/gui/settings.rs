//! What the window has been asked to scan, and how: what of it is kept between
//! runs, and all of it turned into the img-fp command line the scan runs.
//!
//! The window never passes the scan anything the command line could not: a
//! scan from here is the scan those flags would run from a shell, and the log
//! says which flags they were.

use crate::options::{self as o, Opt, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What the window shows, and what of it is kept between runs: the paths,
/// and a value for each option of `options::ALL`.
///
/// The paths are kept as they are: the folders, the exclusions and the three
/// file names typed in. **An option is kept only where it differs from its
/// default**, so one never touched follows the default when the default
/// moves. Saving every option as it stood, which the window did until 0.19,
/// froze defaults nobody had chosen: `.dds` left the extension list then and
/// a window from 0.17 went on asking for it, exiting 2 on every `.dds` it
/// met. An option set by hand stays set until changed or put back with the
/// Defaults button. The one-off cache requests (clear, prune) are never kept:
/// they are carried out once.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "Saved", into = "Saved")]
pub struct Settings {
    pub folders: Vec<PathBuf>,
    pub exclude: Vec<PathBuf>,
    pub report_path: String,
    /// Empty for img-fp's own default.
    pub cache_path: String,
    pub log_path: String,
    /// One per option, in `options::ALL`'s order.
    values: Vec<Value>,
}

/// `Settings` as the file holds them: the paths, and under `options` each
/// option that differs from its default, by its key. The options are a key
/// of their own because the windows before 0.19 wrote every option at the
/// top level: read as choices now, a file one of them left would bring back
/// the defaults of its day.
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    options: BTreeMap<String, serde_json::Value>,
}

impl From<Settings> for Saved {
    fn from(s: Settings) -> Saved {
        let options = o::ALL
            .iter()
            .zip(&s.values)
            .filter(|(opt, v)| opt.kept && **v != opt.default_value())
            .map(|(opt, v)| (opt.key.to_string(), opt.as_json(v)))
            .collect();
        Saved { folders: s.folders, report_path: s.report_path, exclude: s.exclude, cache_path: s.cache_path, log_path: s.log_path, options }
    }
}

impl From<Saved> for Settings {
    /// An option missing, of another type, or not kept, is its default.
    fn from(f: Saved) -> Settings {
        let values = o::ALL
            .iter()
            .map(|opt| {
                let kept = f.options.get(opt.key).filter(|_| opt.kept).and_then(|j| opt.read_json(j));
                kept.unwrap_or_else(|| opt.default_value())
            })
            .collect();
        Settings { folders: f.folders, exclude: f.exclude, report_path: f.report_path, cache_path: f.cache_path, log_path: f.log_path, values }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Saved::default().into()
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

    /// The value of option `o`.
    pub fn get(&self, o: &Opt) -> &Value {
        &self.values[o.at()]
    }

    pub fn set(&mut self, o: &Opt, v: Value) {
        self.values[o.at()] = v;
    }

    /// Whether the check option `o` is on.
    pub fn on(&self, o: &Opt) -> bool {
        self.get(o).on()
    }

    /// Every option and file name back to its default, as the Defaults
    /// button puts them; the folders to scan and to leave out are kept.
    pub fn with_defaults(self) -> Settings {
        Settings { folders: self.folders, exclude: self.exclude, ..Settings::default() }
    }

    /// The img-fp command line these settings are, program name first.
    ///
    /// Every option the window shows is passed, default or not, so the log
    /// records the whole of what the scan was asked. The folders go last,
    /// after `--`, so that no folder name can be read as a flag.
    pub fn argv(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["img-fp".into()];
        for (opt, v) in o::ALL.iter().zip(&self.values) {
            a.extend(opt.command_line(v).into_iter().map(OsString::from));
        }
        // The window's own switches, each with the path it stands for.
        let mut flag = |f: &str, v: &str| {
            a.push(f.into());
            a.push(v.into());
        };
        let (report, cache, log) = (self.report_path.trim(), self.cache_path.trim(), self.log_path.trim());
        if self.on(&o::REPORT) && !report.is_empty() {
            flag("--output", report);
            if let Some(f) = o::REPORT_FORMAT.choice_flag(self.get(&o::REPORT_FORMAT)) {
                flag("--format", f);
            }
        }
        let use_cache = self.on(&o::USE_CACHE);
        // The named file goes with `--no-cache` too when it is to be cleared:
        // left out, `--clear-cache` deleted the default cache, not the one in
        // the field.
        if !cache.is_empty() && (use_cache || self.on(&o::CLEAR_CACHE)) {
            flag("--cache", cache);
        }
        if self.on(&o::LOG) && !log.is_empty() {
            flag("--log-file", log);
        }
        for e in &self.exclude {
            a.push("--exclude".into());
            a.push(e.into());
        }
        if !use_cache {
            a.push("--no-cache".into());
        }
        if self.on(&o::CLEAR_CACHE) {
            a.push("--clear-cache".into());
        }
        if self.on(&o::PRUNE_CACHE) && use_cache {
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
    use crate::options::Kind;

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
        let mut s = Settings { folders: vec!["/x".into()], ..Default::default() };
        s.set(&o::WORK_SIZE, Value::Number(640.0));
        s.set(&o::USE_CACHE, Value::On(false));
        s.set(&o::LOG, Value::On(true));
        s.set(&o::RECURSIVE, Value::On(false));
        s.set(&o::REPORT_FORMAT, Value::Text("Csv".into()));
        s.set(&o::MIN_CORRELATION, Value::Number(0.55));
        s.set(&o::CLEAR_CACHE, Value::On(true));
        s.set(&o::PRUNE_CACHE, Value::On(true));
        let v: serde_json::Value = serde_json::to_value(&s).unwrap();
        let keys: Vec<&str> = v["options"].as_object().unwrap().keys().map(|k| k.as_str()).collect();
        assert_eq!(keys, ["log", "min_pixel_correlation", "recursive", "report_format", "use_cache", "work_size"]);
        assert_eq!(v["options"]["work_size"], serde_json::json!(640));
        let back: Settings = serde_json::from_value(v).unwrap();
        for opt in [&o::WORK_SIZE, &o::USE_CACHE, &o::LOG, &o::RECURSIVE, &o::REPORT_FORMAT, &o::MIN_CORRELATION] {
            assert_eq!(back.get(opt), s.get(opt), "{}", opt.key);
        }
        for opt in [&o::CANDIDATES, &o::EXTENSIONS] {
            assert_eq!(back.get(opt), d.get(opt), "{}", opt.key);
        }
        assert!(!back.on(&o::CLEAR_CACHE) && !back.on(&o::PRUNE_CACHE), "one-off requests are not repeated");

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
        assert_eq!(s.values, d.values);

        // A value of another type, or a choice not offered, is the default;
        // the rest of the file still reads. (It used to make the whole file
        // the defaults, folders and all.)
        let odd = r#"{"folders": ["/x"], "options": {"work_size": "big", "report_format": "Pdf", "threads": 3}}"#;
        let s: Settings = serde_json::from_str(odd).unwrap();
        assert_eq!(s.folders, vec![PathBuf::from("/x")]);
        assert_eq!((s.get(&o::WORK_SIZE), s.get(&o::REPORT_FORMAT)), (d.get(&o::WORK_SIZE), d.get(&o::REPORT_FORMAT)));
        assert_eq!(s.get(&o::THREADS), &Value::Number(3.0));
    }

    /// A file written by the window before the options were a table reads
    /// as it did: a float kept as an f32 comes back at its decimals.
    #[test]
    fn a_file_from_before_the_table_reads_as_it_did() {
        let f = r#"{"folders": ["/x"], "options": {"min_frame_overlap": 0.550000011920929, "mirrored": false,
            "report_format": "Json", "extensions": "png,gif", "report": true}}"#;
        let s: Settings = serde_json::from_str(f).unwrap();
        assert_eq!(s.get(&o::MIN_OVERLAP), &Value::Number(0.55));
        assert!(!s.on(&o::MIRRORED) && s.on(&o::REPORT));
        assert_eq!(s.get(&o::REPORT_FORMAT).text(), "Json");
        assert_eq!(s.get(&o::EXTENSIONS).text(), "png,gif");
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
        for (opt, v) in [(&o::USE_CACHE, false), (&o::CLEAR_CACHE, true), (&o::PRUNE_CACHE, true), (&o::REPORT, true), (&o::LOG, true),
            (&o::SYMLINKS, true), (&o::HIDDEN, true), (&o::MIRRORED, false), (&o::RECURSIVE, false)] {
            s.set(opt, Value::On(v));
        }
        s.cache_path = "/tmp/x".into();
        s.report_path = "/tmp/r.csv".into();
        s.set(&o::REPORT_FORMAT, Value::Text("Json".into()));
        s.log_path = "/tmp/l.txt".into();
        s.exclude = vec!["/tmp/a".into()];
        s.set(&o::EXTENSIONS, Value::Text("*".into()));
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        s.set(&o::USE_CACHE, Value::On(true));
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        // Not using the cache and not clearing it: the named file is left
        // out, since the run would neither read nor write it.
        s.set(&o::USE_CACHE, Value::On(false));
        s.set(&o::CLEAR_CACHE, Value::On(false));
        assert_eq!(img_fp::check_args(s.argv()), Ok(()));
        assert!(!s.argv().iter().any(|a| a == "--cache"));
    }

    /// Clearing the cache with the cache off clears the file named in the
    /// field, not the default one.
    #[test]
    fn clearing_without_using_clears_the_named_file() {
        let mut s = Settings { folders: vec!["/tmp".into()], cache_path: "/x/mine.bin".into(), ..Default::default() };
        s.set(&o::USE_CACHE, Value::On(false));
        s.set(&o::CLEAR_CACHE, Value::On(true));
        let a = s.argv();
        let at = a.iter().position(|x| x == "--cache").expect("the named cache is passed");
        assert_eq!(a[at + 1], "/x/mine.bin");
        assert!(a.iter().any(|x| x == "--no-cache") && a.iter().any(|x| x == "--clear-cache"));
        assert_eq!(img_fp::check_args(a), Ok(()));
    }

    /// Every option with a flag reaches img-fp as what it was set to, at its
    /// default and away from it, and is read back by the command line as
    /// that value.
    #[test]
    fn every_option_reaches_the_command_line_as_set() {
        for opt in o::ALL.iter().filter(|opt| opt.arg.is_some()) {
            let id = opt.arg.unwrap();
            let away = match (&opt.kind, opt.default_value()) {
                (Kind::Check { .. }, Value::On(b)) => Value::On(!b),
                (Kind::Number { max, .. }, Value::Number(n)) => opt.number(if n + 1.0 <= *max { n + 1.0 } else { n / 2.0 }),
                (Kind::Text, _) => Value::Text("png,gif".into()),
                (_, v) => panic!("{}: nothing away from {v:?}", opt.key),
            };
            for v in [opt.default_value(), away] {
                let mut s = Settings { folders: vec!["/tmp".into()], ..Default::default() };
                s.set(opt, v.clone());
                let given = img_fp::given(s.argv()).unwrap_or_else(|e| panic!("{}: {e}", opt.key));
                let back = match (&opt.kind, given.get(id)) {
                    (Kind::Check { inverted }, g) => Value::On(g.is_some() != *inverted),
                    (Kind::Number { .. }, Some(g)) => opt.parse(&g[0]).unwrap(),
                    (Kind::Text, Some(g)) => Value::Text(g.join(",")),
                    (_, None) => panic!("{} not passed", opt.key),
                    _ => unreachable!(),
                };
                assert_eq!(back, v, "{}", opt.key);
            }
        }
    }
}
