//! The pairs a run states, and what each one says.
//!
//! A pair is stated once, from what it rests on (`Stated`: the same bytes,
//! the same pixels, or a claim of the graph), and what it says is worked out
//! then, once, into a `Pair`: its evidence, its scale, where one picture lies
//! in the other, and whether it is clean. The groups, `--against` and the
//! JSON's pairs all read that one record.
//!
//! It used to be stated again for each reader: the JSON's records (two
//! names, two byte strings each) were also where a group member's evidence
//! was looked up, by a map filled from them in name order; the place was a
//! second map that had to copy that order by comparing names, so as to agree
//! with it; and `--against` took the first pair it met. Three answers to one
//! question, which pair states two files, held apart only because no pair
//! was ever stated both ways round. Now a pair is stated once whichever way
//! round it is met (`unique`), and every reader asks it.

use std::collections::HashSet;

use crate::analysis::Item;
use crate::claims::{Claim, How};
use crate::outcome::{Evidence, Place, Relation};
use crate::suggest;
use crate::verify::{self, Verdict};

/// What a pair in the report is stated from: what the keep rule reads, and
/// what its `Pair` is worked out from.
#[derive(Clone, Copy)]
pub(crate) enum Stated<'a> {
    /// The same bytes; see `same::exact_groups`.
    Identical,
    /// Not the same bytes, the same picture; see `same::Kind`.
    SamePixels,
    /// A verified pair.
    Claim(&'a Claim),
}

impl<'a> Stated<'a> {
    fn verdict(&self) -> Option<&'a Verdict> {
        match self {
            Stated::Claim(c) => Some(&c.e.v),
            _ => None,
        }
    }

    /// The pair as the keep rule reads it.
    pub(crate) fn link(&self) -> suggest::Link<'a> {
        match self {
            Stated::Claim(c) => suggest::Link::Edge(&c.e.v, c.e.v.m, c.e.inverted()),
            _ => suggest::Link::Exact,
        }
    }

    /// What the pair of `a` and `b`, in that order, says.
    pub(crate) fn pair(&self, a: usize, b: usize, items: &[Item]) -> Pair {
        let v = self.verdict();
        let relation = match self {
            Stated::Identical => Relation::Identical,
            Stated::SamePixels => Relation::SamePixels,
            Stated::Claim(c) => match c.how {
                How::Direct => Relation::Direct,
                How::Propagated => Relation::Propagated,
                How::Corroborated => Relation::Corroborated,
            },
        };
        let ev = Evidence {
            relation,
            // None for a pair no keypoint was matched for: the same file, or
            // a composed transform.
            aligned_points: match self {
                Stated::Claim(c) if c.how != How::Propagated => Some(c.e.v.n_in),
                _ => None,
            },
            frame_overlap: v.map_or(1.0, |v| round3(v.ov_a.max(v.ov_b))),
            pixel_correlation: v.map_or(1.0, |v| round3(v.blk)),
            mirrored: v.is_some_and(|v| verify::mirrored(&v.m)),
            inverted: matches!(self, Stated::Claim(c) if c.e.inverted()),
        };
        Pair {
            a: a as u32,
            b: b as u32,
            ev,
            scale: round3(file_scale(v.map_or(1.0, |v| v.scale), &items[a], &items[b])),
            place: Place::between(v.map(|v| &v.m), &items[a], &items[b]),
            // No block of the overlap disagrees (`suggest::CLEAN`); the same
            // file always is.
            clean: v.is_none_or(|v| v.blk_min >= suggest::CLEAN),
        }
    }
}

/// Each pair of files once, from the first that states it, whichever way
/// round: the order given is the order kept.
pub(crate) fn unique<'a>(pairs: impl IntoIterator<Item = (usize, usize, Stated<'a>)>) -> Vec<(usize, usize, Stated<'a>)> {
    let mut seen = HashSet::new();
    pairs.into_iter().filter(|&(a, b, _)| seen.insert((a.min(b), a.max(b)))).collect()
}

