//! Which files are one file, and the pairs that says.
//!
//! Three kinds of sameness, each finer than the analysis can be fooled by and
//! each found once:
//!
//! - **The same bytes** (`exact_groups`), before anything is decoded.
//! - **The same analysis**: files not the same bytes that decoded to one
//!   working plane, a photograph saved again as PNG, TIFF or lossless WebP
//!   (`same_analysis`). The analysis is grey.
//! - **The same colour**, of those: what grey cannot see (`colour_heads`). A
//!   colour JPEG and its lossless greyscale copy have one analysis and are
//!   not the same pixels.
//!
//! Each set is answered for by its lowest-indexed file. Only that file is
//! described (for the bytes), and only that file is matched (for the
//! analysis); the others are given its pairs at the end (`Same::expand`), and
//! the pairs between them are stated without a pass verifying any
//! (`Same::pairs`). Everything that asks "is this file another's" asks here:
//! the run used to keep the three answers as loose vectors and spread them
//! back over the files in each place that needed them, by hand, each its own
//! way, and the keep rule then rebuilt the sets from the pairs.

use crate::{Claim, Edge, Item, decode, suggest};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;

/// How two files that are one file are stated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    /// The same bytes.
    Identical,
    /// Not the same bytes, the same pixels.
    SamePixels,
    /// One analysis, and not the same colour: the same picture in grey,
    /// which is a match as perfect as a match is (`Verdict::perfect`).
    SameGrey,
}

pub(crate) struct Same {
    /// Each file's byte original: the first of its set of identical bytes,
    /// or itself.
    bytes: Vec<usize>,
    /// The sets of identical bytes, each in index order, and each one's place
    /// by its original.
    groups: Vec<Vec<usize>>,
    at: HashMap<usize, usize>,
    /// Each file's analysis original: the file matched for it.
    analysis: Vec<usize>,
    /// The files each analysis original answers for, itself first and the
    /// rest in index order, for the originals that answer for another.
    classes: HashMap<usize, Vec<usize>>,
    /// Each file's colour head: the first file of its analysis that is also
    /// of its colour.
    colour: Vec<usize>,
}

impl Same {
    /// `n` files, of which `groups` are the same bytes; no file is yet known
    /// to share another's analysis.
    ///
    /// **The original is a group's first file**, whatever the cache holds.
    /// Electing the member the cache knew made the original depend on the
    /// order of whichever scan wrote the record, and the original decides
    /// what the vocabulary samples: a cached run over IMGS-AGAINST, from
    /// records IMGS-ALL's scan had written, moved 184 verdicts against the
    /// same run cold.
    pub(crate) fn of_bytes(n: usize, groups: Vec<Vec<usize>>) -> Same {
        let mut bytes: Vec<usize> = (0..n).collect();
        let mut at = HashMap::new();
        for (k, g) in groups.iter().enumerate() {
            for &i in g {
                bytes[i] = g[0];
            }
            at.insert(g[0], k);
        }
        let classes = groups.iter().map(|g| (g[0], g.clone())).collect();
        Same { analysis: bytes.clone(), colour: bytes.clone(), bytes, groups, at, classes }
    }

    /// The sets of identical bytes.
    pub(crate) fn byte_groups(&self) -> &[Vec<usize>] {
        &self.groups
    }

    /// The file whose bytes `i` is: the one described for it.
    pub(crate) fn original(&self, i: usize) -> usize {
        self.bytes[i]
    }

    /// Whether `i` is another file's bytes.
    pub(crate) fn is_copy(&self, i: usize) -> bool {
        self.bytes[i] != i
    }

    /// `i` and every other file of its bytes, in index order.
    pub(crate) fn identical_to(&self, i: usize) -> &[usize] {
        match self.at.get(&self.bytes[i]) {
            Some(&k) => &self.groups[k],
            None => std::slice::from_ref(&self.bytes[i]),
        }
    }

    /// The file matched for `i`, from the vocabulary to corroboration.
    pub(crate) fn answers_for(&self, i: usize) -> usize {
        self.analysis[i]
    }

    /// Settled once each byte original is described: which share an
    /// analysis, and of those which a colour. `files` is for the colours a
    /// decode to grey did not measure; see `same_colour`.
    pub(crate) fn analysed(&mut self, files: &[PathBuf], items: &[Item]) {
        let chroma = std::sync::Mutex::new(HashMap::new());
        self.settle(items, |a, b| same_colour(files, items, &chroma, a, b));
    }

