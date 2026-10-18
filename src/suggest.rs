//! Which file of each set of matches to keep, which to delete, and which to
//! look at first: the `action` every report states beside a file.
//!
//! **The rule it answers to is that a deletion loses no picture.** A file is
//! DELETE only when a file that stays shows all of it, or all but a thin
//! strip, at its detail or nearly, and the two were matched well enough to
//! say so. Everything the run cannot prove redundant stays: as REVIEW when it
//! was only ever matched weakly but closely enough to be a copy, as KEEP
//! otherwise. Measured against the benchmark corpora's generated ground
//! truth, none of IMGS-ALL's 16,873 deletions loses anything, and on its
//! families taken two files at a time (`benchmark/pairs.py`) 24 of 5,394,
//! where the `extra` test takes a reduction for an enlargement; the
//! measurement and the cases behind each piece are in CLAUDE.md.
//!
//! The pieces, in the order they are applied:
//!
//! - **A pair is clean** when no block of its overlap disagrees
//!   (`Policy::clean`'s bar). A pair that is not may still delete when it
//!   agrees at `WEAK_DELETES` overall; below that it decides nothing, and a
//!   negative or a mirror image never does. A file left reached only through such pairs is
//!   REVIEW when one of them agrees at `REVIEW` — a copy tinted or
//!   watermarked, which no pair could prove — and KEEP below it, where it is
//!   another photograph of the same subject.
//! - **"Holds"**: `g` holds `f` when all four corners of `f` land inside `g`
//!   and `g` does not shrink `f`'s picture by more than `SHRINK` on either
//!   axis, unless `f`'s extra pixels hold no detail an enlargement of `g`
//!   would lack, which is an upscale (`extra`). Two crops each missing a
//!   strip of under `CROPS` the other has count as one frame. Corners rather
//!   than `frame_overlap`, whose 16x16 grid read a caption strip of 15% as
//!   6%.
//! - **A deletion is checked over the deleted file's whole frame**
//!   (`whole_worst`), flat areas included, which the pixel check lets
//!   abstain. Without it, a caption bar's white strip "matched" the white
//!   background of a slide holding the same photograph.
//! - **A composite is never deleted but by a copy of itself, and never stands
//!   in for the plain picture.** A file that strictly holds a picture which
//!   has its own same-frame copies (`plain`) is a slide, a collage, a meme: it
//!   holds something nothing else does, and a thumbnail cannot check what.
//!   Weak pairs count for that, since a composite matches its photograph
//!   weakly. Where no file of a set is copied at its own frame — a photograph
//!   and a crop of it — the larger frame may delete what it holds, unless
//!   what it has besides reads as a canvas rather than more of the picture
//!   (`on_canvas`): a border, a caption bar, a slide.
//! - **Of files that hold each other, one is kept**: the one closest to the
//!   rest in grey (`TAU`), not softer than the rest, then the one leaving out
//!   least of the others, colour over grey, lossless over lossy, EXIF, and the
//!   smaller file. Pixel-identical files stand as one, the smallest.
//! - **Choosing needs a majority.** The keeper is the middle of what the
//!   files of a frame agree on, and two different pictures of one frame have
//!   no middle: an original and its re-encode, its edit, its recolouring look
//!   alike to every measure the run has, and file size or name order decided
//!   which went. So two pictures of a frame with no third, of which nothing
//!   measured says one shows more (`more`, or a size gap past `SHRINK` that
//!   `holds` tested), are neither deleted,
//!   and both are REVIEW (same bytes or same pixels count as one picture, and
//!   a library file, being kept whatever it is, decides as before). Taken two
//!   files at a time, IMGS-ALL's families lost content in 2,241 of 27,355
//!   pairs that way, the original deleted for its brightened, sharpened,
//!   recoloured or watermarked copy; now in 22 (and the composites' 133 in
//!   2, `on_canvas`). The price is REVIEW: 29,368 of the 54,710 files, from
//!   8,114, every lone re-encode or format conversion among them; on
//!   IMGS-ALL 3,004 from 2,499, and 386 fewer deletions.
//! - **What no check can see is not deleted.** A lossless file is deleted
//!   only for its own pixels, a larger frame or more detail, never for a
//!   file of its frame and size whose pixels differ (`own_pixels`): the
//!   difference may be finer than the run compares, as a changed figure on
//!   an invoice is. And a file of several pictures — an animation, a TIFF's
//!   pages — only for its own bytes, since only its first was matched. Such
//!   a file is kept, and nothing is deleted on its strength.
//! - **Deletion spreads from the keeps**, each file deleted by one already
//!   settled.
//!
//! How the pieces are put together: whether one file may delete another is
//! one question (`Component::refusal`), which decides nothing itself and
//! answers with its reason. What a reason makes of a file nothing deleted —
//! kept and deleting on, kept and deleting nothing, left for a look with its
//! twin, or judged by how closely it matched — is `Refusal`'s, the greatest
//! of a file's refusals deciding; and every file's action is read once, from
//! what settling found of it (`Facts::action`). A new rule is a refusal and
//! its place in that order. `revoke` takes a deletion back when what it
//! rests on is no longer the file the scan judged.

use rayon::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::decode::Traits;
use crate::verify::{Affine, Thumb, Verdict};

/// What the reports say to do with a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Keep,
    Delete,
    Review,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Keep => "KEEP",
            Action::Delete => "DELETE",
            Action::Review => "REVIEW",
        }
    }
}

/// Which rule the actions are suggested by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
// The variants' doc comments are the `--help` text of `--suggest`'s values.
pub enum Mode {
    /// DELETE only a file that a kept file shows all of, at about the same detail
    // `suggest`.
    #[default]
    Content,
    /// KEEP each representative, DELETE a file agreeing with one at 0.9 or more (at the default --min-pixel-correlation), KEEP one agreeing less, REVIEW one below --min-pixel-correlation
    // `by_group`; 0.9 is `CUT` of the way from the bar to 1.
    Correlation,
    /// KEEP each representative and DELETE everything else
    Representative,
}

/// Where, between the run's correlation bar and 1, `Mode::Correlation` stops
/// keeping and starts deleting: 0.9 at the default bar of 0.6. Swept on the
/// found corpus by eye, since its ground truth is no help (below): of
/// matches agreeing with their representative at 0.80-0.90, ten in sixteen
/// were another specimen of the species; at 0.90-0.95 one in sixteen; above
/// 0.95 none. On IMGS-ALL a correlation rule loses content at any cut, since
/// a collage, a slide or a crop's original agrees with its representative
/// over the part they share: 3,875 deletions lose some at 0.8, 3,183 at 0.9,
/// 987 at 0.98, against 3 for `Mode::Content`.
pub const CUT: f32 = 0.75;

/// One file's place in one group: the file, whether it is the group's
/// representative, and how well it agrees with it (1 for the same bytes or
/// pixels).
pub struct Seat {
    pub file: usize,
    pub representative: bool,
    pub correlation: f32,
}

/// The actions of `Mode::Correlation` and `Mode::Representative`, which read
/// nothing but the groups. One answer per file: a representative of any
/// group is kept, and any other file is judged by its best agreement with a
/// representative, since that one is kept.
pub fn by_group(mode: Mode, n: usize, seats: impl IntoIterator<Item = Seat>, min_correlation: f32) -> Vec<Option<Action>> {
    let mut rep = vec![false; n];
    let mut best = vec![None::<f32>; n];
    for s in seats {
        if s.representative {
            rep[s.file] = true;
        } else {
            best[s.file] = Some(best[s.file].map_or(s.correlation, |b| b.max(s.correlation)));
        }
    }
    let cut = min_correlation + CUT * (1.0 - min_correlation);
    (0..n)
        .map(|i| {
            if rep[i] {
                return Some(Action::Keep);
            }
            let c = best[i]?;
            Some(match mode {
                Mode::Representative => Action::Delete,
                _ if c >= cut => Action::Delete,
                _ if c >= min_correlation => Action::Keep,
                _ => Action::Review,
            })
        })
        .collect()
}

/// What the suggestion reads of one file.
#[derive(Clone, Copy)]
pub struct Picture<'a> {
    /// The picture's own size, and its working image's.
    pub dims: (u32, u32),
    pub work: (u32, u32),
    pub thumb: &'a Thumb,
    pub traits: &'a Traits,
    pub bytes: u64,
}

/// One stated pair.
pub enum Link<'a> {
    /// The same bytes, or the same decoded pixels.
    Exact,
    /// A verified match: its verdict, the transform from the first file's
    /// working image to the second's, and whether one is the other's negative.
    Edge(&'a Verdict, Affine, bool),
}

