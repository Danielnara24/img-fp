# img-fp

Finds images that are the same picture, however they were re-encoded, resized,
cropped, rotated, recoloured, or pasted into something else.

Linux, CLI only.

```
img-fp ~/Pictures                       # one group per blank-line block, keeper first
img-fp -r ~/Pictures                    # and every folder below it
img-fp ~/Pictures -o dupes.csv          # the same rows, for a spreadsheet
img-fp ~/Pictures -o dupes.json         # every pair, with what each one rests on
img-fp ~/Pictures --work-size 640       # slower, and finds more of what is embedded
```

The per-image analysis is kept in `$XDG_CACHE_HOME/img-fp/analysis.bin` (or
`~/.cache/img-fp/analysis.bin`), so a second run over the same directory is a
quarter of the time. `--cache PATH` puts it somewhere else and `--no-cache`
keeps it nowhere. One cache serves every directory you scan — a run writes back
what it analysed plus what the file already held about images it did not look
at, and forgets an image once the file is gone.

It is not small: **35 KB an image for photographs, 70 KB for small ones**,
because what it holds is the analysis — 128 bytes of descriptor and 20 of
keypoint for each of up to 600 keypoints, and a small image is the expensive
case rather than the cheap one, since anything under the working size is
enlarged before it is described. `--prune-cache` cuts it back to the corpus in
front of it, `--clear-cache` empties it, and it is one plain file you can
delete at the cost of a re-analysis.

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

| | img-fp (default) | img-fp `--work-size 640` | best of eleven others |
|---|---:|---:|---:|
| F1 | **0.955** | **0.978** | 0.762 (SSCD) |
| precision | 99.6% | 99.5% | 100.0% (three tools, at 25% recall) |
| recall | 91.7% | **96.1%** | 64.8% (SSCD) |
| transformations handled perfectly | 47 of 87 | **60 of 87** | 0 of 87 |
| image inside a bigger image | 27-62 of 62 | **55-62 of 62** | 0-30 of 62 (SSCD) |
| wall clock | **64 s** | 103 s | 1,949 s (SSCD) |
| peak memory | 841 MB | 784 MB | 1,479 MB (SSCD) |

The default runs at `--work-size 384`, below the accuracy knee, and 640 is the
knee. The difference is 2.3 points of F1 — **all of it recall, none of it
precision** — for about 60% more CPU. Read those two memory figures with care:
they come from different sessions, and peak memory here is set by how much the
machine had free at the time as much as by the work size. Measured properly, in
one session with the two sizes alternating, it is **697 MB at the default
against 849 MB at 640**, and the wall clock is 57 s against 82 s.

5,638 images, 62 originals, 90 transformations, ground truth generated rather
than judged, every tool run cold and alone on the same laptop.
`benchmark/BASELINE.md` has the method and the other eleven tools. The two
timing rows are from that one session, where every tool faced the same
temperature and the same cold page cache, and they are left as measured: a
figure from a different session is not comparable with the ones beside it.
img-fp has since had an optimisation pass worth ~9% of its CPU seconds for
identical output, a parameter pass — which is what moved the accuracy rows — and
two more passes over the matcher that were level on this corpus's clock and
worth 5% on a corpus of small images. Then a fifth, which is the only change in
the tool's history to have moved a pair: it stores the vocabulary's centres as
bytes rather than floats, because the descent was waiting for memory, and it is
worth **9% of the CPU seconds here and 22% on a found corpus** for 0.0002 of F1
*upwards* and one more transformation handled perfectly. A sixth took 9% off
the matcher by not paying for verdicts that were going to be thrown away. At
`--work-size 640` the corpus runs in 103 s and 637 CPU-seconds under that
harness; the default runs it in **64 s and 416 CPU-seconds**.

No transformation is a single fixed point: each draws its amount per seed, so
`scale_small` runs from 0.09 to 0.27 and `jpeg_low` from quality 7 to 26. A
transformation counts as *handled perfectly* only when all 62 seeds are found,
across that whole range. img-fp clears 47 of them at the default and 60 at
`--work-size 640`. **No other tool clears one, at any setting.**

Where the difference is, out of 62 originals each:

