//! The options the window shows, each said once: which of img-fp's flags it
//! is, how it is shown, and in what words.
//!
//! `Settings` holds a value per option and `Setup` a widget per option, and
//! both go through this table rather than a field per option. An option used
//! to be written out by hand in a dozen places — a field of `Settings`, of
//! the kept file's form, two conversions, the defaults, the command line, a
//! field of the page, its widget, its showing and its reading — and a place
//! missed compiled and passed every test. Now an option is an entry here (and
//! a widget placed on the page), its default is the command line's own
//! (`img_fp::flag`), and a test asks that every flag of img-fp is either one
//! of these, passed by hand, or not the window's to pass.

use crate::labels as l;

/// How an option is shown, and so what its value is.
pub enum Kind {
    /// A check button. Its flag is given when it is on, or, `inverted`, when
    /// it is off.
    Check { inverted: bool },
    /// A spin button, its value rounded to `digits` decimals. `max` is asked
    /// when the button is made: a limit can be this machine's.
    Number { min: f64, max: fn() -> f64, step: f64, digits: u32 },
    /// An entry, its text as the flag takes it; left out when empty.
    Text,
    /// A drop-down: each choice as the settings file keeps it, the flag's
    /// value (`None`: no flag), and its label.
    Choice(&'static [(&'static str, Option<&'static str>, &'static str)]),
}

/// One option of the window.
pub struct Opt {
    /// Its name in the settings file.
    pub key: &'static str,
    /// The command-line option it is, by clap id: passed as its flag and its
    /// value, defaulting to the command line's default. `None` for the
    /// window's own switches, which `Settings::argv` passes by hand.
    pub arg: Option<&'static str>,
    /// The window's default, as the command line would write it, where it
    /// is not the command line's or there is none.
    pub default: Option<&'static str>,
    pub kind: Kind,
    pub label: &'static str,
    /// Over the option and its label.
    pub tip: Option<&'static str>,
    /// Beside or under it, on the page.
    pub hint: Option<&'static str>,
    /// Kept between runs. The one-off cache requests are carried out once.
    pub kept: bool,
}

/// An option's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    On(bool),
    Number(f64),
    /// Text, or a choice as the settings file keeps it.
    Text(String),
}

impl Value {
    pub fn on(&self) -> bool {
        matches!(self, Value::On(true))
    }

    pub fn number(&self) -> f64 {
        match self {
            Value::Number(v) => *v,
            _ => 0.0,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Value::Text(t) => t,
            _ => "",
        }
    }
}

const fn plain(key: &'static str, arg: Option<&'static str>, kind: Kind, label: &'static str) -> Opt {
    Opt { key, arg, default: None, kind, label, tip: None, hint: None, kept: true }
}

const fn number(min: f64, max: fn() -> f64, step: f64, digits: u32) -> Kind {
    Kind::Number { min, max, step, digits }
}

const CHECK: Kind = Kind::Check { inverted: false };

// Spin ranges, each what a value can mean. The work size goes up to JPEG's
// largest side, 65,535. The command line has no limit, and this one used to
// be 4,096, which turned away a size the command line takes for nothing; past
// the largest picture in a folder any size means what 0 (full size) means.
// Candidates go up to the most images the index can name, aligned points to
// the most keypoints an image is described by (a bar above it matches
// nothing), threads to this machine's cores (the scan runs no more: see
// `img_fp::threads`). They were 2,000, 200 and 1,024, from nothing.

