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
| F1 | **0.980** | 0.890 (SSCD) |
| precision | **100.0%** | 100.0% (six tools) |
| recall | **96.0%** | 80.6% (SSCD) |
| transformations handled perfectly | **102 of 121** | 34 of 121 (SSCD) |
| image inside a bigger image | **6-8 of 8** | 0-3 of 8 |
| wall clock | 161 s | 1178 s (SSCD) |
| peak memory | 1006 MB | 1376 MB (SSCD) |

3,137 images, 46 originals, 124 transformations, ground truth generated rather
than judged, every tool run cold and alone on the same laptop.
`benchmark/BASELINE.md` has the method and the other ten tools.

Where the difference is, out of 8 originals each:

| | img-fp | SSCD | imagededup-CNN | czkawka | PDQ |
|---|---:|---:|---:|---:|---:|
| `collage_2x2` | **8/8** | 0/8 | 0/8 | 0/8 | 0/8 |
| `embed_half` | **8/8** | 1/8 | 0/8 | 0/8 | 0/8 |
| `pdf_page` | **8/8** | 1/8 | 1/8 | 0/8 | 0/8 |
| `slide_deck` | **8/8** | 3/8 | 1/8 | 0/8 | 0/8 |
| `phone_screenshot` | **8/8** | 3/8 | 2/8 | 0/8 | 0/8 |
| `crop_strip_top` | **8/8** | 1/8 | 1/8 | 0/8 | 0/8 |

and out of 38:

| | img-fp | SSCD | imagededup-CNN | czkawka | PDQ |
|---|---:|---:|---:|---:|---:|
| `crop_center_25` | **33/38** | 0/38 | 0/38 | 0/38 | 0/38 |
| `quadrant_tl` | **30/38** | 0/38 | 0/38 | 0/38 | 0/38 |
| `crop_50_upscaled` | **36/38** | 13/38 | 0/38 | 0/38 | 0/38 |

Precision is not traded for any of it: 14 false pairs in 96,638, and 12 of
those are one deliberate trap — a crop that happens to fall inside a single
tile of a tile-shuffled image, where one transform really does explain the
match. SSCD takes that trap 565 times.

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
compared blockwise. Blocks with no detail on either side abstain rather than
agreeing for free. A pair is claimed only when a single transform explains the
match and the pixels along it agree, which is why a tile-shuffled image — every
pixel of the original, in the wrong places — is rejected.

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

| | |
|---|---|
| `--work-size 640` | long side the analysis runs at. Lower is faster and blinder. |
| `--features 600` | local features kept per image. |
| `-k 150` | candidates verified per image. |
| `--min-inliers 8` | correspondences a claim needs. |
| `--min-overlap 0.85` | how much of one image must lie inside the other. |
| `--min-agreement 0.6` | fraction of compared blocks that must agree. |
| `--cache PATH` | reuse the per-image analysis between runs. |
| `-j N` | worker threads. Default: all cores. |
| `-v` | timings per stage. |
| `--dump PATH` | every verdict considered, accepted or not, as CSV. |

## Cost

161 s and 1.0 GB for 3,137 images on a thermally-limited Ryzen 7 3700U
laptop, reading everything from a cold page cache. Decoding is about a third of
that and local feature extraction most of the rest; matching 4.8 million
possible pairs down to 96,638 claims takes under 30 s. With `--cache`, a second
run over the same directory is about 30 s.

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