    /// `analysed`, with the colour asked of `same_colour`.
    ///
    /// Not a file of several pictures: one whose first picture is another
    /// file's is not that file (two animations from one first frame, a
    /// document whose first page is a picture), so it is matched as any
    /// other file is, and stated as no more than a match. A byte copy is
    /// answered for by its original, and is its original's colour.
    fn settle(&mut self, items: &[Item], same_colour: impl Fn(usize, usize) -> bool + Sync) {
        let n = self.bytes.len();
        let pixel_of =
            same_analysis(items, |i| !self.is_copy(i) && items[i].ok && items[i].feats.len() > 0 && !items[i].traits.frames);
        let head = colour_heads(&pixel_of, same_colour);
        self.analysis = (0..n).map(|i| pixel_of[self.bytes[i]]).collect();
        self.colour = (0..n).map(|i| head[self.bytes[i]]).collect();
        let mut classes: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, &o) in self.analysis.iter().enumerate() {
            if o != i {
                classes.entry(o).or_insert_with(|| vec![o]).push(i);
            }
        }
        self.classes = classes;
    }

    /// Every file `o` answers for, itself first; `o` alone if none.
    fn class<'a>(&'a self, o: &'a usize) -> &'a [usize] {
        self.classes.get(o).map_or(std::slice::from_ref(o), Vec::as_slice)
    }

    /// `claims`, which run between the files matched, stated for every file
    /// either end answers for as well: a claim from `a` to `b` becomes one
    /// from each file `a` answers for to each file `b` does, made the same way.
    ///
    /// The verdict is the originals', which is the verdict the others would
    /// have been given, since their analysis is the originals' own. Every
    /// edge still runs from the lower index to the higher, so a file on the
    /// other side of the other end takes the verdict reversed: its scale is
    /// then the scale between the files it names.
    ///
    /// Taken and handed back rather than copied: these are the run's whole
    /// list of pairs, and a copy beside the original (or, with no copies at
    /// all, a duplicate of it) was held to the end of the run, some 110 MB on
    /// 27,000 pictures at the point where the run peaks.
    pub(crate) fn expand(&self, claims: Vec<Claim>) -> Vec<Claim> {
        if self.classes.is_empty() {
            return claims;
        }
        let mut out = Vec::with_capacity(claims.len());
        for &Claim { e, how } in claims.iter() {
            let (a, b) = e.ends();
            for &x in self.class(&a) {
                for &y in self.class(&b) {
                    let e = if (x < y) == (a < b) {
                        Edge::new(x, y, e.v)
                    } else {
                        match e.v.reversed() {
                            Some(r) => Edge::new(y, x, r),
                            None => Edge::new(x, y, e.v),
                        }
                    };
                    out.push(Claim { e, how });
                }
            }
        }
        out
    }

    /// The pairs of files that are one file, each from the lower index to
    /// the higher and in the order the report meets them: the same bytes,
    /// set by set, leaving out files `stated` refuses (two copies of a README
    /// are not a duplicate image); then the files sharing an analysis, by
    /// the file matched for them, each pair the same pixels or the same grey.
    pub(crate) fn pairs(&self, stated: impl Fn(usize) -> bool) -> Vec<(usize, usize, Kind)> {
        let mut out = Vec::new();
        for g in &self.groups {
            let g: Vec<usize> = g.iter().copied().filter(|&i| stated(i)).collect();
            for (w, &a) in g.iter().enumerate() {
                out.extend(g[w + 1..].iter().map(|&b| (a, b, Kind::Identical)));
            }
        }
        let mut heads: Vec<&usize> = self.classes.keys().collect();
        heads.sort_unstable();
        for o in heads {
            let class = self.class(o);
            for (w, &a) in class.iter().enumerate() {
                for &b in &class[w + 1..] {
                    if self.bytes[a] != self.bytes[b] {
                        out.push((a, b, if self.colour[a] == self.colour[b] { Kind::SamePixels } else { Kind::SameGrey }));
                    }
                }
            }
        }
        out
    }
}

