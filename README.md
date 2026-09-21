# img-fp

Finds images that are the same picture, however they were re-encoded, resized,
cropped, rotated, recoloured, or pasted into something else.

Linux, CLI only.

```
img-fp ~/Pictures                       # one group per blank-line block, keeper first
img-fp ~/Pictures -o dupes.json         # full results with transforms and evidence
img-fp ~/Pictures --cache ~/.cache/imgfp.bin   # reuse analysis between runs
```

## What it is for

Most duplicate finders reduce each image to one number and compare numbers.
That works for a re-encode and fails for everything else, because a hash of the
whole image changes when any part of the image changes. On the benchmark in
`benchmark/`, the eleven tools measured find **nothing at all** — 0 out of 8 —
when an image is pasted into a collage, a slide, or a phone screenshot, even
though every pixel of the original is present and untouched.

img-fp matches *parts* of images to parts of other images, and states the
geometric relationship it found. So it finds the photograph inside the slide,
the thumbnail of the 4000-pixel original, and the crop that kept a fifth of the
frame.

| | img-fp | best of eleven others |
|---|---:|---:|
| F1 | **0.978** | 0.762 (SSCD) |
| precision | 99.5% | 100.0% (three tools, at 25% recall) |
| recall | **96.1%** | 64.8% (SSCD) |
| transformations handled perfectly | **59 of 87** | 0 of 87 |
| image inside a bigger image | **55-62 of 62** | 0-30 of 62 (SSCD) |
| wall clock | **105 s** | 1,949 s (SSCD) |
| peak memory | **875 MB** | 1,479 MB (SSCD) |

5,638 images, 62 originals, 90 transformations, ground truth generated rather
than judged, every tool run cold and alone on the same laptop.
`benchmark/BASELINE.md` has the method and the other eleven tools. The two
timing rows are from that one session, where every tool faced the same
temperature and the same cold page cache, and they are left as measured: a
figure from a different session is not comparable with the ones beside it.
img-fp has since had an optimisation pass worth ~9% of its CPU seconds for
identical output, and a parameter pass — which is what moved the accuracy rows
— that was level on the clock.

No transformation is a single fixed point: each draws its amount per seed, so
`scale_small` runs from 0.09 to 0.27 and `jpeg_low` from quality 7 to 26. A
transformation counts as *handled perfectly* only when all 62 seeds are found,
across that whole range. img-fp clears 59 of them. **No other tool clears one.**

Where the difference is, out of 62 originals each:

| | img-fp | SSCD | imagededup-CNN | czkawka | PDQ |
|---|---:|---:|---:|---:|---:|
| `collage_cell` | **62/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `magazine_spread` | **60/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `contact_sheet` | **55/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `picture_in_picture` | **60/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `embed_tiny` | **55/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `slide_deck` | **60/62** | 21/62 | 1/62 | 0/62 | 0/62 |
| `pdf_page` | **62/62** | 16/62 | 2/62 | 0/62 | 0/62 |
| `crop_quarter` | **60/62** | 9/62 | 0/62 | 0/62 | 0/62 |
| `crop_micro` | **40/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `crop_strip_top` | **57/62** | 5/62 | 2/62 | 0/62 | 0/62 |

Where it does *not* lead: PDQ takes `halftone` 58/62 against 56, and SSCD
`rot180` 61 against 60. Non-affine warping used to be on this list and no
longer is — img-fp takes `perspective_top` and `keystone_side` 62/62 each
against SSCD's 61, and leads `barrel_distort` (61 v 48) and `wave_vertical`
(59 v 49). Every perceptual hash is at zero on all four warps.

Precision is not traded for any of it. All 1,088 of its false pairs are the
deliberate rearrangement traps — `column_roll` slides the frame sideways and
wraps, so a crop landing in the unbroken 63% really is present in both files.
Outside the traps: **no wrong pair at all in 224,779 proposals**, against 15.65
million chances to be wrong. SSCD takes the traps 11,237 times.

What that count really tracks is wrong *cluster merges*, and it is the number
worth watching rather than the pair total: a bad merge costs every pair the two
families imply, so its price grows with the corpus while a lone bad pair's does
not. There are none here, where the previous rule left two — but two and zero
are both small numbers, and nothing has been tested above a few thousand
images.

## How it works

