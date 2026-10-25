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