| | img-fp | at `--work-size 640` | SSCD | imagededup-CNN | czkawka | PDQ |
|---|---:|---:|---:|---:|---:|---:|
| `collage_cell` | **59/62** | **62/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `magazine_spread` | **54/62** | **60/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `contact_sheet` | **29/62** | **55/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `picture_in_picture` | **45/62** | **60/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `embed_tiny` | **27/62** | **55/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `embed_small` | **57/62** | **62/62** | 30/62 | 1/62 | 0/62 | 0/62 |
| `slide_deck` | **56/62** | **62/62** | 21/62 | 1/62 | 0/62 | 0/62 |
| `pdf_page` | **56/62** | **62/62** | 16/62 | 2/62 | 0/62 | 0/62 |
| `crop_quarter` | **59/62** | **60/62** | 9/62 | 0/62 | 0/62 | 0/62 |
| `crop_micro` | **39/62** | **41/62** | 0/62 | 0/62 | 0/62 | 0/62 |
| `crop_strip_top` | **54/62** | **57/62** | 5/62 | 2/62 | 0/62 | 0/62 |

**This is where the default's work size costs the most, and it is not an
accident.** An image inside a bigger image is a small part of the frame, so
shrinking the frame to 384 shrinks it again — `embed_tiny` and `contact_sheet`
roughly halve. The lead over the field survives it intact, because the field is
at zero, but a corpus of slides, screenshots or contact sheets is the case for
passing `--work-size 640`.

Where it does *not* lead: PDQ takes `halftone` 58/62, against 55 at the default
and 56 at 640. That is the only row any of the eleven takes, at either setting.
SSCD had `rot180` 61 against 60 and no longer does — 62/62 now. Non-affine
warping used to be on this list too: img-fp takes `keystone_side` 62/62 against
SSCD's 61, is level on `perspective_top` at the default and ahead at 640, and
leads `barrel_distort` (59 v 48) and `wave_vertical` (56 v 49). Every
perceptual hash is at zero on all four warps.

Precision is not traded for any of it — it is the one column the work size does
not move. All 871 of its false pairs are the deliberate rearrangement traps —
`column_roll` slides the frame sideways and wraps, so a crop landing in the
unbroken 63% really is present in both files. Outside the traps: **no wrong
pair at all in 214,373 proposals**, against 15.65 million chances to be wrong,
and the same is true of all 1,088 of them at 640. SSCD takes the traps 11,237
times.

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

Three layouts, the ones `vid-fp` writes. `-o FILE` picks one by its extension
(`.txt`, `.csv`, `.json`; anything else is text), `--format txt|csv|json` picks
one whatever the file is called, and without `-o` — or with `-o -` — the report
goes to stdout, as text unless `--format` says otherwise. Progress, the summary
line and the problems all go to stderr, so stdout is only ever the report.

**Text**, the default: one block per group, representative first, and each
member beside the pair that put it there.

```
group_1: 3 files
	REP,   4032x3024, 3.1MB, /photos/beach.jpg
	MATCH, 4032x3024, 3.1MB, identical, /backup/beach.jpg
	MATCH, 1024x768, 212.4KB, 412 points, overlap 0.99, correlation 0.93, /phone/beach-small.jpg
```

`points` is how many keypoint correspondences agree on the transform,
`overlap` how much of one frame the transform puts inside the other, and
`correlation` how well the pixels of that overlap agree. A pair can also be
`propagated` — a transform composed along a path through the group and then
checked against the pixels, which no keypoints vouch for — and `mirrored` or
`inverted`. Resolution and size are read from the file, and are `-` where its
header could not be.

**CSV**: the same rows with a header, `;`-separated as `vid-fp`'s are —
`group`, `role` (`representative` or `match`), `path`, `width`, `height`,
`size`, `size_bytes`, then the member's pair with the representative:
`relation` (`identical`, `direct` or `propagated`), `aligned_points`,
`frame_overlap`, `pixel_correlation`, `mirrored`, `inverted`. The
representative's own row leaves those empty, and so is any figure that was not
measured. A file in two groups has a row in each, with each group's evidence.

**JSON**: the complete record. Its groups are the CSV's rows, one object per
file keyed by the CSV's columns, with `null` where the CSV has an empty cell;
beside them is every pair asserted, not only each member's pair with its
representative:

```json
{"tool": "img-fp",
 "pairs": [{"a": "...", "b": "...", "aligned_points": 214,
            "frame_overlap": 1.0, "pixel_correlation": 1.0, "scale": 0.25,
            "mirrored": true}],
 "groups": [{"group": "group_1", "representative": "...",
             "files": [{"path": "...", "role": "representative", "width": 4032, ...},
                       {"path": "...", "role": "match", "relation": "direct",
                        "aligned_points": 214, "frame_overlap": 1.0, ...}]}]}
```

