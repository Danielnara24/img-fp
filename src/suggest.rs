//! Which file of each set of matches to keep, which to delete, and which to
//! look at first: the `action` every report states beside a file.
//!
//! **The rule it answers to is that a deletion loses no picture.** A file is
//! DELETE only when a file that stays shows all of it, or all but a thin
//! strip, at its detail or nearly, and the two were matched well enough to
//! say so. Everything the run cannot prove redundant stays: as REVIEW when it
//! was only ever matched weakly but closely enough to be a copy, as KEEP
//! otherwise. Measured against the benchmark corpora's generated ground
//! truth, three of 17,769 deletions lose anything, each a composite on a
//! synthetic canvas; the measurement and the cases behind each piece are in
//! CLAUDE.md.
//!
//! The pieces, in the order they are applied:
//!
//! - **A pair is clean** when no block of its overlap disagrees
//!   (`Policy::clean`'s bar). A pair that is not may still delete when it
//!   agrees at `WEAK_DELETES` overall; below that it decides nothing, and a
//!   negative never does. A file left reached only through such pairs is
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
//!   and a crop of it — nothing says which is a composite, and the larger
//!   frame may delete what it holds.
//! - **Of files that hold each other, one is kept**: the one closest to the
//!   rest in grey (`TAU`), not softer than the rest, then the one leaving out
//!   least of the others, colour over grey, lossless over lossy, EXIF, and the
//!   smaller file. Pixel-identical files stand as one, the smallest.
//! - **Deletion spreads from the keeps**, each file deleted by one already
//!   settled.

use rayon::prelude::*;
use std::collections::HashMap;

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

/// The clean-anchor bar a strong pair's worst block must clear.
const CLEAN: f32 = 0.85;
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
const REVIEW: f32 = 0.9;
/// The correlation at which a pair that is not clean — one block of it
/// disagrees — may still delete. Re-encodes carrying a watermark, or a
/// background recoloured, fail the clean bar and agree at 0.96 overall; two
/// different photographs of one subject reached 0.963 in the found corpus,
/// and are stopped by the whole-frame check rather than by this.
const WEAK_DELETES: f32 = 0.95;
/// How much more fine detail (log2) a file's extra pixels must hold, per
/// octave of size and plus a floor, to count as real rather than an
/// enlargement's.
const EXTRA_PER_OCTAVE: f32 = 0.1;
const EXTRA_FLOOR: f32 = 0.03;
/// How close in grey (summed over the 4x4 grid, 0..=255 a cell) a file must
/// be to the group's middle to be a candidate for keeping.
const TAU: f32 = 2.0;
/// How much softer than the candidates' median a candidate may be.
const SOFT_TIE: f32 = 0.05;
/// The most a block of the whole-frame check may disagree, in grey levels.
const WHOLE_RESIDUAL: f32 = 12.0;
/// How many files' whole-frame checks a file may fail before it is no
/// longer asked: see `settle`.
const WHOLE_TRIES: u32 = 3;
/// Below this, `Traits::colour` is a grey picture.
const COLOURFUL: f32 = 2.0;

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
pub fn suggest(pics: &[Picture], links: &[(usize, usize, Link)], min_correlation: f32, colour: impl Fn(usize) -> f32 + Sync) -> Vec<Option<Action>> {
    let n = pics.len();
    let mut action = vec![None; n];
    let mut sides: Vec<Vec<Side>> = vec![Vec::new(); n];
    // Every pair, weak ones too, for what the frames say about which file is
    // a composite: a composite usually matches its photograph weakly, since
    // the photograph is small inside it.
    let mut all_sides: Vec<Vec<Side>> = vec![Vec::new(); n];
    for (li, (a, b, link)) in links.iter().enumerate() {
        action[*a] = Some(Action::Review);
        action[*b] = Some(Action::Review);
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
                let sx = ((m[0].hypot(m[3])) * k).max(1e-6).log2();
                let sy = ((m[1].hypot(m[4])) * k).max(1e-6).log2();
                let mi = invert(m);
                let a_out = outside(m, pa.work, pb.work);
                let b_out = mi.map_or(1.0, |mi| outside(&mi, pb.work, pa.work));
                let a_in_b = out_by(m, pa.work, pb.work) <= TOL + TOL_PX / pa.dims.0.min(pa.dims.1).max(1) as f32;
                let b_in_a = mi.is_some_and(|mi| out_by(&mi, pb.work, pa.work) <= TOL + TOL_PX / pb.dims.0.min(pb.dims.1).max(1) as f32);
                // Two crops of one picture, each missing a strip the other has:
                // one frame. A composite holds its picture whole, so it is
                // never this; nor is a pair too weak to delete on, or two
                // specimens framed alike would make every file "plain".
                let deletes = v.blk >= min_correlation && (clean || v.blk >= WEAK_DELETES);
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
            Link::Edge(v, ..) => {
                v.blk >= min_correlation && (fwd.clean || v.blk >= WEAK_DELETES)
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
    let ctx = Ctx { pics, links, sides: &sides, all_sides: &all_sides };
    let settled: Vec<Vec<(usize, Action)>> = components.par_iter().map(|m| ctx.settle(m, &colour)).collect();
    for (i, a) in settled.into_iter().flatten() {
        action[i] = Some(a);
    }
    // A file nothing could delete whose best match agrees less than
    // `REVIEW` is another photograph of something like it, not a copy:
    // there is nothing to look at, and it stays.
    let mut best = vec![0f32; n];
    for (a, b, link) in links {
        if let Link::Edge(v, _, false) = link {
            best[*a] = best[*a].max(v.blk);
            best[*b] = best[*b].max(v.blk);
        }
    }
    for i in 0..n {
        if action[i] == Some(Action::Review) && best[i] < REVIEW {
            action[i] = Some(Action::Keep);
        }
    }
    action
}

struct Ctx<'a> {
    pics: &'a [Picture<'a>],
    links: &'a [(usize, usize, Link<'a>)],
    sides: &'a [Vec<Side>],
    all_sides: &'a [Vec<Side>],
}