/// The bar a strong pair's worst block must clear: the clean-anchor bar as
/// it was (`Policy::clean`), not as it is. When that bar rose to 0.9 this was
/// swept on IMGS-ALL beside it: at 0.85, 0.9 and 0.95 the same deletions
/// lose content and the same families keep their original (276) and a
/// faithful copy (304), and only the count of deletions moves (18,183,
/// 18,158, 18,158) with the REVIEWs (1,758, 2,181, 3,164), measured with the
/// whole-frame check at 32 samples a side (`WHOLE_GRID`). A worst block that
/// clusters nothing new can still say a file is redundant.
pub(crate) const CLEAN: f32 = 0.85;
/// How far outside the other frame a corner may land, as a fraction of it,
/// plus `TOL_PX` of the file's own pixels: the transform is fitted, and a
/// thumbnail of a hundred pixels is a few pixels off at four times its size.
const TOL: f32 = 0.02;
const TOL_PX: f32 = 3.0;
/// Below this many octaves (log2) of shrinking, two files are the same size:
/// a 15% resample, one axis or both. Smaller resamples than this hold too
/// little difference in detail for `extra` to measure: a copy squashed 9%
/// to fit a square, or one carrying a watermark, read as holding real detail
/// its original lacked, and neither could be deleted.
const SAME: f32 = 0.2;
/// How many octaves a keeper may shrink a file by, on either axis, before
/// the file's extra pixels are asked whether they hold real detail (`extra`):
/// 1.4 times. A collection of small pictures is full of crops stretched back
/// to the same square, and read that way each crop held "detail" its fuller
/// picture lacked and nothing was deleted. On the benchmark corpora it moves
/// no deletion that loses content.
const SHRINK: f32 = 0.5;
/// How much of each of two frames may lie outside the other, as a fraction
/// of its area, for the two to count as one frame: two crops of a picture,
/// each missing a thin strip the other has. 15% lost twice the composites
/// for a few more deletions.
const CROPS: f32 = 0.10;
/// The correlation below which a file nothing could delete is not a REVIEW
/// but a KEEP: under it, the found corpus's REVIEWs were mostly two
/// specimens of one species, and over it mostly one photograph tinted or
/// watermarked.
pub(crate) const REVIEW: f32 = 0.9;
/// The correlation at which a pair that is not clean — one block of it
/// disagrees — may still delete. Re-encodes carrying a watermark, or a
/// background recoloured, fail the clean bar and agree at 0.96 overall; two
/// different photographs of one subject reached 0.963 in the found corpus,
/// and are stopped by the whole-frame check rather than by this.
const WEAK_DELETES: f32 = 0.95;
/// How much more fine detail (log2) a file's extra pixels must hold, per
/// octave of size and plus a floor, to count as real rather than an
/// enlargement's (`extra`).
///
/// Measured on seeds from all four corpora, each an original against its
/// own downscales (1.5, 2 and 3 times: real detail) and against enlargements
/// of its third-size copy (bilinear, bicubic and Lanczos at 1.5 to 3 times,
/// JPEG at q86-97 or PNG, and nearest at 2: none). An enlargement reads up
/// to 0.3 octave a doubling at p99, Lanczos and JPEG noise most; a real
/// original 0.44 a doubling at p01 on IMGS3+IMGS4, and on IMGS+IMGS2 the
/// same except for seeds that are themselves soft, whose half-size round
/// trip is ~40 dB and whose extra pixels so hold nothing. It was 0.1,
/// unmeasured, under which 6-9% of the enlargements read as real detail:
/// a 3x Lanczos upscale was kept as the fuller copy and its original
/// deleted, and against a library it was `better`. Swept on IMGS-ALL,
/// families keeping their original at 0.1, 0.15, 0.2, 0.25, 0.3, 0.4:
/// 276, 296, 297, 297, 298, 298, none losing content. The corpora cannot
/// see the other side: above 0.25 the originals above start to read as
/// enlargements. 0.2 is where the plateau starts, with the most margin to
/// them.
const EXTRA_PER_OCTAVE: f32 = 0.2;
const EXTRA_FLOOR: f32 = 0.03;
/// How close in grey (summed over the 4x4 grid, 0..=255 a cell) a file must
/// be to the group's middle to be a candidate for keeping: as close as two
/// files holding the same pixels are. The grid is read from each file's own
/// decode, and a JPEG's grey is its Y plane where a PNG's is BT.601 from RGB,
/// so the same pixels differ: over every pair of an original and its exact,
/// stripped, PNG, TIFF and lossless WebP copies on IMGS-ALL, by 1.07 at p90
/// and 3.11 at p99. Tone edits sit far out (brightness, gamma, contrast and
/// grading: at most 1% within 3); those that move colour and not luma,
/// which the grid cannot see at any bar, up to 7%. Swept on IMGS-ALL, 2 to 3
/// is a plateau (272-274 families keep their original, 301 a faithful copy)
/// with a cliff each side: 0.5 drops lossless copies over decode noise (241),
/// 8 lets edits in to win on size (255), no bar at all 90.
const TAU: f32 = 3.0;
/// How much softer than the candidates' median a candidate may be: as much
/// softer as a JPEG re-encode at quality 84 to 96 is than its original, at
/// p99 (0.047), so that such a copy ties with it. At 0 a lossless copy drops
/// out over its decode's own noise (0.015 at most) and only 164 families
/// keep their original; 0.025 to 0.05 is a plateau, and wider erodes it
/// slowly (270 with no bar), as blurred copies come in.
const SOFT_TIE: f32 = 0.05;
/// How much softer than a file another of its size may be and still delete
/// it (`Ctx::softer`).
///
/// **The keeper is chosen by majority** — the grey middle (`TAU`) and the
/// softness median (`SOFT_TIE`) — and on its own a majority can be wrong:
/// an original among nine enlargements of its own third-size copy, each
/// as large as it and softer, was deleted for one of them in 177 of 178
/// families. Sharpness cannot say which file is the original instead: a
/// noisy, halftoned or sharpened copy reads sharper than its original
/// (to -1.5), so a rule keeping the sharpest keeps the edit. What it can
/// say is which deletions lose detail, and this refuses those, whoever was
/// chosen: a file held back only by it is kept, and deletion goes on from
/// it. It only ever removes a deletion.
///
/// Measured on IMGS-ALL's same-size copies against their originals: faithful
/// lossy re-encodes soften by 0.15-0.16 at p95 (jpeg_mid, to_jxl,
/// jpeg_magick), so below that a softer re-encode could not stand in for a
/// sharper one; the soft enlargements above read 0.21-0.29 at p01 (IMGS3+4).
/// Swept at 0.15, 0.2, 0.25, 0.3, and off: originals deleted in those sets
/// 5+1, 9+1, 15+7, 22+16, 68+110 (IMGS+IMGS2, IMGS3+IMGS4: what is left
/// at 0.15 is soft seeds, whose downscale holds them, and a text page); on
/// IMGS-ALL 17,259, 17,271, 17,286, 17,288, 17,300 DELETE, REVIEW
/// 2,496-2,499, 297 / 304 originals and no content lost at every one.
const SOFT_GAP: f32 = 0.15;
/// The most a block of the whole-frame check may disagree, in grey levels.
const WHOLE_RESIDUAL: f32 = 12.0;
/// Samples a side of the whole-frame check (`whole_worst`), which is also
/// the blur it reads each frame at: a sixty-fourth of the frame.
///
/// It was 32, and a thirty-second of the frame blurs thin lines and stripes
/// into an even haze. That was safe only while the fitted transforms were a
/// few pixels out: once they fitted (`verify::best_transform`), two
/// composites of one photograph — a contact sheet whose middle cell is where
/// an embed has its photograph, an inset on a striped canvas beside a
/// photograph on a plain one — registered exactly, their canvases' faint
/// lines read as the other's flat background, and each was deleted for the
/// other. Swept on IMGS-ALL, deletions losing content at 32, 48, 64 and 96:
/// 3, 1, 0, 0, for 18,183, 17,759, 17,293 and 16,002 deletions, the same 276
/// families keeping their original and all 304 a faithful copy. 64 is the
/// coarsest with no loss, so the one that gives up fewest deletions; it also
/// ended the one loss there was before (a browser window over a PDF page).
/// It costs some 0.6 s of a cached run over IMGS.
const WHOLE_GRID: usize = 64;
/// Below this, `Traits::colour` is a grey picture: above the most a grey one
/// reads. Through every codec the benchmark corpus has, a grey seed reads
/// 0.1 at most, and with the strongest noise its generator adds, 1.31 — that
/// last through zune-jpeg, whose chroma upsampling rounds up a quarter level
/// where chroma varies. A faint real tint below it (0.6 to 1.2 in the corpus)
/// counts as grey, which costs nothing: a grey copy of such a picture loses
/// next to nothing. Swept on IMGS-ALL, 1.5 to 3 is flat (271-272 families
/// keep their original, 301-302 a faithful copy), and 1 is below the floor.
pub(crate) const COLOURFUL: f32 = 1.5;

/// One strong pair, seen from one end `f`.
#[derive(Clone, Copy)]
struct Side {
    to: u32,
    link: u32,
    /// Whether `f` is the link's first file.
    forward: bool,
    exact: bool,
    /// All of `f`'s frame lands in `to`'s, and the reverse.
    f_in: bool,
    in_f: bool,
    /// How much `to` enlarges `f` along each of `f`'s axes, in octaves of
    /// the files' own pixels.
    sx: f32,
    sy: f32,
    /// The fraction of `f`'s frame that lands outside `to`'s, and the reverse.
    f_out: f32,
    /// The pair is clean: no block of its overlap disagrees.
    clean: bool,
}

/// The action for every file in a pair, by index; `None` for the rest.
#[cfg(test)]
pub fn suggest(pics: &[Picture], links: &[(usize, usize, Link)], min_correlation: f32, colour: impl Fn(usize) -> f32 + Sync) -> Vec<Option<Action>> {
    suggest_pinned(pics, links, min_correlation, colour, &[]).0
}

