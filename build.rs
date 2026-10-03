//! One cfg, `dispatch`, for an x86-64 build that cannot assume AVX2.
//!
//! The kernels are written for x86-64-v3 (AVX2, FMA, BMI2), and a build for a
//! machine that has them — `target-cpu=native`, or the release's `x86-64-v3` —
//! compiles them in directly. A build for plain x86-64 is what `cargo install`
//! makes, since it reads no `.cargo/config.toml` from the package, and there
//! the hot paths are compiled twice and chosen when the run starts; see
//! `src/simd.rs`. This names that case once, rather than spelling
//! `all(target_arch = "x86_64", not(target_feature = "avx2"))` at every place
//! it decides something.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(dispatch)");
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let features = std::env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    if arch == "x86_64" && !features.split(',').any(|f| f == "avx2") {
        println!("cargo::rustc-cfg=dispatch");
    }
}
