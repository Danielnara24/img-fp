//! Which kernels a run uses, decided when it starts rather than when it was
//! built.
//!
//! The hot loops are written for x86-64-v3 — AVX2, FMA and BMI2, every Intel
//! core since Haswell and every Zen — in two ways: hand-written AVX2 kernels
//! (the vocabulary descent, the word-list filter, the pixel check's gathers,
//! the grey reduction), and plain loops the compiler vectorises eight lanes
//! wide when it may use AVX2 and four when it may not (the whole extractor,
//! most of the matcher).
//!
//! A build for a machine that has them gets both at compile time, and nothing
//! here costs it anything: `v3` is the constant `true` and a `dispatched!`
//! function is a call to the one it wraps. That is any build with
//! `target-cpu=native` on a modern machine, or one for `x86-64-v3`.
//!
//! **A build for plain x86-64 is the case this exists for**: it is the release
//! (since 0.30.0), and it is the one `cargo install img-fp` makes: `cargo install` reads no `.cargo/config.toml`
//! from the package it builds, so the `target-cpu=native` there never reaches
//! it. That build used to have neither half — no hand-written kernels, since
//! they were selected with `cfg(target_feature = "avx2")`, and four-lane
//! loops — and it cost **45% more CPU** to extract and **43% more** to match,
//! on a machine that had every feature the release uses. Now (`cfg(dispatch)`,
//! set by `build.rs`) the CPU is asked once, and:
//!
//! - the hand-written kernels are compiled on every x86-64 build and taken
//!   when `v3()` says the CPU has them;
//! - a function made with `dispatched!` has a second copy compiled for
//!   x86-64-v3, so that whatever it inlines is vectorised as the release's is.
//!   What it reaches is marked `#[cfg_attr(dispatch, inline(always))]`, since
//!   only what is inlined into that copy is compiled with its features.
//!
//! **The output does not depend on which copy ran.** Nothing in the crate asks
//! for floating-point contraction, so a multiply and an add are two roundings
//! whether or not FMA is available; vectorising a loop that adds independent
//! lanes changes no lane's arithmetic; and the hand-written kernels are held
//! to their portable twins, bit for bit, by the tests beside them.

/// Whether this process runs the x86-64-v3 kernels.
#[cfg(target_feature = "avx2")]
#[inline(always)]
pub const fn v3() -> bool {
    true
}

/// Whether this process runs the x86-64-v3 kernels: whether the CPU has every
/// feature `dispatched!` compiles for. Asked once.
#[cfg(all(dispatch, not(target_feature = "avx2")))]
#[inline]
pub fn v3() -> bool {
    use std::sync::atomic::{AtomicU8, Ordering};
    static STATE: AtomicU8 = AtomicU8::new(0);
    #[cold]
    fn ask() -> bool {
        let yes = std::arch::is_x86_feature_detected!("avx")
            && std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("bmi1")
            && std::arch::is_x86_feature_detected!("bmi2")
            && std::arch::is_x86_feature_detected!("f16c")
            && std::arch::is_x86_feature_detected!("fma")
            && std::arch::is_x86_feature_detected!("lzcnt")
            && std::arch::is_x86_feature_detected!("movbe")
            && std::arch::is_x86_feature_detected!("popcnt");
        STATE.store(if yes { 2 } else { 1 }, Ordering::Relaxed);
        yes
    }
    #[cfg(test)]
    if PLAIN.with(|p| p.get()) {
        return false;
    }
    match STATE.load(Ordering::Relaxed) {
        2 => true,
        1 => false,
        _ => ask(),
    }
}

#[cfg(all(test, dispatch, not(target_feature = "avx2")))]
thread_local! {
    /// Set by a test to run the copies a CPU without AVX2 runs, on one that
    /// has it; see `force_plain`.
    static PLAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Run what follows on this thread as a CPU without AVX2 would, so that the
/// two copies can be held to each other on a machine that has it.
#[cfg(all(test, dispatch, not(target_feature = "avx2")))]
pub fn force_plain(on: bool) {
    PLAIN.with(|p| p.set(on));
}

/// Whether this process runs the x86-64-v3 kernels: not on another
/// architecture.
///
/// Nor where `build.rs` and the compiler disagree about AVX2, which rustdoc
/// can make happen — it compiles the crate without `.cargo/config.toml`'s
/// flags, while the build script saw them. The portable paths are compiled
/// into every build without AVX2, so that case is merely slow.
#[cfg(not(any(target_feature = "avx2", dispatch)))]
#[inline(always)]
pub const fn v3() -> bool {
    false
}

/// A function that runs `$body` — a function of the same arguments — and,
/// under `cfg(dispatch)`, runs it compiled for x86-64-v3 when the CPU has it:
///
/// ```ignore
/// dispatched! {
///     pub fn extract(g: &Gray, p: &Params) -> Features => extract_any;
/// }
/// ```
///
/// The copy is a named function that calls `$body` directly, and `$body` is
/// `#[cfg_attr(dispatch, inline(always))]`, so it is compiled inside the copy.
/// It is not a closure on purpose: a closure was the first form, and the
/// compiler inlined the extractor into the closure, found the closure too
/// large to inline into the copy, and so ran all of it four lanes wide from
/// inside a function compiled for eight.
///
/// Without `cfg(dispatch)` the function is a call to `$body`.
macro_rules! dispatched {
    ($(#[$m:meta])* $vis:vis fn $name:ident($($a:ident: $t:ty),* $(,)?) $(-> $r:ty)? => $body:path;) => {
        $(#[$m])*
        $vis fn $name($($a: $t),*) $(-> $r)? {
            #[cfg(dispatch)]
            {
                #[target_feature(enable = "avx,avx2,bmi1,bmi2,f16c,fma,lzcnt,movbe,popcnt")]
                unsafe fn v3($($a: $t),*) $(-> $r)? {
                    $body($($a),*)
                }
                if $crate::simd::v3() {
                    // SAFETY: `v3()` has just said the CPU has every feature
                    // the copy is compiled for.
                    return unsafe { v3($($a),*) };
                }
            }
            $body($($a),*)
        }
    };
}
pub(crate) use dispatched;
