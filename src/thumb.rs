//! The thumbnail an analysis keeps for the pixel check: made from the
//! working plane (`Thumb::build`) and stored with the analysis. Here and not
//! in `verify.rs`, which reads it, because what it holds is part of what a
//! cached record depends on: see `analysis::VERSION`.

use crate::decode::Gray;

/// Thumbnail kept for pixel verification: small, blurred once at build time.
///
/// Carries a mip pyramid, because the pixel check compares two views of the
/// same region that are almost never at the same resolution. Reading both with
/// plain bilinear taps samples whichever side is finer far below its Nyquist
/// rate, and an aliased view does not correlate with a properly filtered one —
/// so the check reported disagreement for a difference the *sampling* had
/// introduced. The pyramid is derived from `px` and is not serialised.
///
/// Nor is it kept. It is built for each comparison that reads it, into
/// scratch the worker keeps (`Pyramid`), and that costs a few microseconds,
/// paid once for a run of comparisons on one side: a level is the one below
/// filtered by [1 3 3 1]/8 each way and halved (`Pyramid`). Kept on the thumbnail it was a third again of every
/// thumbnail, held from the first comparison to the end of the run — 103 MB
/// on a corpus of 27,000 pictures, nearly all of whose thumbnails are read
/// above level zero, and alive at the end of verification, which is where
/// that run peaks. The levels are a pure function of `px`, so when they are
/// made cannot change what they hold.
#[derive(Clone, Debug, Default)]
pub struct Thumb {
    pub w: u16,
    pub h: u16,
    /// Scale from working-image coordinates to thumbnail coordinates.
    pub scale: f32,
    /// In the heap, or in the run's scratch file (`spill`).
    pub px: crate::spill::Shared<u8>,
    /// Which thumbnail this is, for a worker's `Pyramid` to know it already
    /// holds its levels: unique to each `Thumb::new`, and shared only by a
    /// clone, which holds the same pixels. 0, a default thumbnail's, is none.
    pub id: u64,
}

impl Thumb {
    pub fn build(g: &Gray, long: usize) -> Thumb {
        // `fit_to` takes ownership, and the working image is wanted whole
        // elsewhere; resampling from a borrow avoids copying it first.
        let long_side = g.w.max(g.h);
        let owned;
        let t: &Gray = if long == 0 || long_side <= long {
            g
        } else {
            let s = long as f32 / long_side as f32;
            let tw = ((g.w as f32 * s).round() as usize).max(1);
            let th = ((g.h as f32 * s).round() as usize).max(1);
            owned = crate::decode::resize_area(g, tw, th);
            &owned
        };
        let scale = t.w as f32 / g.w as f32;
        // Stretched to the full byte range before it is rounded, so that the
        // pixel check's flatness bar is measured against the picture's own
        // contrast rather than against 255. A dim or foggy photograph spans a
        // few dozen codes; rounded as it stood, every block of it fell under
        // that bar, and an exact re-encode of it abstained its way to a score
        // of nothing. Min and max commute with inversion, so an inverted match
        // still compares like with like, and a picture that already spans the
        // whole range is rounded exactly as before.
        let (lo, hi) = t.px.iter().fold((1f32, 0f32), |(lo, hi), &v| {
            let v = v.clamp(0.0, 1.0);
            (lo.min(v), hi.max(v))
        });
        let (lo, inv) = if hi > lo { (lo, 1.0 / (hi - lo)) } else { (0.0, 1.0) };
        Thumb::new(
            t.w as u16,
            t.h as u16,
            scale,
            t.px.iter().map(|v| ((v.clamp(0.0, 1.0) - lo) * inv).clamp(0.0, 1.0) * 255.0 + 0.5).map(|v| v as u8).collect(),
        )
    }

    /// A thumbnail from its level zero, which is all there is of one.
    pub fn new(w: u16, h: u16, scale: f32, px: Vec<u8>) -> Thumb {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Thumb { w, h, scale, px: px.into(), id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
    }

    /// The same thumbnail, its pixels in the run's scratch file.
    pub fn spilled(self) -> Thumb {
        Thumb { px: crate::spill::share(crate::spill::Kind::Thumbs, self.px), ..self }
    }
}
