//! Two things, both said once here.
//!
//! **`dispatch`**, a cfg for an x86-64 build that cannot assume AVX2. The
//! kernels are written for x86-64-v3 (AVX2, FMA, BMI2), and a build for a
//! machine that has them — `target-cpu=native`, or `x86-64-v3` — compiles them
//! in directly. A build for plain x86-64 is what the release is, and what
//! `cargo install` and a plain `cargo build` make; there the hot paths are
//! compiled twice and chosen when the run starts; see
//! `src/simd.rs`. This names that case once, rather than spelling
//! `all(target_arch = "x86_64", not(target_feature = "avx2"))` at every place
//! it decides something.
//!
//! **`IMGFP_ANALYSIS`**, what a file's analysis computes from: the sources
//! that make it (`ANALYSIS_SOURCES`), the crates they call and those crates'
//! own dependencies at the versions `Cargo.lock` gives (`analysis_crates`),
//! and the target and the CPU features it is compiled for, hashed. A cached
//! analysis is kept under it (`cache.rs`), so a build whose analysis could
//! differ reads none of another's, and nobody has to remember to say so: until
//! 0.82.0 a format number was raised by hand, and when the analysis changed
//! after 0.76.0 without one, caches filled before it were read as if they were
//! today's (IMGS-ALL2 F1 0.9343 cached against 0.9414 cold).
//!
//! **Only what can change a value**, so a release does not cost every user
//! their whole library's analysis for nothing. Not a line that is only a
//! comment; not a crate only the matching, the report or the window uses; not
//! the build profile (an optimised build computes the same numbers). Nor the
//! compiler or the flags beyond the CPU features: Rust's arithmetic is IEEE's
//! to the bit and never fused unless the code asks (`mul_add`, which the
//! features decide; a native build differs from the plain one on JPEG XL
//! alone, through jxl-grid's FMA), and the analysis sorts nothing whose ties
//! an unstable sort could order differently. Checked: the records of 636
//! pictures made by builds of rustc 1.92 and 1.99 are byte for byte the same.
//! That the list
//! is whole is held by `analysis::tests::the_analysis_reads_only_its_sources`:
//! no listed file names a module outside it but those that change no value.
//! `IMGFP_ANALYSIS_SOURCES` and `IMGFP_ANALYSIS_CRATES` are the lists, for
//! tests.
//!
//! **`IMGFP_BUILD`**, what this build computes from: every source file, the
//! manifest and the lock file, the compiler, the target, its features and the
//! flags, hashed. The last run's matching is kept under it (`matched.rs`). It
//! was the hash of the running program's own bytes, which `img-fp` and
//! `img-fp-gui` never share, so a scan from the window never took a matching
//! the command line had kept, nor the other way round, though both run the
//! same library; and every run read the program whole to hash it. Cargo
//! features are left out for the same reason: the window's is the only one
//! that changes the build, and it adds the window, not a line of the scan.

use std::path::Path;

/// The files a file's analysis is computed by; see `IMGFP_ANALYSIS`. A
/// folder stands for every file under it but `tests.rs`, which only tests.
const ANALYSIS_SOURCES: [&str; 8] = [
    "src/decode",
    "src/sift.rs",
    "src/analysis.rs",
    "src/simd.rs",
    "src/thumb.rs",
    "src/heif.rs",
    "src/extensions.rs",
    "build.rs",
];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(dispatch)");
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let features = std::env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    if arch == "x86_64" && !features.split(',').any(|f| f == "avx2") {
        println!("cargo::rustc-cfg=dispatch");
    }

    let mut h = Fnv::default();
    let mut files = Vec::new();
    list(Path::new("src"), &mut files);
    files.sort();
    for f in files.iter().map(Path::new).chain([Path::new("build.rs"), Path::new("Cargo.toml"), Path::new("Cargo.lock")]) {
        h.bytes(f.as_os_str().as_encoded_bytes());
        h.bytes(&std::fs::read(f).unwrap_or_default());
    }
    for var in ["TARGET", "PROFILE", "OPT_LEVEL", "DEBUG", "CARGO_CFG_TARGET_FEATURE", "CARGO_ENCODED_RUSTFLAGS"] {
        println!("cargo::rerun-if-env-changed={var}");
        h.bytes(var.as_bytes());
        h.bytes(std::env::var(var).unwrap_or_default().as_bytes());
    }
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = std::process::Command::new(rustc).arg("-vV").output().map(|o| o.stdout).unwrap_or_default();
    h.bytes(&version);
    println!("cargo::rerun-if-changed=src");
    println!("cargo::rerun-if-changed=Cargo.toml");
    println!("cargo::rerun-if-changed=Cargo.lock");
    println!("cargo::rustc-env=IMGFP_BUILD={:016x}{:016x}", h.0, h.1);

    let mut sources = Vec::new();
    for s in ANALYSIS_SOURCES {
        let p = Path::new(s);
        if p.is_dir() {
            list(p, &mut sources);
        } else {
            sources.push(p.to_path_buf());
        }
    }
    sources.retain(|p| p.file_name().is_some_and(|n| n != "tests.rs") && p.extension().is_some_and(|e| e == "rs"));
    sources.sort();
    let mut a = Fnv::default();
    // A line that is only a comment computes nothing, so an edit to the
    // documentation does not cost every cached analysis.
    let mut code_of = Vec::new();
    for f in &sources {
        a.bytes(f.as_os_str().as_encoded_bytes());
        let text = std::fs::read(f).unwrap_or_default();
        let code: Vec<&[u8]> = text.split(|&c| c == b'\n').filter(|l| !l.trim_ascii_start().starts_with(b"//")).collect();
        let code = code.join(&b'\n');
        a.bytes(&code);
        // A file's tests build no analysis, and name crates it does not use.
        let end = code.windows(TESTS.len()).position(|w| w == TESTS).unwrap_or(code.len());
        code_of.extend_from_slice(&code[..end]);
        code_of.push(b'\n');
    }
    let crates = analysis_crates(&code_of);
    for line in &crates {
        a.bytes(line.as_bytes());
    }
    for var in ["TARGET", "CARGO_CFG_TARGET_FEATURE"] {
        a.bytes(var.as_bytes());
        a.bytes(std::env::var(var).unwrap_or_default().as_bytes());
    }
    println!("cargo::rustc-env=IMGFP_ANALYSIS={:016x}{:016x}", a.0, a.1);
    let names: Vec<String> = sources.iter().map(|p| p.display().to_string()).collect();
    println!("cargo::rustc-env=IMGFP_ANALYSIS_SOURCES={}", names.join(":"));
    println!("cargo::rustc-env=IMGFP_ANALYSIS_CRATES={}", crates.join(","));
}