/// For every file, the file whose analysis stands for it: the lowest-indexed
/// of the files `candidate` admits that were analysed to exactly the same
/// features and thumbnail, and itself otherwise.
///
/// **The analysis is a function of the working plane and nothing else**, so
/// two files analysed alike are two files that decoded to one picture at the
/// working size — a photograph saved again as PNG, TIFF or lossless WebP, a
/// JPEG with its metadata stripped — and matching them separately compares a
/// picture with itself. It is decided from the analysis rather than from the
/// plane (as `Planes` does), so that a cached run, which decodes nothing,
/// decides it alike. A hash of the keypoints and the thumbnail finds the
/// candidates, and every one is then compared whole, descriptors included: a
/// shared hash is a reason to compare, as in the exact pass.
///
/// A featureless file is never a candidate. Two blank pictures of one size
/// stretch to the same thumbnail whatever their shade.
///
/// Alike here is alike in grey, which is all the analysis sees, and that is
/// what matching them as one rests on; whether they are also the same
/// *pixels* is `colour_heads`'s question.
fn same_analysis(items: &[Item], candidate: impl Fn(usize) -> bool + Sync) -> Vec<usize> {
    use std::hash::{BuildHasher, Hash, Hasher};
    let n = items.len();
    let keys = std::collections::hash_map::RandomState::new();
    let hashed: Vec<(u64, usize)> = (0..n)
        .into_par_iter()
        .filter(|&i| candidate(i))
        .map(|i| {
            let (f, t) = (&items[i].feats, &items[i].thumb);
            let mut h = keys.build_hasher();
            (f.w, f.h, f.kps.len(), t.w, t.h, t.scale.to_bits()).hash(&mut h);
            for k in f.kps.iter() {
                (k.x.to_bits(), k.y.to_bits(), k.sigma.to_bits(), k.angle.to_bits()).hash(&mut h);
            }
            t.px.hash(&mut h);
            (h.finish(), i)
        })
        .collect();
    let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
    for (h, i) in hashed {
        by_hash.entry(h).or_default().push(i);
    }
    let alike = |a: &Item, b: &Item| {
        let (fa, fb, ta, tb) = (&a.feats, &b.feats, &a.thumb, &b.thumb);
        fa.w == fb.w
            && fa.h == fb.h
            && fa.kps.len() == fb.kps.len()
            && fa.kps.iter().zip(fb.kps.iter()).all(|(p, q)| {
                (p.x.to_bits(), p.y.to_bits(), p.sigma.to_bits(), p.angle.to_bits())
                    == (q.x.to_bits(), q.y.to_bits(), q.sigma.to_bits(), q.angle.to_bits())
            })
            && fa.desc == fb.desc
            && (ta.w, ta.h, ta.scale.to_bits()) == (tb.w, tb.h, tb.scale.to_bits())
            && ta.px == tb.px
    };
    let mut of: Vec<usize> = (0..n).collect();
    let groups: Vec<Vec<usize>> = by_hash.into_values().filter(|g| g.len() > 1).collect();
    let answered: Vec<(usize, usize)> = groups
        .into_par_iter()
        .flat_map_iter(|mut group| {
            group.sort_unstable();
            let mut firsts: Vec<usize> = Vec::new();
            let mut out = Vec::new();
            for i in group {
                match firsts.iter().find(|&&f| alike(&items[f], &items[i])) {
                    Some(&f) => out.push((i, f)),
                    None => firsts.push(i),
                }
            }
            out
        })
        .collect();
    for (i, f) in answered {
        of[i] = f;
    }
    of
}

/// For every file, the lowest-indexed file `same_analysis` put with it that
/// is also alike in colour (`same_colour`), and itself otherwise: the files
/// that are the same *pixels*, of those the analysis cannot tell apart.
///
/// **Grey is all the analysis sees**, and a JPEG's grey is its Y plane, so a
/// copy with its colour taken out losslessly (`jpegtran -grayscale`, a
/// viewer's "lossless greyscale", the benchmark's own `greyscale`) or its
/// chroma edited has its original's analysis to the bit. Stated as the same
/// pixels, it stood as one file with the original in the keep rule, the
/// smaller of the two stood for both — the grey copy — and the colour
/// photograph was suggested for deletion. Such a pair is still matched as
/// one file, since its matches are the same matches (matched apart, each
/// counted the other and stopped the second look: 0.07 points of recall on
/// IMGS-ALL); it is stated as the perfect match it is in grey, and the keep
/// rule's colour over grey decides between them.
fn colour_heads(of: &[usize], same_colour: impl Fn(usize, usize) -> bool + Sync) -> Vec<usize> {
    let mut classes: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &o) in of.iter().enumerate() {
        if o != i {
            classes.entry(o).or_insert_with(|| vec![o]).push(i);
        }
    }
    // On every thread, since the colour may have to be decoded.
    let answered: Vec<(usize, usize)> = classes
        .into_values()
        .collect::<Vec<_>>()
        .into_par_iter()
        .flat_map_iter(|mut class| {
            class.sort_unstable();
            let mut heads: Vec<usize> = Vec::new();
            let mut out = Vec::new();
            for i in class {
                match heads.iter().find(|&&h| same_colour(h, i)) {
                    Some(&h) => out.push((i, h)),
                    None => heads.push(i),
                }
            }
            out
        })
        .collect();
    let mut head: Vec<usize> = (0..of.len()).collect();
    for (i, h) in answered {
        head[i] = h;
    }
    head
}

