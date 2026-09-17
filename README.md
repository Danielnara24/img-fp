# img-fp

Finds images that are the same picture, however they were re-encoded, resized,
cropped, rotated, recoloured, or pasted into something else.

Linux, CLI only.

```
img-fp ~/Pictures                       # print groups, one per blank-line block
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
| F1 | **0.969** | 0.762 (SSCD) |
| precision | 99.6% | 100.0% (three tools, at 25% recall) |
| recall | **94.4%** | 64.8% (SSCD) |
| transformations handled perfectly | **50 of 87** | 0 of 87 |
| image inside a bigger image | **54-62 of 62** | 0-30 of 62 (SSCD) |
| wall clock | **105 s** | 1,949 s (SSCD) |
| peak memory | **875 MB** | 1,479 MB (SSCD) |

5,638 images, 62 originals, 90 transformations, ground truth generated rather
than judged, every tool run cold and alone on the same laptop.
`benchmark/BASELINE.md` has the method and the other eleven tools.

No transformation is a single fixed point: each draws its amount per seed, so
`scale_small` runs from 0.09 to 0.27 and `jpeg_low` from quality 7 to 26. A
transformation counts as *handled perfectly* only when all 62 seeds are found,
across that whole range. img-fp clears 50 of them. **No other tool clears one.**

Where the difference is, out of 62 originals each:

| | img-fp | SSCD | imagededup-CNN | czkawka | PDQ |
|---|---:|---:|---:|---:|---:|
| `collage_cell` | **61/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `magazine_spread` | **59/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `contact_sheet` | **54/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `picture_in_picture` | **58/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `embed_tiny` | **54/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `slide_deck` | **60/62** | 21/62 | 1/62 | 0/62 | 0/62 |
| `pdf_page` | **62/62** | 16/62 | 2/62 | 0/62 | 0/62 |
| `crop_quarter` | **60/62** | 9/62 | 0/62 | 0/62 | 0/62 |
| `crop_micro` | **40/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `crop_strip_top` | **57/62** | 5/62 | 2/62 | 0/62 | 0/62 |

Where it does *not* lead: PDQ takes `halftone` 58/62 against 56, and SSCD
`rot180` 61 against 60. Non-affine warping used to be on this list and no
longer is — `perspective_top` and `keystone_side` are level with SSCD at 61/62
each, and img-fp leads `barrel_distort` (58 v 48) and `wave_vertical` (57 v
49). Every perceptual hash is at zero on all four warps.

Precision is not traded for any of it. Of 832 false pairs, 830 are the
deliberate rearrangement traps — `column_roll` slides the frame sideways and
wraps, so a crop landing in the unbroken 63% really is present in both files.
Outside the traps: **2 wrong pairs in 220,657 proposals**, against 15.65
million chances to be wrong. SSCD takes the traps 11,237 times.

Those two are two wrong cluster merges, and that is the number worth watching
rather than the pair count: a bad merge costs every pair the two families
imply, so its price grows with the corpus while a lone bad pair's does not.

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
abstain rather than agreeing for free. A pair is claimed only when a single
transform explains the match and the pixels along it agree, which is why a
tile-shuffled image — every pixel of the original, in the wrong places — is
rejected.

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
 "groups": [["...", "..."]]}
```

`pairs` is what the tool asserts; each entry is a pair it actually tested.
`groups` is their transitive closure, for convenience. The two are not
interchangeable — expanding a group back into pairs credits the tool with
matches it never made.

## Options

There are deliberately few. Every option here is one that changes what the
tool costs or what it is willing to claim; anything that was only ever a
number somebody fitted to a corpus has been removed or derived. See
`benchmark/VALIDATION.md` for which, and what the evidence was.

**What it costs**

| | |
|---|---|
| `--work-size 640` | long side the analysis runs at. Lower is faster and blinder. |
| `--features 600` | local features kept per image. |
| `-k 150` | candidates verified per image. |

**What it will claim**

| | |
|---|---|
| `--min-inliers 10` | correspondences a claim needs. |
| `--min-overlap 0.85` | how much of one image must lie inside the other. |
| `--min-agreement 0.6` | fraction of compared blocks that must agree. |
| `--ratio 0.9` | Lowe ratio. Higher keeps ambiguous matches for geometry to filter. |

**Plumbing**

| | |
|---|---|
| `--cache PATH` | reuse the per-image analysis between runs. |
| `-j N` | worker threads. Default: all cores. |
| `-o PATH` | write JSON instead of a summary. |
| `-v` | timings per stage. |
| `--dump PATH` | every verdict considered, accepted or not, as CSV. |
| `--no-propagate` | skip the transform-propagation pass. |

## Cost

105 s and 875 MB for 5,638 images on a thermally-limited Ryzen 7 3700U laptop,
reading everything from a cold page cache. Decoding is about 30% of that and
local feature extraction about half; narrowing 15.9 million possible pairs down
to 220,657 claims takes the remaining fifth. With `--cache`, a second run over
the same directory is a quarter of the time.

The peak is mostly the analysis itself — a few hundred kilobytes of descriptors
and one thumbnail per image, which is what the matching stages read. Decoding a
photograph costs far more than keeping it (a 44-megapixel file is 133 MB of RGB
and reduces to 1.2 MB), so the workers share a budget for decoded pictures and
wait for room in it rather than letting the peak be decided by how many large
files happen to sit next to each other in the directory.

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