/// Where a source file's tests begin.
const TESTS: &[u8] = b"#[cfg(test)]\nmod tests";

/// Crates the analysis names that compute none of its values: errors' text,
/// the thread pool, the content hash (what a record is kept under, not what
/// it holds), the loader of libheif (whose own version each HEIF record
/// carries, `made_by`), and an exact byte search, each of which answers by
/// contract whatever its version; and what only builds the code (derive
/// macros' parsers, build scripts' probes), which a decoder's own version
/// already stands for. Left out wherever they are met.
const COMPUTES_NOTHING: [&str; 12] = [
    "anyhow", "rayon", "rayon-core", "blake3", "libloading", "memchr", "proc-macro2", "quote", "syn", "unicode-ident", "autocfg", "version_check",
];

/// The crates the analysis's code (`code`) is computed by, each as `name
/// version` from `Cargo.lock`, sorted: every dependency of the manifest the
/// code names (`name::`), less `COMPUTES_NOTHING`, and everything those
/// depend on. Not the whole lock file: a dependency of the matching or the
/// window updated is no reason to analyse every picture again.
fn analysis_crates(code: &[u8]) -> Vec<String> {
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap_or_default();
    let lock = std::fs::read_to_string("Cargo.lock").unwrap_or_default();
    // The manifest's `[dependencies]`, by name.
    let mut direct = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == "[dependencies]";
        } else if let Some(name) = t.split(['=', ' ', '.']).next().filter(|n| inside && !n.is_empty() && !n.starts_with('#')) {
            direct.push(name.to_string());
        }
    }
    let named = |name: &str| {
        let ident = format!("{}::", name.replace('-', "_"));
        code.windows(ident.len()).enumerate().any(|(i, w)| w == ident.as_bytes() && (i == 0 || !(code[i - 1].is_ascii_alphanumeric() || code[i - 1] == b'_')))
    };
    // The lock file's packages: name, version, and what each depends on
    // ("name" or "name version").
    let mut packages: Vec<(String, String, Vec<String>)> = Vec::new();
    for block in lock.split("[[package]]").skip(1) {
        let field = |k: &str| block.lines().find_map(|l| l.strip_prefix(&format!("{k} = \"")).map(|v| v.trim_end_matches('"').to_string()));
        let (Some(name), Some(version)) = (field("name"), field("version")) else { continue };
        let deps = block.split("dependencies = [").nth(1).map_or(Vec::new(), |d| {
            d[..d.find(']').unwrap_or(d.len())].split(',').map(|x| x.trim().trim_matches('"').to_string()).filter(|x| !x.is_empty()).collect()
        });
        packages.push((name, version, deps));
    }
    let find = |dep: &str| -> Vec<usize> {
        let mut words = dep.split(' ');
        let (name, version) = (words.next().unwrap_or(""), words.next());
        if COMPUTES_NOTHING.contains(&name) {
            return Vec::new();
        }
        (0..packages.len()).filter(|&k| packages[k].0 == name && version.is_none_or(|v| packages[k].1 == v)).collect()
    };
    let mut todo: Vec<usize> = direct.iter().filter(|d| named(d)).flat_map(|d| find(d)).collect();
    let mut seen = vec![false; packages.len()];
    while let Some(k) = todo.pop() {
        if std::mem::replace(&mut seen[k], true) {
            continue;
        }
        for d in &packages[k].2 {
            todo.extend(find(d));
        }
    }
    let mut out: Vec<String> = (0..packages.len()).filter(|&k| seen[k]).map(|k| format!("{} {}", packages[k].0, packages[k].1)).collect();
    out.sort();
    out
}

/// Every file under `dir`, as paths.
fn list(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            list(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Two FNV-1a hashes from different starts, as `matched.rs` keeps its own.
struct Fnv(u64, u64);

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325, 0x6c62_272e_07bb_0142)
    }
}

impl Fnv {
    fn bytes(&mut self, b: &[u8]) {
        const P: u64 = 0x0100_0000_01b3;
        self.0 = (self.0 ^ b.len() as u64).wrapping_mul(P);
        self.1 = (self.1 ^ b.len() as u64).wrapping_mul(P).rotate_left(5);
        for &x in b {
            self.0 = (self.0 ^ x as u64).wrapping_mul(P);
            self.1 = (self.1 ^ x as u64).wrapping_mul(P).rotate_left(5);
        }
    }
}