```
walk ──► exact hash ──► decode ──► local features ──► vocabulary
                                                          │
       groups ◄── propagate ◄── verify ◄── inverted-file candidates
```

**Exact duplicates first.** Files are grouped by size, and only same-size files
are read and hashed. On a normal directory this finds every byte-identical copy
for almost nothing.

**Local features, not a global hash.** Each image is decoded once to a small
grayscale working image and described by up to 600 scale-invariant keypoints in
the SIFT family. Being local is the whole point: a crop keeps the features
inside it, and an image pasted into a slide keeps all of them. Being
scale-invariant is what lets a 160-pixel thumbnail match its 4000-pixel
original. The channel used is the mean of R, G and B rather than a luma
weighting, so swapping the channels does not change what the tool sees.

**A vocabulary built from the corpus being searched.** Descriptors are
quantised through a hierarchical k-means tree trained on a sample of the images
in front of it, and indexed in an inverted file. Candidates are ranked by an
idf-weighted *containment* score — the fraction of one image's landmarks
present in the other — rather than by similarity. A cosine would rank a small
crop against its own parent near zero, because the two have wildly different
numbers of features, and containment is the case that matters most.

**Every claim is verified geometrically and then against the pixels.** A
candidate is not accepted because it scored well. Matched descriptors propose a
transform, the transform is fitted and its inliers counted, the two frames are
intersected through it, and the overlap is resampled from both images and
compared blockwise. Both sides are read through a mip pyramid at the scale the
comparison actually samples them, so that a 4000-pixel original and its
160-pixel thumbnail are compared at a resolution both of them have rather than
one being aliased against the other. Blocks with no detail on either side
abstain rather than agreeing for free, and the rest are averaged: *how well*
the overlap agrees, not how many of its blocks cleared a bar. A pair is claimed
only when a single transform explains the match and the pixels along it agree,
which is why a tile-shuffled image — every pixel of the original, in the wrong
places — is rejected.

**Transforms compose, so matching is transitive but claims are not.** If A is a
crop of B and B is a crop of C, the A-to-C transform is known exactly. img-fp
composes it and *checks it against the pixels* rather than assuming it. That
recovers most of the pairs direct matching misses, at roughly the cost of a
memory read, and it is still a real test of that pair.

**Three acceptance tests, not one threshold.** A match believed on its own
evidence (an *anchor*) is the only kind that can put two files in one cluster,
so it is held to the highest standard: a wrong one does not cost a pair, it
costs every pair the two clusters imply. A match between files already in one
cluster is held to a lower one, because it cannot merge anything. And any
single match that is the *only* link between two clusters is held higher still,
or dropped — during development one such link turned two beach photographs into
354 false pairs, and another turned a 225-pixel picture of the Earth into 3,002.

An anchor also has to show that its evidence reaches around what it claims: the
inliers must bracket the middle of the region the transform says the two images
share. A fitted transform interpolates between its correspondences and
extrapolates beyond them, and two different photographs laid out on the same
page template match along the template — the rules, the margins, the caption —
from which the transform then claims the whole page, including the photograph
it never touched. That was the source of every cross-family mistake the tool
made.

## Output

Human-readable by default: one group per blank-line-separated block.

With `-o`, JSON carrying what each claim rests on:

```json
{"tool": "img-fp",
 "pairs": [{"a": "...", "b": "...", "inliers": 214, "overlap": 1.0,
            "agreement": 1.0, "scale": 0.25}],
 "groups": [{"representative": "...", "files": ["...", "..."]}]}
```

`pairs` is what the tool asserts; each entry is a pair it actually tested.

A **group is a representative and every file that matched it directly**. The
representative is the best-connected file — usually the original or a clean
re-encode — and it comes first in the human-readable output. Every other member
was verified against *it*, by the same pixel check as any other claim, so a
group is a set of pairs the run really made. It is the file to keep, and the
one every file you would delete on the strength of the group was compared with.

It is deliberately neither of the two obvious things:

- Not the **transitive closure**, because matching is not transitive. A
  photograph inside a slide and the same photograph on a poster are each a
  match for the photograph and not for each other; a left half and a right half
  are both crops of the whole and share nothing. The closure merges those and
  then asserts, of pairs nobody checked, that they are the same picture — on
  the benchmark corpus, 13,773 untested pairs of which 10,208 are wrong. It is
  also fragile: one bad pair between two families merges them entirely, which
  measured at ~7,400 false pairs from a single edge.