/// `suggest`, with some files kept whatever the rest say, and beside each
/// DELETE the file it rests on: the one that holds it and stays.
///
/// A pinned file is `--against`'s library. It is KEEP, it is never deleted,
/// and where it holds the same frame as files that are not pinned it is the
/// one of them kept, so a picture the library has is answered by the
/// library's copy rather than by a copy among the files being asked about.
/// Deletion spreads from the pinned files first. `pinned` may be empty, and
/// then nothing is.
pub fn suggest_pinned(
    pics: &[Picture],
    links: &[(usize, usize, Link)],
    min_correlation: f32,
    colour: impl Fn(usize) -> f32 + Sync,
    pinned: &[bool],
) -> (Vec<Option<Action>>, Vec<Option<usize>>) {
    let n = pics.len();
    let mut sides: Vec<Vec<Side>> = vec![Vec::new(); n];
    // Every pair, weak ones too, for what the frames say about which file is
    // a composite: a composite usually matches its photograph weakly, since
    // the photograph is small inside it.
    let mut all_sides: Vec<Vec<Side>> = vec![Vec::new(); n];
    for (li, (a, b, link)) in links.iter().enumerate() {
        let (pa, pb) = (&pics[*a], &pics[*b]);
        // Working pixels to the file's own, for each.
        let shrink = |p: &Picture| {
            let file = p.dims.0.max(p.dims.1) as f32;
            if file > 0.0 { p.work.0.max(p.work.1) as f32 / file } else { 1.0 }
        };
        let k = shrink(pa) / shrink(pb);
        let (fwd, back) = match link {
            Link::Exact => {
                let ls = k.max(1e-6).log2();
                let s = Side { to: *b as u32, link: li as u32, forward: true, exact: true, f_in: true, in_f: true, sx: ls, sy: ls, f_out: 0.0, clean: true };
                (s, Side { to: *a as u32, forward: false, sx: -ls, sy: -ls, ..s })
            }
            Link::Edge(v, m, inverted) => {
                let clean = v.blk_min >= CLEAN;
                if *inverted {
                    continue;
                }
                let sx = (hypot(m[0], m[3]) * k).max(1e-6).log2();
                let sy = (hypot(m[1], m[4]) * k).max(1e-6).log2();
                let mi = invert(m);
                let a_out = outside(m, pa.work, pb.work);
                let b_out = mi.map_or(1.0, |mi| outside(&mi, pb.work, pa.work));
                let a_in_b = out_by(m, pa.work, pb.work) <= TOL + TOL_PX / pa.dims.0.min(pa.dims.1).max(1) as f32;
                let b_in_a = mi.is_some_and(|mi| out_by(&mi, pb.work, pa.work) <= TOL + TOL_PX / pb.dims.0.min(pb.dims.1).max(1) as f32);
                // Two crops of one picture, each missing a strip the other has:
                // one frame. A composite holds its picture whole, so it is
                // never this; nor is a pair too weak to delete on, or two
                // specimens framed alike would make every file "plain".
                let deletes = !mirrored(m) && v.blk >= min_correlation && (clean || v.blk >= WEAK_DELETES);
                let mutual = deletes && !a_in_b && !b_in_a && a_out <= CROPS && b_out <= CROPS;
                let (a_in_b, b_in_a) = (a_in_b || mutual, b_in_a || mutual);
                let s = Side { to: *b as u32, link: li as u32, forward: true, exact: false, f_in: a_in_b, in_f: b_in_a, sx, sy, f_out: a_out, clean };
                (s, Side { to: *a as u32, forward: false, f_in: b_in_a, in_f: a_in_b, sx: -sx, sy: -sy, f_out: b_out, ..s })
            }
        };
        all_sides[*a].push(fwd);
        all_sides[*b].push(back);
        let usable = match link {
            Link::Exact => true,
            // A mirror image settles nothing either way, as a negative does
            // not. It shows all of the picture, turned the other way round,
            // and which of the two is the right way round is not something a
            // pair can say: the rule chose between them on quality, so a
            // mirrored re-encode a little better than its original was kept
            // and the original deleted — 14 of 19 photographs, a flip saved at
            // quality 95 beside the original at 92. Left to a person, as
            // REVIEW when it agrees as closely as a copy does.
            Link::Edge(v, m, _) => {
                !mirrored(m) && v.blk >= min_correlation && (fwd.clean || v.blk >= WEAK_DELETES)
            }
        };
        if usable {
            sides[*a].push(fwd);
            sides[*b].push(back);
        }
    }
    for s in sides.iter_mut().chain(all_sides.iter_mut()) {
        s.sort_by_key(|e| e.to);
        s.dedup_by_key(|e| e.to);
    }
    // Strong components, each settled on its own.
    let mut comp = vec![usize::MAX; n];
    let mut components: Vec<Vec<usize>> = Vec::new();
    for s0 in 0..n {
        if comp[s0] != usize::MAX || sides[s0].is_empty() {
            continue;
        }
        let c = components.len();
        comp[s0] = c;
        let mut members = vec![s0];
        let mut q = vec![s0];
        while let Some(x) = q.pop() {
            for e in &sides[x] {
                let y = e.to as usize;
                if comp[y] == usize::MAX {
                    comp[y] = c;
                    members.push(y);
                    q.push(y);
                }
            }
        }
        members.sort_unstable();
        components.push(members);
    }
    let ctx = Ctx { pics, links, sides: &sides, all_sides: &all_sides, pinned };
    let mut facts: Vec<Option<Facts>> = vec![None; n];
    for (a, b, link) in links {
        for f in [*a, *b] {
            let e = facts[f].get_or_insert_with(Facts::default);
            if let Link::Edge(v, _, false) = link {
                e.best = e.best.max(v.blk);
            }
        }
    }
    let settled: Vec<Vec<(usize, Facts)>> = components.par_iter().map(|m| ctx.settle(m, &colour)).collect();
    for (i, f) in settled.into_iter().flatten() {
        let best = facts[i].map_or(0.0, |e| e.best);
        facts[i] = Some(Facts { best, ..f });
    }
    let action = (0..n).map(|i| facts[i].map(|f| f.action(ctx.is_pinned(i)))).collect();
    let keeper = facts.iter().map(|f| f.and_then(|f| f.deleted_by())).collect();
    (action, keeper)
}

/// What settling found of one file in a pair, from which its action is read
/// (`Facts::action`). A file in no strong component is never settled: its
/// facts are the default, an open file nothing refused.
#[derive(Clone, Copy, Debug, Default)]
struct Facts {
    state: State,
    /// The weightiest reason a settled file gave for not deleting it, while
    /// it was open (`Refusal`'s order).
    refused: Option<Refusal>,
    /// A deletion rests on it.
    deletes: bool,
    /// One of its strong pairs is clean.
    clean: bool,
    /// Its class is two pictures of a frame that nothing decided between
    /// (`Refusal::Undecided`).
    undecided: bool,
    /// The best correlation of any pair it is in, negatives aside.
    best: f32,
}

/// Where spreading deletion left a file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    /// Neither kept nor deleted: nothing that settled could delete it, and
    /// nothing chose it.
    #[default]
    Open,
    /// Kept, and deletion spread from it.
    Keep,
    /// Deleted for the file it names.
    Delete(usize),
}

/// Why a file that holds another does not delete it (`Component::refusal`), by
/// what that makes of the file if nothing else settles it. A file several
/// files refused for different reasons is what the greatest makes it, and
/// they are in order of caution: a file one refusal would keep and delete on
/// from, another would keep and delete nothing on the strength of, and a
/// third would leave for a look, is left for a look. Up to 0.51 `Softer`
/// came last, unwritten, and a file held or undecided that a softer copy
/// refused too was kept and deleted others: 112 of 40,000 random families
/// of up to six files moved when it was put first. No corpus has the case;
/// IMGS-ALL, IMGS-PROVENANCE and `pairs.py` are unchanged to the byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Refusal {
    /// It does not hold it, or not in a shape that may delete, or not over
    /// the whole frame. The file stays, REVIEW or KEEP by its best match.
    No,
    /// It is softer (`Ctx::softer`): the file holds detail nothing settled
    /// has, so it is kept and deletion goes on from it.
    Softer,
    /// What no check can show another file holds: more pictures than the
    /// first (`Traits::frames`), or a lossless file's own pixels
    /// (`Ctx::own_pixels`). Kept, and nothing is deleted on its strength.
    Held,
    /// One of two pictures of one frame and nothing says which is the
    /// original: both are REVIEW.
    Undecided,
}

impl Facts {
    fn deleted_by(&self) -> Option<usize> {
        match self.state {
            State::Delete(k) => Some(k),
            _ => None,
        }
    }

    /// The file's action, whole: what spreading made of it, then what it
    /// was matched to says of a file that stays.
    fn action(&self, pinned: bool) -> Action {
        if pinned {
            return Action::Keep;
        }
        let a = match (self.state, self.refused) {
            (State::Delete(_), _) => return Action::Delete,
            (State::Keep, _) | (State::Open, Some(Refusal::Held)) => Action::Keep,
            (State::Open, _) => Action::Review,
        };
        // Of an undecided pair, whatever it was chosen as, unless a deletion
        // rests on it, which makes it the copy to keep. It is a strong match,
        // not another photograph.
        if self.undecided && !self.deletes {
            return Action::Review;
        }
        // A file that stays and was only ever matched weakly is for a person
        // to look at, whatever it was chosen as, unless a deletion rests on
        // it.
        let a = if a == Action::Keep && !self.deletes && !self.clean { Action::Review } else { a };
        // But one whose best match agrees less than `REVIEW` is another
        // photograph of something like it, not a copy: there is nothing to
        // look at, and it stays.
        if a == Action::Review && self.best < REVIEW {
            Action::Keep
        } else {
            a
        }
    }
}

/// Whether a transform turns its picture over: a left-right or top-bottom
/// flip, alone or with any rotation.
fn mirrored(m: &Affine) -> bool {
    m[0] * m[4] - m[1] * m[3] < 0.0
}

/// For each DELETE, the file that stays and holds it: the end of its chain
/// of keepers. Deletion spreads one holder at a time, so the file a deletion
/// rests on (`suggest_pinned`'s second answer) may be a DELETE itself, and
/// named as the copy that is kept it was a file suggested for deletion too:
/// 52 of 337 DELETEs on `derived/Desktop`. `None` where `keeper` has none,
/// or where the chain does not end on a file that stays.
pub fn kept_at_end(actions: &[Option<Action>], keeper: &[Option<usize>]) -> Vec<Option<usize>> {
    (0..keeper.len())
        .map(|i| {
            if actions.get(i).copied().flatten() != Some(Action::Delete) {
                return None;
            }
            let mut end = keeper[i]?;
            // A chain visits each file at most once; a longer walk is a loop.
            for _ in 0..keeper.len() {
                if actions.get(end).copied().flatten() != Some(Action::Delete) {
                    return Some(end);
                }
                end = keeper.get(end).copied().flatten()?;
            }
            None
        })
        .collect()
}

/// Why a file is no longer the file a scan judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lapse {
    /// It is not there.
    Gone,
    /// It has another length, or changed after the scan started
    /// (`last::changed_since`).
    Changed,
}

/// Take back, as REVIEW resting on nothing, each DELETE that no longer
/// stands: one whose kept copy (`kept`, the end of its chain:
/// `kept_at_end`) has lapsed, since carried out it would take the last copy
/// of what it shows, and one that has lapsed itself, which is not the file
/// the scan judged. `kept` may be shorter than `actions`, or empty, as for
/// the group rules, which name no kept copy. The deletions taken back for
/// their kept copy, and why it lapsed.
///
/// `--last` and the window both ask this, of a scan read back and of files
/// gone to the Trash or changed since.
pub fn revoke(actions: &mut [Option<Action>], kept: &mut [Option<usize>], lapsed: impl Fn(usize) -> Option<Lapse>) -> Vec<(usize, Lapse)> {
    let mut unheld = Vec::new();
    for (i, a) in actions.iter_mut().enumerate() {
        if *a != Some(Action::Delete) {
            continue;
        }
        let copy = kept.get(i).copied().flatten().and_then(&lapsed);
        if copy.is_none() && lapsed(i).is_none() {
            continue;
        }
        *a = Some(Action::Review);
        if let Some(k) = kept.get_mut(i) {
            *k = None;
        }
        unheld.extend(copy.map(|why| (i, why)));
    }
    unheld
}