pub static WORK_SIZE: Opt = Opt {
    tip: Some("Pixels on the long side each image is analysed at. Higher finds more, especially small pictures inside larger ones such as slides and collages, but is slower. 640 is a good choice when that matters. 0 does not shrink images at all, which is much slower on large photos."),
    ..plain("work_size", Some("work_size"), number(0.0, || 65535.0, 32.0, 0), l::WORK_SIZE)
};
pub static CANDIDATES: Opt = Opt {
    tip: Some("Possible matches checked for each image."),
    ..plain("candidates", Some("candidates"), number(1.0, || img_fp::MOST_CANDIDATES as f64, 10.0, 0), l::CANDIDATES)
};
pub static MIN_POINTS: Opt = Opt {
    tip: Some("Matching points two images must share. Higher is stricter."),
    ..plain("min_aligned_points", Some("min_aligned_points"), number(0.0, || img_fp::MOST_ALIGNED_POINTS as f64, 1.0, 0), l::MIN_POINTS)
};
pub static MIN_OVERLAP: Opt = Opt {
    tip: Some("How much of one image must lie inside the other, from 0 to 1. Higher is stricter."),
    ..plain("min_frame_overlap", Some("min_frame_overlap"), number(0.0, || 1.0, 0.05, 2), l::MIN_OVERLAP)
};
pub static MIN_CORRELATION: Opt = Opt {
    tip: Some("How closely the pixels of that shared area must agree, from 0 to 1. Higher is stricter."),
    ..plain("min_pixel_correlation", Some("min_pixel_correlation"), number(0.0, || 1.0, 0.05, 2), l::MIN_CORRELATION)
};
pub static THREADS: Opt = Opt {
    hint: Some("Worker threads. 0 uses every core."),
    ..plain("threads", Some("threads"), number(0.0, || img_fp::cores() as f64, 1.0, 0), l::THREADS)
};
pub static EXTENSIONS: Opt = Opt {
    hint: Some("Extensions a folder is searched for, comma-separated. * takes every file, and !gif leaves GIFs out of everything else."),
    ..plain("extensions", Some("extensions"), Kind::Text, l::EXTENSIONS)
};
pub static RECURSIVE: Opt = Opt {
    // The command line's default is the top folder only; a person pointing a
    // window at their photos means all of them.
    default: Some("true"),
    ..plain("recursive", Some("recursive"), CHECK, l::RECURSIVE)
};
pub static SYMLINKS: Opt = plain("follow_symlinks", Some("follow_symlinks"), CHECK, l::SYMLINKS);
pub static HIDDEN: Opt = Opt {
    tip: Some("Also look in folders whose names start with a dot. They hold programs' icons and themes rather than photos, \
               so they are left out unless one is added as a folder to scan."),
    ..plain("hidden", Some("hidden"), CHECK, l::HIDDEN)
};
/// The second look; off is `--skip-mirrored`.
pub static MIRRORED: Opt = Opt {
    tip: Some("Images that found fewer than two matches are searched again, mirrored and inverted. In a library with few \
               duplicates that is most images and most of a rescan's time. Turned off, scans there are much faster, \
               most mirrored and inverted copies are missed, and a group they linked together can split in two."),
    hint: Some("Off is much faster on a library with few duplicates, and misses most mirrored and inverted copies."),
    ..plain("mirrored", Some("skip_mirrored"), Kind::Check { inverted: true }, l::MIRRORED)
};

// The window's own switches: each says whether a path beside it is used, and
// `Settings::argv` passes the flags they stand for.

pub static REPORT: Opt = Opt { default: Some("false"), ..plain("report", None, CHECK, l::REPORT) };
/// `-o` alone goes by the file name's extension.
pub static REPORT_FORMAT: Opt = Opt {
    default: Some("FromName"),
    ..plain(
        "report_format",
        None,
        Kind::Choice(&[("FromName", None, "From the file name"), ("Txt", Some("txt"), "Text"), ("Csv", Some("csv"), "CSV"), ("Json", Some("json"), "JSON")]),
        l::REPORT_FORMAT,
    )
};
pub static USE_CACHE: Opt = Opt {
    default: Some("true"),
    hint: Some("Keeps each image's analysis, so a folder scanned again only analyses what changed."),
    ..plain("use_cache", None, CHECK, l::USE_CACHE)
};
pub static CLEAR_CACHE: Opt = Opt { default: Some("false"), kept: false, ..plain("clear_cache", None, CHECK, l::CLEAR_CACHE) };
pub static PRUNE_CACHE: Opt = Opt { default: Some("false"), kept: false, ..plain("prune_cache", None, CHECK, l::PRUNE_CACHE) };
pub static LOG: Opt = Opt {
    default: Some("false"),
    hint: Some("Every skipped file, problem and stage timing, in full."),
    ..plain("log", None, CHECK, l::LOG_FILE)
};

/// Every option, in the order the command line passes them.
pub static ALL: [&Opt; 17] = [
    &WORK_SIZE, &CANDIDATES, &MIN_POINTS, &MIN_OVERLAP, &MIN_CORRELATION, &THREADS, &EXTENSIONS,
    &RECURSIVE, &SYMLINKS, &HIDDEN, &MIRRORED,
    &REPORT, &REPORT_FORMAT, &USE_CACHE, &CLEAR_CACHE, &PRUNE_CACHE, &LOG,
];

impl Opt {
    /// Where it is in `ALL`, which is where `Settings` keeps its value.
    pub fn at(&self) -> usize {
        ALL.iter().position(|o| o.key == self.key).expect("every option is in ALL")
    }

    /// `s` as this option's value: as the command line or the settings file
    /// writes it. `None` if it is no such value.
    pub fn parse(&self, s: &str) -> Option<Value> {
        match self.kind {
            Kind::Check { inverted } => match s {
                "true" => Some(Value::On(!inverted)),
                "false" => Some(Value::On(inverted)),
                _ => None,
            },
            Kind::Number { .. } => s.trim().parse().ok().map(|v| self.number(v)),
            Kind::Text => Some(Value::Text(s.trim().to_string())),
            Kind::Choice(cs) => cs.iter().find(|c| c.0 == s).map(|c| Value::Text(c.0.to_string())),
        }
    }