- Not **maximal cliques** (which is what the sibling `vid-fp` uses), because a
  family of a photograph and its ninety transformations is not a complete
  graph. The same corpus gives 6,991 cliques for 62 families, one file
  appearing in 577 of them.

Measured: **122 groups covering all 5,512 matched files, 9,713 claims, none of
them untested**, in 11 ms.

Two consequences, both deliberate:

- **Groups overlap.** A file that is a duplicate of two representatives is
  reported under both — that is how a file only one of them reached gets
  reported at all. The output is a list of relationships, not a partition.
- **A group is not an all-pairs claim.** Two members that both matched the
  representative have not been compared with each other, so expanding a group
  into pairs asserts more than the run did. Read `pairs` for the claims
  themselves.

## Options

There are deliberately few, and each pass over the tool has removed more than
it added: thirteen options once changed the result, and six do. Anything that
was only ever a number somebody fitted to a corpus has been removed or derived.
See `benchmark/VALIDATION.md` for the rule and `CLAUDE.md` for every sweep.

**What it costs**

| | |
|---|---|
| `--work-size 640` | long side the analysis runs at, Higher = Slower. Usable 448-896. |
| `-k 150` | candidates verified per image, Higher = Slower. Usable 100-300. |

**What it will claim**

| | |
|---|---|
| `--min-inliers 10` | correspondences a claim needs, Higher = Stricter. Usable 8-20. |
| `--min-overlap 0.85` | how much of one image must lie inside the other, Higher = Stricter. Usable 0.60-0.95. |
| `--min-agreement 0.5` | how well the overlap must correlate, averaged over the blocks that carry detail, Higher = Stricter. Usable 0.45-0.70. |
| `--no-propagate` | skip the transform-propagation pass, On = Stricter. Costs 7.1 points of F1. |

Stricter is not safer. Below the bottom of each range unrelated photographs
start merging into one family, and above the top the tool simply stops finding
things; the defaults sit where they do to keep a margin from the first, not to
win the second.

**Plumbing**

| | |
|---|---|
| `--cache PATH` | reuse the per-image analysis between runs. |
| `-j N` | worker threads. Default: all cores. |
| `-o PATH` | write JSON instead of a summary. |
| `-v` | timings per stage. |
| `--dump PATH` | every verdict considered, accepted or not, as CSV. |

Nothing in that last table changes a pair.

## Cost

About 95 s for 5,638 images on a thermally-limited Ryzen 7 3700U laptop,
reading everything from a cold page cache. Decoding is about 35% of that and
local feature extraction about 45%; narrowing 15.9 million possible pairs down
to 224,779 claims takes the remaining fifth. With `--cache`, a second run over
the same directory is a quarter of the time.

Peak memory is two things added together. The steady part is the analysis
itself — a few hundred kilobytes of descriptors and one thumbnail per image,
which is what the matching stages read — and comes to about 650 MB on this
corpus. The rest is decoded picture in flight: decoding a photograph costs far
more than keeping it (a 44-megapixel file is 133 MB of RGB and reduces to
1.2 MB), so the workers share a budget and wait for room in it rather than
letting the peak be decided by how many large files happen to sit next to each
other in the directory. That budget is a fraction of what the machine reports
free, so the total lands between 850 MB and 1,050 MB depending on how much
memory the machine had to spare — and a run that reports a bigger number is
not necessarily doing anything differently.

For scale: the best-scoring competitor takes 1,949 s on the same corpus, and
the fastest thing that finds anything at all beyond byte-identical copies
(imgdupes) takes 33 s to reach F1 0.107.

## Formats

JPEG, PNG, WebP, GIF, BMP, TIFF, AVIF, HEIC/HEIF, JXL, ICO, PNM, TGA, QOI,
OpenEXR, farbfeld. Identified by content, so a file with a wrong extension or
none at all still works — one seed in the benchmark corpus has no extension.

A format a tool cannot open is indistinguishable from one it failed to match,
which is why this list is longer than it looks like it needs to be: three of
the tools in the baseline score zero on AVIF, HEIC and JXL purely because they
cannot read them.

Needs `libheif` (HEIC/AVIF) at build time. Everything else is pure Rust.

## Building

```bash
cargo build --release      # target/release/img-fp
cargo test --release
```