struct Ctx<'a> {
    pics: &'a [Picture<'a>],
    links: &'a [(usize, usize, Link<'a>)],
    sides: &'a [Vec<Side>],
    all_sides: &'a [Vec<Side>],
    /// See `suggest_pinned`.
    pinned: &'a [bool],
}

impl Ctx<'_> {
    fn is_pinned(&self, f: usize) -> bool {
        self.pinned.get(f).copied().unwrap_or(false)
    }

    fn side(&self, f: usize, g: usize) -> Option<&Side> {
        let s = &self.sides[f];
        s.binary_search_by_key(&(g as u32), |e| e.to).ok().map(|k| &s[k])
    }

    /// All of `f`'s frame lands in `g`'s.
    fn frame(&self, f: usize, g: usize) -> bool {
        self.side(f, g).is_some_and(|e| e.exact || e.f_in)
    }

    fn same(&self, f: usize, g: usize) -> bool {
        self.frame(f, g) && self.frame(g, f)
    }

    fn strict_in(&self, f: usize, g: usize) -> bool {
        self.frame(f, g) && !self.frame(g, f)
    }

    /// `k` is the size of `d` and softer than it by more than `SOFT_GAP`:
    /// it does not show `d` at `d`'s detail, so it may not stand in for it.
    fn softer(&self, k: usize, d: usize) -> bool {
        let Some(e) = self.side(d, k) else { return false };
        !e.exact && e.sx.abs() <= SAME && e.sy.abs() <= SAME && softness(self.pics[k].traits) - softness(self.pics[d].traits) > SOFT_GAP
    }

    /// `d` is a lossless file of `k`'s frame and size whose pixels are not
    /// `k`'s (not the same bytes, nor the same decoded pixels): two
    /// pictures, as far as anything says.
    ///
    /// **The run cannot see what tells two such files apart.** Every check
    /// it has reads the picture at a thumbnail's 128 pixels, or at a
    /// sixty-fourth of the frame, and its keypoints at the working size: two
    /// screenshots of an invoice, one total reading $1,250 and the other
    /// $9,750, agreed at 1.00 and shared every keypoint at distance 0, and of
    /// an invoice, its re-encode and the other invoice, the other was deleted;
    /// a line of text changed, the original was. A lossy file differs from
    /// its source by the codec everywhere, so its pixels say nothing either
    /// way; a lossless one differs only where it was changed. No number is
    /// read here: the formats say it.
    fn own_pixels(&self, k: usize, d: usize) -> bool {
        self.pics[d].traits.lossless
            && self.side(d, k).is_some_and(|e| !e.exact && e.sx.abs() <= SAME && e.sy.abs() <= SAME)
            && self.same(k, d)
    }

    /// `k`, which strictly holds `d`, is `d` on a canvas: what `k` shows
    /// besides `d` holds no picture.
    ///
    /// **Read off the detail both files already measured.** Over its whole
    /// frame `k`'s detail is `d`'s where `d` lies and the rest's elsewhere,
    /// so if the rest is flat `k` reads `f` times `d`'s, `f` the share of
    /// `k`'s frame that is `d`; if it is more of the picture, about `d`'s
    /// own. `k` is a canvas when it is nearer the first, split at their
    /// geometric middle, `sqrt(f)`. Eight pixels apart, the coarsest the
    /// measure has: that is the structure a picture's content holds, which
    /// two renderings of one region share, where pixel-scale detail is what
    /// re-encoding, noise and resampling move. At one pixel apart a third of
    /// slightly cropped photographs read as canvases.
    ///
    /// It decides only what nothing else can (`settle`'s `lone`): a set with
    /// no picture copied at its own frame. On the pairs benchmark
    /// (`benchmark/pairs.py`), composites deleted their original 971 times,
    /// 133 of them losing content; with this, 40 and 2 — a caption bar of a
    /// tenth of the frame is near the split — while crops and turns kept
    /// 2,729 of their 3,014 deletions. On IMGS-ALL it never applies: every
    /// family there has its picture copied at its own frame.
    fn on_canvas(&self, k: usize, d: usize) -> bool {
        let Some(e) = self.side(d, k) else { return false };
        let (pd, pk) = (&self.pics[d], &self.pics[k]);
        let area = |p: &Picture| p.dims.0 as f32 * p.dims.1 as f32;
        if area(pk) <= 0.0 || area(pd) <= 0.0 {
            return false;
        }
        let f = (area(pd) * (e.sx + e.sy).exp2() / area(pk)).clamp(1e-3, 1.0);
        // How many of `d`'s pixels one of `k`'s spans.
        let s = (-(e.sx + e.sy) / 2.0).exp2();
        let (td, tk) = (&pd.traits.detail, &pk.traits.detail);
        tk[3] / at_distance(&td[..4], 8.0 * s).max(1e-4) < f.sqrt()
    }

    /// `g` shows all of `f`'s picture, at `f`'s detail or finer.
    fn holds(&self, g: usize, f: usize) -> bool {
        let Some(e) = self.side(f, g) else { return false };
        if e.exact {
            return true;
        }
        if !e.f_in {
            return false;
        }
        let l = e.sx.min(e.sy);
        !(-l > SHRINK && extra(self.pics[g].traits, self.pics[f].traits, (-l).exp2()) > EXTRA_PER_OCTAVE * -l + EXTRA_FLOOR)
    }

    /// `g` holds `f` and shows more of it: more frame, or real detail `f`
    /// lacks.
    fn more(&self, g: usize, f: usize) -> bool {
        if !self.holds(g, f) {
            return false;
        }
        let e = self.side(f, g).unwrap();
        if e.exact {
            return false;
        }
        if !e.in_f {
            return true;
        }
        let l = e.sx.min(e.sy);
        l > SAME && extra(self.pics[f].traits, self.pics[g].traits, l.exp2()) > EXTRA_PER_OCTAVE * l + EXTRA_FLOOR
    }

    /// The transform from `f`'s working image to `g`'s.
    fn transform(&self, f: usize, g: usize) -> Option<Affine> {
        let e = self.side(f, g)?;
        match &self.links[e.link as usize].2 {
            Link::Exact => None,
            Link::Edge(_, m, _) => if e.forward { Some(*m) } else { invert(m) },
        }
    }

    /// Settle one strong component: keep one file of each class, spread
    /// deletion from what is kept, and say what each member came to.
    fn settle(&self, members: &[usize], colour: &(impl Fn(usize) -> f32 + Sync)) -> Vec<(usize, Facts)> {
        let c = Component::new(self, members);
        let mut state: HashMap<usize, State> = HashMap::new();
        let mut refused: HashMap<usize, Refusal> = HashMap::new();
        let mut deletes: HashSet<usize> = HashSet::new();
        // The pinned files are kept before anything is chosen, and deletion
        // spreads from them first: a file both a pinned file and another
        // could delete is the pinned one's.
        let mut q: VecDeque<usize> = members.iter().copied().filter(|&f| self.is_pinned(f)).collect();
        // Each class's choice, kept unless a file of another class may delete
        // it. A class holding a pinned file keeps that, kept already.
        let mut chosen = Vec::with_capacity(c.classes.len());
        for (ci, class) in c.classes.iter().enumerate() {
            if class.iter().any(|&f| self.is_pinned(f)) {
                continue;
            }
            let k = self.choose(class, colour);
            chosen.push(k);
            if !self.sides[k].iter().any(|e| c.class[&(e.to as usize)] != ci && c.refusal(e.to as usize, k).is_none()) {
                q.push_back(k);
            }
        }
        for &k in &q {
            state.insert(k, State::Keep);
        }
        loop {
            // Every settled file deletes what it may of the open files it is
            // paired with; each refusal is noted against the file refused.
            while let Some(x) = q.pop_front() {
                for e in &self.sides[x] {
                    let y = e.to as usize;
                    if state.contains_key(&y) {
                        continue;
                    }
                    match c.refusal(x, y) {
                        None => {
                            state.insert(y, State::Delete(x));
                            deletes.insert(x);
                            q.push_back(y);
                        }
                        Some(r) => {
                            let w = refused.entry(y).or_insert(r);
                            *w = (*w).max(r);
                        }
                    }
                }
            }
            // Kept, and deletion goes on from them: a class's choice that a
            // file of another class could delete and no settled file did
            // (what could have was never settled itself), and a file only a
            // softer one could have deleted (`Refusal::Softer`).
            q.extend(chosen.iter().copied().filter(|k| !state.contains_key(k)));
            let mut softer: Vec<usize> =
                refused.iter().filter(|&(d, &r)| r == Refusal::Softer && !state.contains_key(d) && !q.contains(d)).map(|(&d, _)| d).collect();
            softer.sort_unstable();
            q.extend(softer);
            if q.is_empty() {
                break;
            }
            for &k in &q {
                state.insert(k, State::Keep);
            }
        }
        // Which pinned file a deletion is said to rest on. Deletion spreads
        // from whichever settled file reaches a file first, and against a
        // library that was as often a greyscale or sharpened copy as the
        // library's plain one: true, and the wrong file to show someone as
        // the copy they already have. So a deleted file that pinned files can
        // delete names the one `choose` would keep of them — colour over grey,
        // not softer than the rest — tried in that order until one passes.
        // The actions are already settled; only the name changes.
        if members.iter().any(|&f| self.is_pinned(f)) {
            for &y in members {
                if !matches!(state.get(&y), Some(State::Delete(_))) {
                    continue;
                }
                let mut cands: Vec<usize> = self.sides[y].iter().map(|e| e.to as usize).filter(|&x| self.is_pinned(x)).collect();
                while !cands.is_empty() {
                    let k = self.choose(&cands, colour);
                    if c.refusal(k, y).is_none() {
                        state.insert(y, State::Delete(k));
                        break;
                    }
                    cands.retain(|&c| c != k);
                }
            }
        }
        // A class of two pictures left open for want of a third is for a
        // person to look at, both of it: the one `choose` picked is no
        // likelier the original than the other.
        let mut undecided: HashSet<usize> = HashSet::new();
        for (d, &r) in &refused {
            if r == Refusal::Undecided && !state.contains_key(d) {
                undecided.extend(c.classes[c.class[d]].iter().copied());
            }
        }
        members
            .iter()
            .map(|&i| {
                let facts = Facts {
                    state: state.get(&i).copied().unwrap_or_default(),
                    refused: refused.get(&i).copied(),
                    deletes: deletes.contains(&i),
                    clean: self.sides[i].iter().any(|e| e.clean),
                    undecided: undecided.contains(&i),
                    best: 0.0,
                };
                (i, facts)
            })
            .collect()
    }

    /// The file of a class to keep.
    fn choose(&self, class: &[usize], colour: &(impl Fn(usize) -> f32 + Sync)) -> usize {
        // Not one another file of the class shows more of.
        let mut c: Vec<usize> =
            class.iter().copied().filter(|&f| !class.iter().any(|&g| g != f && self.side(f, g).is_some() && self.more(g, f))).collect();
        if c.is_empty() {
            c = class.to_vec();
        }
        if c.len() == 1 {
            return c[0];
        }
        let grid = |f: usize| &self.pics[f].traits.grid;
        let d_grey = |a: usize, b: usize| grid(a).iter().zip(grid(b)).map(|(x, y)| (x - y).abs()).sum::<f32>();
        let mid: HashMap<usize, f32> =
            c.iter().map(|&f| (f, c.iter().filter(|&&g| g != f).map(|&g| d_grey(f, g)).sum::<f32>() / (c.len() - 1) as f32)).collect();
        let k0 = *c.iter().min_by(|a, b| mid[a].total_cmp(&mid[b])).unwrap();
        let mut t: Vec<usize> = c.iter().copied().filter(|&f| d_grey(f, k0) <= TAU).collect();
        if t.is_empty() {
            t = vec![k0];
        }
        let soft = |f: usize| softness(self.pics[f].traits);
        let mut s: Vec<f32> = t.iter().map(|&f| soft(f)).collect();
        s.sort_by(f32::total_cmp);
        let median = if s.len() % 2 == 1 { s[s.len() / 2] } else { (s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0 };
        let sharp: Vec<usize> = t.iter().copied().filter(|&f| soft(f) <= median + SOFT_TIE).collect();
        if !sharp.is_empty() {
            t = sharp;
        }
        // Pixel-identical files stand as one: the smallest of them.
        let twins = |f: usize| -> Vec<usize> {
            c.iter().copied().filter(|&g| g == f || self.side(f, g).is_some_and(|e| e.exact)).collect()
        };
        let mut reps: Vec<usize> =
            t.iter().map(|&f| twins(f).into_iter().min_by_key(|&g| (self.pics[g].bytes, g)).unwrap()).collect();
        reps.sort_unstable();
        reps.dedup();
        let lossless = |f: usize| twins(f).iter().any(|&g| self.pics[g].traits.lossless);
        // How much of the others' frames a file leaves out, to the nearest
        // two per cent: of files that hold each other only nearly, the one
        // that loses least.
        let lost = |f: usize| -> u32 {
            let s: f32 = reps.iter().filter(|&&g| g != f).filter_map(|&g| self.side(g, f)).map(|e| (e.f_out - TOL).max(0.0)).sum();
            (s / 0.02).round() as u32
        };
        let mut order: Vec<(u32, bool, bool, u64, f32, usize)> = reps
            .iter()
            .map(|&f| (lost(f), lossless(f), self.pics[f].traits.exif, self.pics[f].bytes, mid.get(&f).copied().unwrap_or(0.0), f))
            .collect();
        // Least left out, lossless, EXIF, smaller, nearer the middle; ties
        // to the lower index.
        order.sort_by(|a, b| {
            a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)).then(a.3.cmp(&b.3)).then(a.4.total_cmp(&b.4)).then(a.5.cmp(&b.5))
        });
        // Colour over grey, asked in that order and only as far as needed: a
        // JPEG's colour is not measured by the decode, and is decoded here.
        for &(.., f) in &order {
            let known = twins(f).iter().map(|&g| self.pics[g].traits.colour).find(|&v| v >= 0.0);
            if known.unwrap_or_else(|| colour(f)) > COLOURFUL {
                return f;
            }
        }
        order[0].5
    }
}