/// Whether two files analysed alike are alike in colour too; see
/// `same_analysis`. Cheapest proof first: a colour the decode measured, the
/// same to the bit, which is what files of the same RGB give (a PNG and its
/// TIFF or lossless WebP); two JPEGs that differ only in their metadata;
/// and failing those, the colour of each one's grid read from the file
/// (`decode::chroma_of`), which costs a decode of each.
///
/// The colours are the same when they differ by no more than `COLOURFUL`:
/// the most colour codec noise gives a grey picture, so it is the most two
/// encodings of one picture's colour can differ by and still be noise. A
/// colour photograph against its grey copy differs by its own colour.
fn same_colour(files: &[PathBuf], items: &[Item], chroma: &std::sync::Mutex<HashMap<usize, Option<decode::Chroma>>>, a: usize, b: usize) -> bool {
    let (ca, cb) = (items[a].traits.colour, items[b].traits.colour);
    if ca >= 0.0 && ca.to_bits() == cb.to_bits() {
        return true;
    }
    // Both JPEGs with colour planes, which a decode to grey does not measure.
    if ca < 0.0 && cb < 0.0 && decode::same_jpeg_coding(&files[a], &files[b]) == Some(true) {
        return true;
    }
    let of = |i: usize| -> Option<decode::Chroma> {
        if let Some(&c) = chroma.lock().unwrap().get(&i) {
            return c;
        }
        let c = decode::chroma_of(&files[i]);
        chroma.lock().unwrap().insert(i, c);
        c
    };
    match (of(a), of(b)) {
        (Some(x), Some(y)) => decode::chroma_distance(&x, &y) <= suggest::COLOURFUL,
        _ => false,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::How;
    use crate::sift::{self, Features, DESC_LEN};
    use crate::verify::{Affine, Thumb, Verdict};
    use std::sync::Arc;

    fn feats(seed: u8) -> Features {
        Features {
            w: 64,
            h: 48,
            kps: vec![sift::Keypoint { x: 1.0, y: 2.0, sigma: 1.6, angle: seed as f32 }].into(),
            desc: vec![seed; DESC_LEN].into(),
        }
    }

    fn item(f: Features, t: u8) -> Item {
        Item {
            feats: Arc::new(f),
            thumb: Arc::new(Thumb::new(4, 3, 0.0625, vec![t; 12])),
            dims: (640, 480),
            ok: true,
            err: None,
            damaged: None,
            traits: Default::default(),
        }
    }

    /// Files analysed alike answer to the lowest of them; a file analysed
    /// differently, a featureless one and one `candidate` refuses do not.
    #[test]
    fn files_analysed_alike_are_one_file() {
        let items = vec![
            item(feats(1), 9),
            item(feats(2), 9),
            // Its own copy of 0's analysis, as a cached run holds it.
            item(feats(1), 9),
            // The same features and another thumbnail.
            item(feats(1), 8),
            item(Features { w: 64, h: 48, ..Default::default() }, 9),
            item(Features { w: 64, h: 48, ..Default::default() }, 9),
            item(feats(2), 9),
        ];
        let of = same_analysis(&items, |i| items[i].feats.len() > 0 && i != 6);
        assert_eq!(of, vec![0, 1, 0, 3, 4, 5, 6]);
        // Alike in grey and in colour, the same pixels; alike in grey only,
        // as a grey copy of a colour JPEG is, its own colour.
        assert_eq!(colour_heads(&of, |_, _| true), vec![0, 1, 0, 3, 4, 5, 6]);
        assert_eq!(colour_heads(&of, |a, b| (a, b) != (0, 2)), vec![0, 1, 2, 3, 4, 5, 6]);
        let of = vec![0, 1, 0, 3, 0];
        assert_eq!(colour_heads(&of, |a, b| (a, b) != (0, 2) && (a, b) != (0, 4)), vec![0, 1, 2, 3, 2], "the first file of its colour");
    }

    /// A pair between originals is stated for every copy of either, each
    /// running from the lower index to the higher with the verdict turned to
    /// match — and a run with no copies is left exactly as it was.
    #[test]
    fn a_pair_is_stated_for_every_copy_of_either_file() {
        let m: Affine = [2.0, 0.0, 5.0, 0.0, 2.0, 7.0];
        let v = Verdict { m, n_in: 30, ov_a: 0.25, ov_b: 1.0, scale: 2.0, ..Default::default() };
        let claims = vec![Claim { e: Edge::new(0, 2, v), how: How::Corroborated }];
        // 1 and 4 are copies of 0; 3 is a copy of 2.
        let same = Same::of_bytes(5, vec![vec![0, 1, 4], vec![2, 3]]);
        let out = same.expand(claims.clone());
        let mut names: Vec<(usize, usize)> = out.iter().map(|c| c.e.ends()).collect();
        names.sort_unstable();
        assert_eq!(names, [(0, 2), (0, 3), (1, 2), (1, 3), (2, 4), (3, 4)]);
        assert!(out.iter().all(|c| c.how == How::Corroborated), "each made as the original pair was");
        for c in out.iter() {
            let ((a, b), ev) = (c.e.ends(), &c.e.v);
            assert!(a < b);
            // Files sharing 0's bytes are the small side: scale 2 from them.
            let from_zero = same.original(a) == 0;
            assert_eq!(ev.scale, if from_zero { 2.0 } else { 0.5 }, "{a}-{b}");
            assert_eq!((ev.ov_a, ev.ov_b), if from_zero { (0.25, 1.0) } else { (1.0, 0.25) });
        }
        let alone = Same::of_bytes(5, Vec::new()).expand(claims);
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].e.ends(), (0, 2));
    }

    /// The three kinds of sameness, settled and stated: the same bytes, then
    /// each set sharing an analysis, the same pixels where the colour agrees
    /// and the same grey where it does not. A file of several pictures shares
    /// no analysis, and two identical files that are no pictures are not
    /// stated.
    #[test]
    fn the_files_that_are_one_file_are_stated_once_each() {
        let mut items = vec![
            item(feats(1), 9),
            item(feats(1), 9),
            item(feats(1), 9),
            item(feats(1), 9),
            item(feats(1), 9),
            item(feats(2), 9),
            item(feats(1), 9),
            Item::default(),
            Item::default(),
        ];
        items[6].traits.frames = true;
        // 1 is 0's bytes, 4 is 3's, 7 and 8 are one file and no picture.
        let mut same = Same::of_bytes(items.len(), vec![vec![0, 1], vec![3, 4], vec![7, 8]]);
        // 3 is not the others' colour.
        same.settle(&items, |a, b| a != 3 && b != 3);
        assert_eq!((0..9).map(|i| same.answers_for(i)).collect::<Vec<_>>(), [0, 0, 0, 0, 0, 5, 6, 7, 7]);
        assert_eq!(same.identical_to(4), [3, 4]);
        assert_eq!(same.identical_to(2), [2]);
        use Kind::*;
        assert_eq!(
            same.pairs(|i| i != 8),
            [
                (0, 1, Identical),
                (3, 4, Identical),
                (0, 2, SamePixels),
                (0, 3, SameGrey),
                (0, 4, SameGrey),
                (1, 2, SamePixels),
                (1, 3, SameGrey),
                (1, 4, SameGrey),
                (2, 3, SameGrey),
                (2, 4, SameGrey),
            ]
        );
        // A match of 0's is a match of every file it answers for.
        let out = same.expand(vec![Claim { e: Edge::new(0, 5, Verdict::default()), how: How::Direct }]);
        assert_eq!(out.iter().map(|c| c.e.ends()).collect::<Vec<_>>(), [(0, 5), (1, 5), (2, 5), (3, 5), (4, 5)]);
    }
}