/// What a stated pair says, `a` to `b`, by the run's file numbers.
#[derive(Clone, Copy, Debug)]
pub struct Pair {
    pub a: u32,
    pub b: u32,
    pub ev: Evidence,
    /// How many pixels of `b` one pixel of `a` spans, in the files' own
    /// pixels: 0.5 when `b` is `a` at half the size. See `file_scale`.
    pub scale: f32,
    /// Where `a` lies in `b`, for the window to draw.
    pub place: Option<Place>,
    /// No block of the overlap disagrees (`suggest::CLEAN`).
    pub clean: bool,
}

impl Pair {
    pub fn ends(&self) -> (usize, usize) {
        (self.a as usize, self.b as usize)
    }

    /// Its two files, lower first: the pair whichever way round it is read.
    pub fn key(&self) -> (usize, usize) {
        let (a, b) = self.ends();
        (a.min(b), a.max(b))
    }

    /// The file across the pair from `f`, one of its ends.
    pub fn other(&self, f: usize) -> usize {
        let (a, b) = self.ends();
        if a == f {
            b
        } else {
            a
        }
    }

    /// Where `f`, one of its ends, lies in the other.
    pub fn place_from(&self, f: usize) -> Option<Place> {
        if f == self.a as usize {
            self.place
        } else {
            self.place?.reversed()
        }
    }
}

/// A verdict's scale, which is the analysis images', as the files' own.
///
/// A transform is fitted between working images, and every picture is
/// analysed at about `--work-size` on its long side however large the file
/// is: a photograph and a copy shrunk to a third of it are both 512 pixels
/// there, and a crop of half the photograph is too. So the verdict's scale
/// said 0.99 for the copy a third the size and 0.5 for the crop that has the
/// photograph's own pixels. Measured in each file's pixels instead — each
/// working image's long side over its file's — it is what a person reading
/// "scale" expects: how many pixels of `b` one pixel of `a` spans.
fn file_scale(scale: f32, a: &Item, b: &Item) -> f32 {
    let shrink = |it: &Item| {
        let file = it.dims.0.max(it.dims.1);
        (file > 0).then(|| it.feats.w.max(it.feats.h) as f32 / file as f32)
    };
    match (shrink(a), shrink(b)) {
        (Some(sa), Some(sb)) if sb > 0.0 => scale * sa / sb,
        _ => scale,
    }
}

fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(a: usize, b: usize, place: Option<Place>) -> Pair {
        let ev = Evidence { relation: Relation::Direct, aligned_points: Some(12), frame_overlap: 1.0, pixel_correlation: 0.95, mirrored: false, inverted: false };
        Pair { a: a as u32, b: b as u32, ev, scale: 0.5, place, clean: true }
    }

    /// Met both ways round, a pair is stated once, from the first: no reader
    /// is left to choose between two.
    #[test]
    fn a_pair_met_both_ways_round_is_stated_once() {
        let met = [(0, 1, Stated::Identical), (1, 0, Stated::SamePixels), (2, 1, Stated::SamePixels), (1, 2, Stated::Identical), (0, 2, Stated::Identical)];
        let kept: Vec<(usize, usize, bool)> = unique(met).into_iter().map(|(a, b, s)| (a, b, matches!(s, Stated::Identical))).collect();
        assert_eq!(kept, [(0, 1, true), (2, 1, false), (0, 2, true)]);
    }

    /// Read from either end, a pair says the same thing: the other file, its
    /// two files lower first, and where each lies in the other.
    #[test]
    fn a_pair_reads_alike_from_either_end() {
        // `a` at half the size, 10% in from the left of `b`.
        let p = pair(7, 3, Some(Place([0.5, 0.0, 0.1, 0.0, 0.5, 0.0])));
        assert_eq!((p.other(7), p.other(3)), (3, 7));
        assert_eq!(p.key(), (3, 7));
        assert_eq!(p.place_from(7), p.place);
        assert_eq!(p.place_from(3), Some(Place([2.0, 0.0, -0.2, 0.0, 2.0, 0.0])));
        assert_eq!(pair(7, 3, None).place_from(3), None);
    }
}