/// One strong component as `Ctx::settle` reads it: which of its files are
/// composites, its classes, and the whole-frame checks made so far.
struct Component<'c, 'a> {
    ctx: &'c Ctx<'a>,
    /// Copied at its own frame by some file, weak pairs included.
    plain: HashMap<usize, bool>,
    /// Holds no plain picture whole and more besides.
    part: HashMap<usize, bool>,
    any_plain: bool,
    /// Classes: files of one frame, each holding the other.
    class: HashMap<usize, usize>,
    classes: Vec<Vec<usize>>,
    /// How many different pictures each class holds: its files, each set
    /// of the same bytes or the same pixels counted once.
    distinct: Vec<usize>,
    /// `whole_worst` by (keeper, deleted), and the blurs it read.
    whole: RefCell<HashMap<(usize, usize), f32>>,
    blurs: RefCell<Blurs>,
}

impl<'c, 'a> Component<'c, 'a> {
    fn new(ctx: &'c Ctx<'a>, members: &[usize]) -> Self {
        let frame_any = |f: usize, g: usize| -> bool {
            let s = &ctx.all_sides[f];
            s.binary_search_by_key(&(g as u32), |e| e.to).ok().is_some_and(|k| s[k].exact || s[k].f_in)
        };
        let is_plain = |f: usize| ctx.all_sides[f].iter().any(|e| frame_any(f, e.to as usize) && frame_any(e.to as usize, f));
        let plain: HashMap<usize, bool> = members.iter().map(|&f| (f, is_plain(f))).collect();
        let any_plain = plain.values().any(|&p| p);
        let part: HashMap<usize, bool> = members
            .iter()
            .map(|&f| {
                let holds_plain = ctx.all_sides[f].iter().any(|e| {
                    let g = e.to as usize;
                    frame_any(g, f) && !frame_any(f, g) && is_plain(g)
                });
                (f, !holds_plain)
            })
            .collect();
        let mut class: HashMap<usize, usize> = HashMap::new();
        let mut classes: Vec<Vec<usize>> = Vec::new();
        for &f in members {
            if class.contains_key(&f) {
                continue;
            }
            let c = classes.len();
            class.insert(f, c);
            let mut list = vec![f];
            let mut q = vec![f];
            while let Some(x) = q.pop() {
                for e in &ctx.sides[x] {
                    let y = e.to as usize;
                    if !class.contains_key(&y) && ctx.same(x, y) && ctx.holds(x, y) && ctx.holds(y, x) {
                        class.insert(y, c);
                        list.push(y);
                        q.push(y);
                    }
                }
            }
            list.sort_unstable();
            classes.push(list);
        }
        let distinct = classes
            .iter()
            .map(|c| c.iter().enumerate().filter(|&(i, &f)| !c[..i].iter().any(|&g| ctx.side(f, g).is_some_and(|e| e.exact))).count())
            .collect();
        Component { ctx, plain, part, any_plain, class, classes, distinct, whole: Default::default(), blurs: Default::default() }
    }

    /// The whole-frame check of deleting `d` for `k` (`whole_worst`).
    fn whole(&self, k: usize, d: usize) -> f32 {
        let Some(m) = self.ctx.transform(d, k) else { return 0.0 };
        let (pd, pk) = (&self.ctx.pics[d], &self.ctx.pics[k]);
        *self.whole.borrow_mut().entry((k, d)).or_insert_with(|| whole_worst(&mut self.blurs.borrow_mut(), (d, pd), (k, pk), &m))
    }

    /// Why `k` may not delete `d`, or `None` where it may. Asked of every
    /// pair as deletion spreads, and it decides nothing itself: what a
    /// refusal makes of `d` is `Refusal`'s to say.
    fn refusal(&self, k: usize, d: usize) -> Option<Refusal> {
        let x = self.ctx;
        if !x.holds(k, d) {
            return Some(Refusal::No);
        }
        // A file of several pictures shows more than the first, which is
        // all any match was made on: only a copy of its bytes holds it.
        if x.pics[d].traits.frames && !x.side(d, k).is_some_and(|e| e.exact) {
            return Some(Refusal::Held);
        }
        // In a set with no picture copied at its own frame — a photograph
        // and its crop, say — nothing says which file is a composite, and
        // the larger frame may delete what it holds: unless what it has
        // besides reads as a canvas rather than more of the picture.
        let shape = x.same(k, d) || (x.strict_in(d, k) && self.part[&d] && (self.plain[&k] || !self.any_plain && !x.on_canvas(k, d)));
        if !shape || self.whole(k, d) > WHOLE_RESIDUAL {
            return Some(Refusal::No);
        }
        // Two different pictures of one frame and nothing else of it, and
        // nothing measured says which shows more: nothing says which is the
        // original. Something has when the two differ in size by more than
        // `SHRINK`, where `holds` asked whether the larger's extra pixels
        // hold detail, or when one shows detail the other lacks (`more`). A
        // pinned file is kept whatever it is, so against a library the
        // question is not asked.
        if !x.is_pinned(k)
            && self.class.get(&k).is_some_and(|&c| self.class.get(&d) == Some(&c) && self.distinct[c] < 3)
            && x.side(d, k).is_some_and(|e| !e.exact && e.sx.abs() <= SHRINK && e.sy.abs() <= SHRINK)
            && !x.more(k, d)
            && !x.more(d, k)
        {
            return Some(Refusal::Undecided);
        }
        // A lossless file's pixels are its picture, and a file of its frame
        // and size whose pixels are not the same is another picture or
        // cannot be shown not to be: what tells them apart may be finer than
        // anything the run compares (`own_pixels`). Two such lossless files
        // are a pair nothing decides, as above, and a lossy one does not
        // stand in for a lossless one: against a library, whose lossy copy
        // of a picture answers for it by the user's decision, only a
        // lossless library file is asked.
        if x.own_pixels(k, d) {
            if x.pics[k].traits.lossless {
                return Some(Refusal::Undecided);
            }
            if !x.is_pinned(k) {
                return Some(Refusal::Held);
            }
        }
        // Asked last, so that what it refuses is what it alone refused.
        if x.softer(k, d) {
            return Some(Refusal::Softer);
        }
        None
    }
}