    /// `v`, a number of this option, rounded to its decimals: a spin button
    /// stepped from 0.85 by 0.05 reads 0.7999999999999999, and a file
    /// written by a window before 0.59 kept 0.55 as a float.
    pub fn number(&self, v: f64) -> Value {
        let digits = match self.kind {
            Kind::Number { digits, .. } => digits,
            _ => 0,
        };
        let f = 10f64.powi(digits as i32);
        Value::Number((v * f).round() / f)
    }

    /// The default: the window's own, or else the command line's.
    pub fn default_value(&self) -> Value {
        static DEFAULTS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
        let all = DEFAULTS.get_or_init(|| {
            ALL.iter()
                .map(|o| {
                    let cli = o.arg.and_then(img_fp::flag).and_then(|f| f.default);
                    let text = o.default.map(str::to_string).or(cli).unwrap_or_default();
                    o.parse(&text).unwrap_or_else(|| panic!("{}: the default {text:?} is no value of it", o.key))
                })
                .collect()
        });
        all[self.at()].clone()
    }

    /// The value as the settings file keeps it.
    pub fn as_json(&self, v: &Value) -> serde_json::Value {
        match (v, &self.kind) {
            (Value::On(b), _) => (*b).into(),
            (Value::Number(n), Kind::Number { digits: 0, .. }) => (*n as u64).into(),
            (Value::Number(n), _) => (*n).into(),
            (Value::Text(t), _) => t.as_str().into(),
        }
    }

    /// The value the settings file keeps, or `None` if it holds none of this
    /// option's (another type, a choice no longer offered).
    pub fn read_json(&self, j: &serde_json::Value) -> Option<Value> {
        match self.kind {
            Kind::Check { .. } => j.as_bool().map(Value::On),
            Kind::Number { .. } => j.as_f64().map(|v| self.number(v)),
            Kind::Text | Kind::Choice(_) => j.as_str().and_then(|s| self.parse(s)),
        }
    }

    /// The flag and its value, as `Settings::argv` passes an option with an
    /// `arg`; nothing for a check that is off, or empty text.
    pub fn command_line(&self, v: &Value) -> Vec<String> {
        let Some(flag) = self.arg.and_then(img_fp::flag) else { return Vec::new() };
        match (&self.kind, v) {
            (Kind::Check { inverted }, Value::On(on)) if on != inverted => vec![flag.long],
            (Kind::Number { digits: 0, .. }, Value::Number(n)) => vec![flag.long, (*n as u64).to_string()],
            (Kind::Number { .. }, Value::Number(n)) => vec![flag.long, n.to_string()],
            (Kind::Text, Value::Text(t)) if !t.is_empty() => vec![flag.long, t.clone()],
            _ => Vec::new(),
        }
    }

    /// The flag's value of a choice: `None` when it gives no flag.
    pub fn choice_flag(&self, v: &Value) -> Option<&'static str> {
        let Kind::Choice(cs) = self.kind else { return None };
        cs.iter().find(|c| c.0 == v.text()).and_then(|c| c.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// img-fp's options the window passes by hand (`Settings::argv`): the paths,
    /// and what the switches above stand for.
    const BY_HAND: [&str; 9] = ["roots", "exclude", "output", "format", "no_cache", "cache", "clear_cache", "prune_cache", "log_file"];

    /// img-fp's options the window never passes: another kind of run, an output
    /// it has no place for, or what its own worker decides.
    const NOT_SHOWN: [&str; 10] = ["from_file", "null", "against", "list", "suggest", "dump", "last", "no_last", "verbose", "completions"];

    /// Each of img-fp's options is the window's to show, its to pass by
    /// hand, or not its to pass, and only one of these: an option added to
    /// the command line fails here until it is put in one.
    #[test]
    fn every_flag_is_decided() {
        let mut decided: Vec<&str> = ALL.iter().filter_map(|o| o.arg).chain(BY_HAND).chain(NOT_SHOWN).collect();
        decided.push("man");
        let n = decided.len();
        decided.sort_unstable();
        decided.dedup();
        assert_eq!(decided.len(), n, "an option is decided twice");
        // clap's own, there or not depending on when it is asked.
        let mut ids: Vec<String> = img_fp::option_ids().into_iter().filter(|i| i != "help" && i != "version").collect();
        ids.sort_unstable();
        assert_eq!(decided, ids);
    }

    /// Every option has a default that is a value of it, a flag where it
    /// names one, and a key of its own.
    #[test]
    fn every_option_is_whole() {
        let mut keys: Vec<&str> = ALL.iter().map(|o| o.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), ALL.len());
        for (i, o) in ALL.iter().enumerate() {
            assert_eq!(o.at(), i);
            o.default_value();
            if let Some(a) = o.arg {
                assert!(img_fp::flag(a).is_some(), "{} has no flag", o.key);
            }
            assert_eq!(o.read_json(&o.as_json(&o.default_value())), Some(o.default_value()), "{}", o.key);
        }
    }
}