`pairs` is what the tool asserts; each entry is a pair it actually tested.
`mirrored`, `inverted`, `identical` and `propagated` appear only when true.

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

Measured: **123 groups covering all 5,514 matched files, 9,765 claims, none of
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
it added: thirteen options once changed the result, and five do. Anything that
was only ever a number somebody fitted to a corpus has been removed or derived.
See `benchmark/VALIDATION.md` for the rule and `CLAUDE.md` for every sweep.

**What it costs**

| | |
|---|---|
| `--work-size 384` | long side the analysis runs at, Lower = Faster and blinder. |
| `-k 150` | candidates verified per image, Higher = Slower. |

`--work-size` is the only one of the two that is really connected to the clock:
it scales decode and feature extraction, which are 80% of a run, so cost is
near enough linear in it. **The default is below the accuracy knee on
purpose.** 640 is the knee, and going there costs 61% more CPU for 2.3 points
of F1 — all of it recall, and most of that recall is the containment rows,
where the photograph is a small part of a larger canvas and shrinking the
canvas shrinks the photograph past what the detector can describe. Use 640 for
a corpus of screenshots, slides or contact sheets; the default is sized for a
camera roll. Nothing above 640 is worth asking for: 768 buys 0.0006 for 16%
more CPU, and 896 goes backwards and merges two families.

`-k` is on a plateau — 100 through 300 are identical to the pair — so it saves
a little and claims the same either way. There is no third: everything below is
a statement about what the tool should claim, and none of it is a way to buy
time.

**What it will claim**

| | |
|---|---|
| `--min-aligned-points 10` | keypoint correspondences that must agree on one transform, Higher = Stricter. Usable 8-20. |
| `--min-frame-overlap 0.85` | how much of one image's frame must lie inside the other, Higher = Stricter. Usable 0.60-0.95. |
| `--min-pixel-correlation 0.5` | how well the pixels of that overlap must correlate, averaged over the blocks carrying detail, Higher = Stricter. Usable 0.45-0.70. |

The last two are not a loose and a tight version of one bar, however much they
look like it. The overlap floor is **geometry** — how much of a frame the
fitted transform claims, with no pixel read — and the correlation floor is the
**pixels** in it, so each is the only defence against a failure mode the other
cannot see at any setting. Two different photographs on the same page furniture
overlap perfectly and correlate at nothing; an image rolled sideways and wrapped
correlates almost perfectly over the fragment it still shares and overlaps at
little. `CLAUDE.md` has the per-mode numbers.

Stricter is not safer. Below the bottom of each range unrelated photographs
start merging into one family, and above the top the tool simply stops finding
things; the defaults sit where they do to keep a margin from the first, not to
win the second.

**Plumbing**

| | |
|---|---|
| `--cache PATH` | keep the per-image analysis here instead of `~/.cache/img-fp`. |
| `--no-cache` | do not read or write it at all. |
| `--prune-cache` | drop cached analyses of images this scan did not find. |
| `--clear-cache` | delete the cache before running. |
| `-r` | descend into subdirectories. Default: only the images directly in each directory named. |
| `-x EXT,...` | extensions a directory walk takes, as in `vid-fp`. Default: every format below. `-x '*'` takes every file, including those with no extension; `-x '!gif'` every file but those; `-x 'jpg,png,!png'` a list with one removed. |
| `-t N` | worker threads. Default: all cores. |
| `-o PATH` | write the results here; `-` is stdout. Text, CSV or JSON by the extension. |
| `--format F` | `txt`, `csv` or `json`, whatever `-o` is called. |
| `-v` | timings per stage. |
| `--dump PATH` | every verdict considered, accepted or not, as CSV. |
| `--log-file PATH` | everything the run had to say, uncapped. |

Nothing in that last table changes a pair.

## What it skipped, and what went wrong

Nothing is said about a file while the run is working — a per-file line costs
nothing until the day it is a quarter of a million of them pushing the results
off the screen. The last thing a run prints is a count of what it passed over
and what it could not do:

```
128 groups, 217380 pairs over 5637 images in 63.6s

Skipped:
      3  file(s) whose extension is not searched (see -x)
         - /home/daniel/Documents/IMGS/derived/manifest.csv

Problems (5 total):
      1  image(s) could not be read
         - .../WhatsApp Images/Foto 38622.png: unexpected end of file
      4  image(s) have no features and can only match a byte-identical copy
         - .../Desktop/Foto 55132.png
```