impl Ctx<'_> {
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

    fn settle(&self, members: &[usize], colour: &(impl Fn(usize) -> f32 + Sync)) -> Vec<(usize, Action)> {
        let mut action: HashMap<usize, Action> = HashMap::new();
        let frame_any = |f: usize, g: usize| -> bool {
            let s = &self.all_sides[f];
            s.binary_search_by_key(&(g as u32), |e| e.to).ok().is_some_and(|k| s[k].exact || s[k].f_in)
        };
        let is_plain = |f: usize| self.all_sides[f].iter().any(|e| frame_any(f, e.to as usize) && frame_any(e.to as usize, f));
        let plain: HashMap<usize, bool> = members.iter().map(|&f| (f, is_plain(f))).collect();
        let any_plain = plain.values().any(|&p| p);
        // A part: holds no plain picture whole and more besides.
        let part: HashMap<usize, bool> = members
            .iter()
            .map(|&f| {
                let holds_plain = self.all_sides[f].iter().any(|e| {
                    let g = e.to as usize;
                    frame_any(g, f) && !frame_any(f, g) && is_plain(g)
                });
                (f, !holds_plain)
            })
            .collect();
        let mut whole_memo: HashMap<(usize, usize), f32> = HashMap::new();
        let mut blurs = Blurs::default();
        let mut whole = |k: usize, d: usize| -> f32 {
            let Some(m) = self.transform(d, k) else { return 0.0 };
            let (pd, pk) = (&self.pics[d], &self.pics[k]);
            *whole_memo.entry((k, d)).or_insert_with(|| whole_worst(&mut blurs, (d, pd), (k, pk), &m))
        };
        let mut refused: HashMap<usize, u32> = HashMap::new();
        let mut can_delete = |k: usize, d: usize| -> bool {
            if !self.holds(k, d) {
                return false;
            }
            // In a set with no picture copied at its own frame — a photograph
            // and its crop, say — nothing says which file is a composite, and
            // the larger frame may delete what it holds.
            let lone = !any_plain;
            let shape = self.same(k, d) || (self.strict_in(d, k) && part[&d] && (plain[&k] || lone));
            if !shape {
                return false;
            }
            // A file the whole-frame check has refused this often holds
            // something none of its family does — a sticker, a vignette —
            // and asking the rest of the family again only costs time.
            let tries = refused.entry(d).or_insert(0);
            if *tries >= WHOLE_TRIES {
                return false;
            }
            let ok = whole(k, d) <= WHOLE_RESIDUAL;
            if !ok {
                *tries += 1;
            }
            ok
        };
        // Classes: files of one frame, each holding the other.
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
                for e in &self.sides[x] {
                    let y = e.to as usize;
                    if !class.contains_key(&y) && self.same(x, y) && self.holds(x, y) && self.holds(y, x) {
                        class.insert(y, c);
                        list.push(y);
                        q.push(y);
                    }
                }
            }
            list.sort_unstable();
            classes.push(list);
        }
        let mut deleters: std::collections::HashSet<usize> = Default::default();
        let mut keeps = Vec::new();
        let mut chosen = Vec::with_capacity(classes.len());
        for (ci, c) in classes.iter().enumerate() {
            let k = self.choose(c, colour);
            chosen.push(k);
            let beaten = self.sides[k].iter().any(|e| {
                let g = e.to as usize;
                class[&g] != ci && can_delete(g, k)
            });
            if !beaten {
                action.insert(k, Action::Keep);
                keeps.push(k);
            }
        }
        let mut q: std::collections::VecDeque<usize> = keeps.into_iter().collect();
        loop {
            while let Some(x) = q.pop_front() {
                for e in &self.sides[x] {
                    let y = e.to as usize;
                    if !action.contains_key(&y) && can_delete(x, y) {
                        action.insert(y, Action::Delete);
                        deleters.insert(x);
                        q.push_back(y);
                    }
                }
            }
            // A class's choice that a file of another class could delete, and
            // that no settled file did: what could have deleted it was never
            // settled itself. It is kept, and deletion goes on from it.
            q.extend(chosen.iter().copied().filter(|k| !action.contains_key(k)));
            if q.is_empty() {
                break;
            }
            for &k in &q {
                action.insert(k, Action::Keep);
            }
        }
        // A file that stays and was only ever matched weakly is for a person
        // to look at, whatever it was chosen as — unless a deletion rests on
        // it, which makes it the copy to keep.
        members
            .iter()
            .map(|&i| {
                let a = action.get(&i).copied().unwrap_or(Action::Review);
                let weak_only = !deleters.contains(&i) && !self.sides[i].iter().any(|e| e.clean);
                (i, if a == Action::Keep && weak_only { Action::Review } else { a })
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
/// residual must stay under `WHOLE_RESIDUAL` grey levels.
fn whole_worst(blurs: &mut Blurs, (d, pd): (usize, &Picture), (k, pk): (usize, &Picture), m: &Affine) -> f32 {
    let (td, wd, tk, wk) = (pd.thumb, pd.work, pk.thumb, pk.work);
    const N: usize = 32;
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // Strong, or weak and agreeing almost everywhere, it settles them.
        for v in [&c, &verdict(0.97, 0.5)] {
            let got = suggest(&pics[..2], &[(0, 1, Link::Edge(v, IDENTITY, false))], 0.6, |_| 30.0);
            assert!(got.contains(&Some(Action::Keep)) && got.contains(&Some(Action::Delete)), "{got:?}");
        }
        assert_eq!(suggest(&pics, &[], 0.6, |_| 30.0), [None, None, None], "a file in no pair has no action");
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