/// How far a frame mapped through `m` lands outside the other, as a fraction
/// of the other's side: zero when every corner lands inside.
fn out_by(m: &Affine, a: (u32, u32), b: (u32, u32)) -> f32 {
    let (aw, ah, bw, bh) = (a.0 as f32, a.1 as f32, b.0.max(1) as f32, b.1.max(1) as f32);
    let mut worst = 0f32;
    for (x, y) in [(0.0, 0.0), (aw, 0.0), (0.0, ah), (aw, ah)] {
        let u = m[0] * x + m[1] * y + m[2];
        let v = m[3] * x + m[4] * y + m[5];
        worst = worst.max(-u / bw).max(u / bw - 1.0).max(-v / bh).max(v / bh - 1.0);
    }
    worst
}

/// The fraction of a frame mapped through `m` that lands outside the other:
/// the mapped frame is a parallelogram, clipped to the other's rectangle.
fn outside(m: &Affine, a: (u32, u32), b: (u32, u32)) -> f32 {
    let (aw, ah, bw, bh) = (a.0 as f32, a.1 as f32, b.0 as f32, b.1 as f32);
    let mut poly: Vec<(f32, f32)> = [(0.0, 0.0), (aw, 0.0), (aw, ah), (0.0, ah)]
        .iter()
        .map(|&(x, y)| (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]))
        .collect();
    let area = |p: &[(f32, f32)]| -> f32 {
        let n = p.len();
        (0..n).map(|i| p[i].0 * p[(i + 1) % n].1 - p[(i + 1) % n].0 * p[i].1).sum::<f32>().abs() / 2.0
    };
    let whole = area(&poly);
    if whole <= 0.0 {
        return 1.0;
    }
    // Sutherland-Hodgman against the four edges: x >= 0, x <= bw, y >= 0, y <= bh.
    for edge in 0..4 {
        let inside = |p: (f32, f32)| match edge {
            0 => p.0 >= 0.0,
            1 => p.0 <= bw,
            2 => p.1 >= 0.0,
            _ => p.1 <= bh,
        };
        let cross = |p: (f32, f32), q: (f32, f32)| -> (f32, f32) {
            let t = match edge {
                0 => (0.0 - p.0) / (q.0 - p.0),
                1 => (bw - p.0) / (q.0 - p.0),
                2 => (0.0 - p.1) / (q.1 - p.1),
                _ => (bh - p.1) / (q.1 - p.1),
            };
            (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
        };
        let src = std::mem::take(&mut poly);
        for i in 0..src.len() {
            let (p, q) = (src[i], src[(i + 1) % src.len()]);
            match (inside(p), inside(q)) {
                (true, true) => poly.push(q),
                (true, false) => poly.push(cross(p, q)),
                (false, true) => {
                    poly.push(cross(p, q));
                    poly.push(q);
                }
                (false, false) => {}
            }
        }
        if poly.is_empty() {
            return 1.0;
        }
    }
    (1.0 - area(&poly) / whole).clamp(0.0, 1.0)
}

fn invert(m: &Affine) -> Option<Affine> {
    let d = m[0] * m[4] - m[1] * m[3];
    if d.abs() < 1e-12 {
        return None;
    }
    let (a, b, c, e) = (m[4] / d, -m[1] / d, -m[3] / d, m[0] / d);
    Some([a, b, -(a * m[2] + b * m[5]), c, e, -(c * m[2] + e * m[5])])
}

/// How much softer a picture is at its own pixel scale: how much its pixels
/// differ more from those two away than from their neighbours, along and
/// down.
fn softness(t: &Traits) -> f32 {
    let d = &t.detail;
    ((d[1].max(1e-3) / d[0].max(1e-3)).log2() + (d[5].max(1e-3) / d[4].max(1e-3)).log2()) / 2.0
}

/// `h`, measured at 1, 2, 4 and 8 pixels, at distance `d`, interpolated in
/// log-log and held to [1, 8].
fn at_distance(h: &[f32], d: f32) -> f32 {
    let t = d.clamp(1.0, 8.0).log2();
    let i = (t as usize).min(2);
    let u = t - i as f32;
    let (a, b) = (h[i].max(1e-4).log2(), h[i + 1].max(1e-4).log2());
    (a + (b - a) * u).exp2()
}

/// In octaves, how much more pixel-scale detail `g` holds than an
/// interpolation of `f` would, where `g` is `s` (>= 1) times `f`'s size.
/// Both are first put on one footing at the scale they share: `g` eight
/// pixels apart against `f` at `8 / s`.
fn extra(f: &Traits, g: &Traits, s: f32) -> f32 {
    let (hf, hg) = (&f.detail[..4], &g.detail[..4]);
    let c = hg[3] / at_distance(hf, 8.0 / s).max(1e-4);
    (hg[0].max(1e-4) * s / (hf[0].max(1e-4) * c)).log2()
}

/// Whether `k` shows what `d` shows over the whole of `d`'s frame, mapped by
/// `m` (`d`'s working image to `k`'s).
///
/// The pixel check compares the blocks of an overlap that carry detail and
/// lets the flat ones abstain, which is right for deciding whether two files
/// are one picture and wrong for deciding that one may go: a caption bar's
/// white strip and a slide's white margin are both flat. Here every sample
/// counts. Both thumbnails are read at the footprint one sample covers in
/// each, a linear tone map is fitted between them (contrast, brightness and
/// each thumbnail's own stretch), and the worst 8x8-sample block's mean
/// residual must stay under `WHOLE_RESIDUAL` grey levels, on a grid of
/// `WHOLE_GRID` samples a side.
fn whole_worst(blurs: &mut Blurs, (d, pd): (usize, &Picture), (k, pk): (usize, &Picture), m: &Affine) -> f32 {
    let (td, wd, tk, wk) = (pd.thumb, pd.work, pk.thumb, pk.work);
    const N: usize = WHOLE_GRID;
    const B: usize = 8;
    if wd.0 == 0 || wk.0 == 0 || td.w == 0 || tk.w == 0 {
        return f32::INFINITY;
    }
    let sd = td.w as f32 / wd.0 as f32;
    let sk = tk.w as f32 / wk.0 as f32;
    let span_d = td.w as f32 / N as f32;
    let span_k = (m[0] * m[4] - m[1] * m[3]).abs().sqrt() * sk * wd.0 as f32 / N as f32;
    let bd = blurs.get(d, td, span_d);
    let bk = blurs.get(k, tk, span_k);
    let mut a = [0f32; N * N];
    let mut b = [0f32; N * N];
    for iy in 0..N {
        for ix in 0..N {
            let xd = (ix as f32 + 0.5) / N as f32 * wd.0 as f32;
            let yd = (iy as f32 + 0.5) / N as f32 * wd.1 as f32;
            let u = (m[0] * xd + m[1] * yd + m[2]) * sk;
            let v = (m[3] * xd + m[4] * yd + m[5]) * sk;
            a[iy * N + ix] = bilinear(&bd, xd * sd, yd * sd);
            b[iy * N + ix] = bilinear(&bk, u, v);
        }
    }
    let n = (N * N) as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut cov, mut var) = (0f32, 0f32);
    for (x, y) in a.iter().zip(&b) {
        cov += (x - ma) * (y - mb);
        var += (x - ma) * (x - ma);
    }
    let g = if var > 1e-6 { cov / var } else { 0.0 };
    let o = mb - g * ma;
    let mut worst = 0f32;
    for by in 0..N / B {
        for bx in 0..N / B {
            let mut r = 0f32;
            for y in by * B..(by + 1) * B {
                for x in bx * B..(bx + 1) * B {
                    let k = y * N + x;
                    r += (b[k] - (g * a[k] + o)).abs();
                }
            }
            worst = worst.max(r / (B * B) as f32);
        }
    }
    worst
}

crate::simd::dispatched! {
    fn blurred(t: &Thumb, sigma: f32) -> Plane => blurred_any;
}

/// A thumbnail blurred by a Gaussian of `sigma` pixels, reflected at the
/// edges and cut at four sigmas, as a float plane. A blur rather than a mip
/// pyramid because a pyramid's 2x2 grid lands differently on the two sides:
/// on a thumbnail thirty rows tall that alone read 13 grey levels of
/// disagreement between a crop and the picture it was cut from.
/// Compiled again for x86-64-v3 (`blurred`), since the run's own build is
/// plain x86-64 and this is the suggestion's one loop worth vectorising.
///
/// A blur of two pixels or more is taken on the thumbnail halved first (a
/// 2x2 mean), at half the width: a quarter of the pixels and half the taps,
/// for a filter whose own width hides the halving's grid.
#[cfg_attr(dispatch, inline(always))]
fn blurred_any(t: &Thumb, sigma: f32) -> Plane {
    let (mut w, mut h) = (t.w as usize, t.h as usize);
    let mut px: Vec<f32> = t.px.iter().map(|&v| v as f32).collect();
    let mut f = 1f32;
    while sigma / f >= 2.0 && w >= 8 && h >= 8 {
        let (nw, nh) = (w / 2, h / 2);
        let mut next = vec![0f32; nw * nh];
        for y in 0..nh {
            let (r0, r1) = (&px[2 * y * w..(2 * y + 1) * w], &px[(2 * y + 1) * w..(2 * y + 2) * w]);
            for (x, o) in next[y * nw..(y + 1) * nw].iter_mut().enumerate() {
                *o = (r0[2 * x] + r0[2 * x + 1] + r1[2 * x] + r1[2 * x + 1]) * 0.25;
            }
        }
        (w, h, px, f) = (nw, nh, next, f * 2.0);
    }
    let sigma = sigma / f;
    if sigma < 0.6 {
        return Plane { w, h, f, px };
    }
    let r = (4.0 * sigma + 0.5) as isize;
    let mut k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= sum);
    // Half-sample reflection: d c b a | a b c d.
    let reflect = |i: isize, n: usize| -> usize {
        let n = n as isize;
        let mut i = i;
        loop {
            if i < 0 {
                i = -i - 1;
            } else if i >= n {
                i = 2 * n - i - 1;
            } else {
                return i as usize;
            }
        }
    };
    // Each row padded by its reflection once, then every tap added across
    // the whole row: the inner loop has no edge to think about.
    let mut tmp = vec![0f32; w * h];
    let mut pad = vec![0f32; w + 2 * r as usize];
    for y in 0..h {
        let row = &px[y * w..(y + 1) * w];
        for (j, p) in pad.iter_mut().enumerate() {
            *p = row[reflect(j as isize - r, w)];
        }
        let dst = &mut tmp[y * w..(y + 1) * w];
        for (j, &kv) in k.iter().enumerate() {
            for (d, s) in dst.iter_mut().zip(&pad[j..j + w]) {
                *d += kv * s;
            }
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for (j, &kv) in k.iter().enumerate() {
            let sy = reflect(y as isize + j as isize - r, h);
            let (src, dst) = (&tmp[sy * w..(sy + 1) * w], &mut out[y * w..(y + 1) * w]);
            for (d, s) in dst.iter_mut().zip(src) {
                *d += kv * s;
            }
        }
    }
    Plane { w, h, f, px: out }
}