**The two lists are different kinds of thing, and only one of them is a
failure.** A skip is something img-fp was never going to read: a file whose
extension is not an image format (which is what makes pointing it at a home
directory reasonable, and is also the one thing that can hide a photograph — a
JPEG named `.txt`, or with no extension, is invisible unless `-x '*'` asks for
every file; a file that walk reaches and that turns out not to be a picture is
a skip too, and is never reported as an identical pair), a symlink met during a walk (a link and its
target are one set of bytes, so following both would manufacture a duplicate
pair out of one file — a path *named* on the command line is followed), or a
file reached twice through overlapping roots. None of those touch the exit
code. A problem is something the run was asked for and did not get, and each
one does.

The last category above is the odd one and it earns its place: those four files
decoded perfectly and described to nothing, because a crop of a night sky has
no local features to find. Nothing failed, and the tool still has nothing to
say about them — which is worth a line, because the alternative is a file that
silently cannot match. It does not fire on ordinary photographs: 2,786 camera
photos produce no skips and no problems at all.

`--log-file PATH` is the unabridged version. The summary names up to ten
examples per category; the log holds every one of them, in full, as they
happen, along with the per-stage timings whether or not `-v` asked for them on
screen. It describes one run and is truncated at the start of it.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Ran clean |
| `1` | Fatal error; the run did not finish |
| `2` | Finished and reported, but something failed |
| `130` | Interrupted with Ctrl-C |

`2` is the `Problems` list above being non-empty. None of it changes the result
— the pairs are the pairs either way — which is exactly why it needs a code,
because the results line reads the same whether every file opened or a third of
them refused. The unreadable images are in the JSON's `failures` as well.

A stale cache is not one of them. A cache written at another `--work-size`
describes a different analysis and is discarded whole by design, silently and
every time; a cache file that is *damaged* is reported, because it will cost a
full re-analysis on every run until someone notices. A cache directory that
cannot be created is reported for the same reason, and the run goes on without
one — the pairs are the pairs either way.

`130` is the shell's convention for a Ctrl-C. An interrupted run keeps
every image it had finished describing, so the next run over the same files
does not describe them again, and it exits at once: each description is
written to the cache as soon as it is made, so there is nothing left to save
when the key is pressed. What the cache holds is the descriptions and nothing
else — matching is about the corpus as a whole, so a run interrupted while
matching keeps every description and does the matching again.

## Cost

About 95 s for 5,638 images on a thermally-limited Ryzen 7 3700U laptop,
reading everything from a cold page cache. Decoding is about 35% of that and
local feature extraction about 45%; narrowing 15.9 million possible pairs down
to 224,779 claims takes the remaining fifth. That is a run with nothing cached;
a second run over the same directory is a quarter of the time, because four
fifths of that work is the analysis and the analysis is kept.

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
OpenEXR, farbfeld. Identified by content, so a file with the wrong extension
still works. One with none at all is left out of a directory walk unless
`-x '*'` is given, as in `vid-fp` — one seed in the benchmark corpus has no
extension, which is why `bench.py` passes it. A file named on the command line
is taken whatever it is called.

A format a tool cannot open is indistinguishable from one it failed to match,
which is why this list is longer than it looks like it needs to be: three of
the tools in the baseline score zero on AVIF, HEIC and JXL purely because they
cannot read them.

Needs `libheif` (HEIC/AVIF) at build time. Everything else is pure Rust.

## Installing

A prebuilt x86_64 Linux binary is attached to each
[release](https://github.com/Danielnara24/img-fp/releases); it needs `libheif1`
1.17 or newer installed (Ubuntu 24.04+, Debian 13+) and a CPU with AVX2. Or
from crates.io, with libheif's development package installed:

```bash
RUSTFLAGS="-C target-cpu=native" cargo install img-fp
```

The flag matters: the hot loops have AVX2 kernels chosen at compile time, and
without it `cargo install` builds the portable fallbacks.

## Building

```bash
cargo build --release      # target/release/img-fp
cargo test --release
```

Inside the repository `.cargo/config.toml` already sets `target-cpu=native`.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

The released binary links the system's `libheif` (LGPL-3.0) dynamically; it is
not bundled, and that does not affect the licence above.