/// A blurred thumbnail, each of whose pixels is `f` of the thumbnail's.
struct Plane {
    w: usize,
    h: usize,
    f: f32,
    px: Vec<f32>,
}

/// A blurred plane read bilinearly at thumbnail position (`x`, `y`), pixel
/// centres at half-integers, clamped at the edges.
fn bilinear(p: &Plane, x: f32, y: f32) -> f32 {
    let (w, h, px) = (p.w, p.h, &p.px);
    let fx = (x / p.f - 0.5).clamp(0.0, (w - 1) as f32);
    let fy = (y / p.f - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx as usize, fy as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (u, v) = (fx - x0 as f32, fy - y0 as f32);
    let top = px[y0 * w + x0] * (1.0 - u) + px[y0 * w + x1] * u;
    let bot = px[y1 * w + x0] * (1.0 - u) + px[y1 * w + x1] * u;
    top * (1.0 - v) + bot * v
}

/// Blurred thumbnails already made in one component, by file and blur.
#[derive(Default)]
struct Blurs(HashMap<(usize, i32), std::rc::Rc<Plane>>);

impl Blurs {
    /// File `i`'s thumbnail blurred to a footprint of `span` pixels: a
    /// Gaussian of half that, in steps of a quarter octave, so that the
    /// footprints a file is read at in its pairs share a few blurs.
    fn get(&mut self, i: usize, t: &Thumb, span: f32) -> std::rc::Rc<Plane> {
        let step = (4.0 * (0.5 * span).max(0.25).log2()).round() as i32;
        let sigma = (step as f32 / 4.0).exp2();
        self.0.entry((i, step)).or_insert_with(|| blurred(t, sigma).into()).clone()
    }
}

/// `a.hypot(b)` without asking libm for it. `f32::hypot` is glibc's
/// `hypotf`, which glibc 2.35 versioned anew, so a binary built on a newer
/// system asked for glibc 2.35 and the release check refused it: 0.34.0 and
/// 0.35.0 were never published. Squared and summed in f64, which holds both
/// squares exactly, and rounded once: the correctly rounded f32 result.
fn hypot(a: f32, b: f32) -> f32 {
    let (a, b) = (a as f64, b as f64);
    (a * a + b * b).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deletion is taken back when its kept copy has gone or changed, or it
    /// has changed itself; only the first is said to be for its kept copy,
    /// and a file that is not a deletion is left as it is.
    #[test]
    fn a_deletion_is_taken_back_when_what_it_rests_on_lapses() {
        use Action::*;
        // 0 kept and gone; 1 rests on it; 2 rests on 5, changed; 3 changed,
        // resting on 0 too; 4 changed, resting on 6, which is fine; 5 a KEEP
        // that changed; 6 kept.
        let mut actions = [Some(Keep), Some(Delete), Some(Delete), Some(Delete), Some(Delete), Some(Keep), Some(Keep)];
        let mut kept = [None, Some(0), Some(5), Some(0), Some(6), None, None];
        let lapsed = |i: usize| match i {
            0 => Some(Lapse::Gone),
            3..=5 => Some(Lapse::Changed),
            _ => None,
        };
        let taken = revoke(&mut actions, &mut kept, lapsed);
        assert_eq!(taken, [(1, Lapse::Gone), (2, Lapse::Changed), (3, Lapse::Gone)]);
        assert_eq!(actions, [Some(Keep), Some(Review), Some(Review), Some(Review), Some(Review), Some(Keep), Some(Keep)]);
        assert_eq!(kept, [None; 7]);
        // The group rules name no kept copy: only a changed deletion goes.
        let mut actions = [Some(Delete), Some(Delete), Some(Keep)];
        assert!(revoke(&mut actions, &mut [], |i| (i == 1).then_some(Lapse::Changed)).is_empty());
        assert_eq!(actions, [Some(Delete), Some(Review), Some(Keep)]);
    }

    /// A deletion names the file at the end of its chain of holders, which
    /// stays; a chain that ends on no such file, or loops, names none.
    #[test]
    fn a_deletion_names_the_file_that_stays() {
        use Action::*;
        let actions = [Some(Keep), Some(Delete), Some(Delete), Some(Delete), Some(Delete), Some(Review)];
        let keeper = [None, Some(0), Some(1), Some(4), Some(3), None];
        assert_eq!(kept_at_end(&actions, &keeper), vec![None, Some(0), Some(0), None, None, None]);
    }

    fn thumb() -> Thumb {
        Thumb::new(16, 12, 0.125, (0..16 * 12).map(|i| ((i * 37) % 251) as u8).collect())
    }

    fn colourful() -> Traits {
        Traits { detail: [8.0, 12.0, 16.0, 20.0, 8.0, 12.0], colour: 30.0, ..Default::default() }
    }

    fn verdict(blk: f32, blk_min: f32) -> Verdict {
        Verdict { blk, blk_min, blk_n: 36, ov_a: 1.0, ov_b: 1.0, scale: 1.0, ..Default::default() }
    }

    const IDENTITY: Affine = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];

    #[test]
    fn of_two_identical_files_one_is_kept_and_the_other_deleted() {
        let (t, tr) = (thumb(), colourful());
        let pic = |bytes| Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits: &tr, bytes };
        let pics = [pic(2_000), pic(1_000)];
        let got = suggest(&pics, &[(0, 1, Link::Exact)], 0.6, |_| 30.0);
        // The same pixels: the smaller file is kept.
        assert_eq!(got, [Some(Action::Delete), Some(Action::Keep)]);
    }

    /// An original among several softer copies of its size is not deleted
    /// for one of them, however many there are: the majority picks the
    /// keeper, and none of them may stand in for the sharper file.
    #[test]
    fn a_softer_copy_does_not_delete_a_sharper_one_of_its_size() {
        let t = thumb();
        let sharp = Traits { detail: [10.0, 12.5, 16.0, 20.0, 10.0, 12.5], colour: 30.0, ..Default::default() };
        let soft = Traits { detail: [5.0, 10.0, 15.0, 19.0, 5.0, 10.0], colour: 30.0, ..Default::default() };
        let pic = |traits, bytes| Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits, bytes };
        // The original, then three softer copies, each the smaller file.
        let pics = [pic(&sharp, 5_000), pic(&soft, 1_000), pic(&soft, 1_100), pic(&soft, 1_200)];
        let v = verdict(1.0, 1.0);
        let links: Vec<(usize, usize, Link)> =
            (0..4).flat_map(|a| (a + 1..4).map(move |b| (a, b))).map(|(a, b)| (a, b, Link::Edge(&v, IDENTITY, false))).collect();
        let (got, keeper) = suggest_pinned(&pics, &links, 0.6, |_| 30.0, &[]);
        assert_eq!(got[0], Some(Action::Keep), "{got:?}");
        assert!((1..4).all(|i| keeper[i] != Some(1) || got[1] == Some(Action::Keep)));
        // Alike in softness, the smallest is kept as before.
        let pics = [pic(&soft, 5_000), pic(&soft, 1_000), pic(&soft, 1_100), pic(&soft, 1_200)];
        let got = suggest(&pics, &links, 0.6, |_| 30.0);
        assert_eq!(got, [Some(Action::Delete), Some(Action::Keep), Some(Action::Delete), Some(Action::Delete)]);
    }

    /// Every pair of `n` files, matched through the identity at `v`.
    fn all_pairs(n: usize, v: &Verdict) -> Vec<(usize, usize, Link<'_>)> {
        (0..n).flat_map(|a| (a + 1..n).map(move |b| (a, b))).map(|(a, b)| (a, b, Link::Edge(v, IDENTITY, false))).collect()
    }

    /// A file of several pictures is deleted only for a copy of its bytes:
    /// two animations sharing their first frame, or an animation and a still
    /// of that frame, are each kept.
    #[test]
    fn a_file_of_several_pictures_is_deleted_only_for_its_bytes() {
        let t = thumb();
        let still = colourful();
        let anim = Traits { frames: true, ..colourful() };
        let pic = |traits, bytes| Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits, bytes };
        let v = verdict(1.0, 1.0);
        let pics = [pic(&anim, 9_000), pic(&anim, 8_000), pic(&still, 1_000), pic(&still, 1_100)];
        let got = suggest(&pics, &all_pairs(4, &v), 0.6, |_| 30.0);
        assert!(got[0] == Some(Action::Keep) && got[1] == Some(Action::Keep), "{got:?}");
        // Its bytes again may go.
        let got = suggest(&pics[..2], &[(0, 1, Link::Exact)], 0.6, |_| 30.0);
        assert_eq!(got.iter().filter(|a| **a == Some(Action::Delete)).count(), 1, "{got:?}");
    }

    /// Two lossless files of one frame and size whose pixels differ are two
    /// pictures, whatever a majority says: of a lossless original, its lossy
    /// re-encode and another lossless picture that every measure takes for
    /// it, neither lossless file is deleted. Nor is a lossless file deleted
    /// for a lossy one of its size.
    #[test]
    fn a_lossless_file_is_deleted_only_for_its_own_pixels() {
        let t = thumb();
        let png = Traits { lossless: true, ..colourful() };
        let jpg = colourful();
        let pic = |traits, bytes| Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits, bytes };
        let v = verdict(1.0, 1.0);
        let pics = [pic(&png, 5_000), pic(&jpg, 2_000), pic(&png, 5_100)];
        let got = suggest(&pics, &all_pairs(3, &v), 0.6, |_| 30.0);
        assert!(got[0] != Some(Action::Delete) && got[2] != Some(Action::Delete), "{got:?}");
        assert_eq!(got[1], Some(Action::Delete), "the re-encode still goes: {got:?}");
        // A lossless file and two lossy copies, which outvote it.
        let pics = [pic(&png, 5_000), pic(&jpg, 2_000), pic(&jpg, 1_500)];
        let got = suggest(&pics, &all_pairs(3, &v), 0.6, |_| 30.0);
        assert_eq!(got[0], Some(Action::Keep), "{got:?}");
        // The same pixels are one picture, and one of them goes.
        let got = suggest(&pics[..1].iter().chain(&pics[..1]).copied().collect::<Vec<_>>(), &[(0, 1, Link::Exact)], 0.6, |_| 30.0);
        assert_eq!(got.iter().filter(|a| **a == Some(Action::Delete)).count(), 1, "{got:?}");
    }

    #[test]
    fn a_pinned_file_is_kept_and_named_by_what_it_deletes() {
        let (t, tr) = (thumb(), colourful());
        let pic = |bytes| Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits: &tr, bytes };
        // Unpinned, the smaller file 1 would be kept; pinned, the larger is.
        let pics = [pic(2_000), pic(1_000)];
        let (got, keeper) = suggest_pinned(&pics, &[(0, 1, Link::Exact)], 0.6, |_| 30.0, &[true, false]);
        assert_eq!(got, [Some(Action::Keep), Some(Action::Delete)]);
        assert_eq!(keeper, [None, Some(0)]);
        // Two pinned files are both kept, whatever they hold of each other.
        let (got, _) = suggest_pinned(&pics, &[(0, 1, Link::Exact)], 0.6, |_| 30.0, &[true, true]);
        assert_eq!(got, [Some(Action::Keep), Some(Action::Keep)]);
    }

    #[test]
    fn a_weak_pair_settles_nothing_unless_it_agrees_almost_everywhere() {
        let (t, tr) = (thumb(), colourful());
        let pic = Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits: &tr, bytes: 1_000 };
        let pics = [pic, Picture { bytes: 2_000, ..pic }, Picture { bytes: 3_000, ..pic }];
        // One block disagrees at a correlation under `WEAK_DELETES`, the
        // correlation is under the run's bar, and one is the other's negative.
        // Only the first agrees closely enough to be a copy worth a look.
        let (a, b, c) = (verdict(0.93, 0.5), verdict(0.55, 0.9), verdict(0.95, 0.95));
        for (v, inverted, want) in [(&a, false, Action::Review), (&b, false, Action::Keep), (&c, true, Action::Keep)] {
            let got = suggest(&pics[..2], &[(0, 1, Link::Edge(v, IDENTITY, inverted))], 0.6, |_| 30.0);
            assert_eq!(got, [Some(want), Some(want)]);
        }
        // Strong, or weak and agreeing almost everywhere, it settles them,
        // beside a third copy that says which is the picture.
        let strong = verdict(1.0, 1.0);
        for v in [&c, &verdict(0.97, 0.5)] {
            let links = [(0, 1, Link::Edge(v, IDENTITY, false)), (0, 2, Link::Edge(&strong, IDENTITY, false)), (1, 2, Link::Edge(&strong, IDENTITY, false))];
            let got = suggest(&pics, &links, 0.6, |_| 30.0);
            assert!(got.contains(&Some(Action::Keep)) && got.contains(&Some(Action::Delete)), "{got:?}");
        }
        assert_eq!(suggest(&pics, &[], 0.6, |_| 30.0), [None, None, None], "a file in no pair has no action");
    }

    #[test]
    fn a_mirror_image_settles_nothing_and_is_left_for_a_look() {
        let (t, tr) = (thumb(), colourful());
        let pic = Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits: &tr, bytes: 1_000 };
        // The mirror is the larger file, which would win were it upright.
        let pics = [pic, Picture { bytes: 2_000, ..pic }];
        let v = verdict(0.99, 0.98);
        let flip: Affine = [-1.0, 0.0, 127.0, 0.0, 1.0, 0.0];
        assert_eq!(suggest(&pics, &[(0, 1, Link::Edge(&v, flip, false))], 0.6, |_| 30.0), [Some(Action::Review); 2]);
        // Beside upright copies, they settle and the mirror waits.
        let pics = [pic, Picture { bytes: 2_000, ..pic }, Picture { bytes: 3_000, ..pic }, Picture { bytes: 4_000, ..pic }];
        let links = [
            (0, 1, Link::Edge(&v, IDENTITY, false)),
            (0, 3, Link::Edge(&v, IDENTITY, false)),
            (1, 3, Link::Edge(&v, IDENTITY, false)),
            (1, 2, Link::Edge(&v, flip, false)),
            (0, 2, Link::Edge(&v, flip, false)),
        ];
        let got = suggest(&pics, &links, 0.6, |_| 30.0);
        assert_eq!(got[2], Some(Action::Review), "{got:?}");
        assert!(got.contains(&Some(Action::Keep)) && got.contains(&Some(Action::Delete)), "{got:?}");
    }

    /// Two different pictures of one frame and nothing else of it: neither
    /// is deleted, and both are left for a look, however closely they agree.
    /// A third settles it; the same pixels need none; a library file is kept
    /// whatever it is, and settles it as before.
    #[test]
    fn two_pictures_of_one_frame_alone_are_both_left_for_a_look() {
        let (t, tr) = (thumb(), colourful());
        let pic = Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits: &tr, bytes: 1_000 };
        let pics = [pic, Picture { bytes: 2_000, ..pic }, Picture { bytes: 3_000, ..pic }];
        let (v, low) = (verdict(1.0, 1.0), verdict(0.8, 0.9));
        for v in [&v, &low] {
            let got = suggest(&pics[..2], &[(0, 1, Link::Edge(v, IDENTITY, false))], 0.6, |_| 30.0);
            assert_eq!(got, [Some(Action::Review); 2], "{}", v.blk);
        }
        let links = [(0, 1, Link::Edge(&v, IDENTITY, false)), (0, 2, Link::Edge(&v, IDENTITY, false)), (1, 2, Link::Edge(&v, IDENTITY, false))];
        let got = suggest(&pics, &links, 0.6, |_| 30.0);
        assert_eq!(got.iter().filter(|a| **a == Some(Action::Delete)).count(), 2, "{got:?}");
        assert!(suggest(&pics[..2], &[(0, 1, Link::Exact)], 0.6, |_| 30.0).contains(&Some(Action::Delete)));
        let (got, keeper) = suggest_pinned(&pics[..2], &[(0, 1, Link::Edge(&v, IDENTITY, false))], 0.6, |_| 30.0, &[true, false]);
        assert_eq!((got, keeper), (vec![Some(Action::Keep), Some(Action::Delete)], vec![None, Some(0)]));
    }

    /// A picture and a larger frame holding it with nothing else of either:
    /// the larger deletes it when what it has besides is more of the picture
    /// (an uncropped original), and not when it is a flat canvas (a caption
    /// bar, a border), which its detail, spread over more frame, says; then
    /// both are kept, as a composite and its photograph are.
    #[test]
    fn a_larger_frame_deletes_what_it_holds_unless_the_rest_is_canvas() {
        // A smooth picture, so that its edge reads alike in both thumbnails.
        let t = Thumb::new(16, 12, 0.125, (0..12).flat_map(|y| (0..16).map(move |x| (x * 12 + y * 7) as u8)).collect());
        // The small picture is the larger frame's left half, thumbnail and all.
        let half = Thumb::new(8, 12, 0.125, (0..12).flat_map(|y| t.px[y * 16..y * 16 + 8].to_vec()).collect());
        let photo = Traits { detail: [8.0, 12.0, 16.0, 20.0, 8.0, 12.0], colour: 30.0, ..Default::default() };
        // Half the larger frame is the photograph: on a flat canvas its
        // detail reads half; as more of the photograph, the same.
        let canvas = Traits { detail: [4.0, 6.0, 8.0, 10.0, 4.0, 6.0], colour: 30.0, ..Default::default() };
        let small = Picture { dims: (64, 96), work: (64, 96), thumb: &half, traits: &photo, bytes: 1_000 };
        let v = verdict(1.0, 1.0);
        // Both kept: a composite and its photograph are two things to keep.
        for (traits, want) in [(&photo, Action::Delete), (&canvas, Action::Keep)] {
            let large = Picture { dims: (128, 96), work: (128, 96), thumb: &t, traits, bytes: 2_000 };
            let got = suggest(&[small, large], &[(0, 1, Link::Edge(&v, IDENTITY, false))], 0.6, |_| 30.0);
            assert_eq!(got[0], Some(want), "{got:?}");
        }
    }

    #[test]
    fn by_group_keeps_representatives_and_cuts_on_correlation() {
        let seat = |file, representative, correlation| Seat { file, representative, correlation };
        // Two groups: 0 heads one, 3 the other, and 1 is in both.
        let seats = || [seat(0, true, 1.0), seat(1, false, 0.7), seat(2, false, 0.95), seat(3, true, 1.0), seat(1, false, 0.92), seat(4, false, 0.5)];
        let (k, d, r) = (Some(Action::Keep), Some(Action::Delete), Some(Action::Review));
        // The cut at 0.6 is 0.9: file 1 is judged by its better match.
        assert_eq!(by_group(Mode::Correlation, 6, seats(), 0.6), [k, d, d, k, r, None]);
        assert_eq!(by_group(Mode::Representative, 6, seats(), 0.6), [k, d, d, k, d, None]);
        // A representative of one group is kept though it matched another.
        assert_eq!(by_group(Mode::Correlation, 2, [seat(0, true, 1.0), seat(1, false, 0.99), seat(1, true, 1.0), seat(0, false, 0.99)], 0.6), [k, k]);
    }

    #[test]
    #[ignore]
    fn blur_timings() {
        let px: Vec<u8> = (0..128 * 96).map(|i| ((i * 37) % 251) as u8).collect();
        let t = Thumb::new(128, 96, 0.25, px);
        for sigma in [0.8f32, 1.3, 2.0, 2.8] {
            let n = 2000;
            let t0 = std::time::Instant::now();
            let mut s = 0f32;
            for _ in 0..n {
                s += blurred(&t, sigma).px[5];
            }
            println!("sigma {sigma}: {:.1} us ({s})", t0.elapsed().as_secs_f64() * 1e6 / n as f64);
        }
    }
}
