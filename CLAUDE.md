# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from — read it before
changing how anything is scored.

**Status: the tool exists and beats every measured competitor by a wide
margin.** On the current corpus — 5,638 files, 62 seeds, 90 transformations,
every amount drawn per seed — F1 **0.978** at 99.5% precision and 96.1% recall,
against SSCD's 0.762 / 92.6% / 64.8%, and roughly two minutes against SSCD's
1,949 s.

(**The cost figures are the soft ones here, and the accuracy figures are not.**
Accuracy is a property of the build: F1 0.978 is `out/v9`, reproducible to the
pair. Wall clock is a property of the session — this laptop's idle temperature
alone moves it 25% — and the three numbers people want to compare were taken in
three different sessions: the eleven competitors in `out/v5`, img-fp's previous
build at 105 s in `out/v8`, this build at 122 s in `out/v9` on a die that
started 12 C hotter. The only like-for-like reading of the last two is six
alternating runs of each, which puts them level. So quote "about two minutes,
against SSCD's half hour" and do not put weight on the third significant
figure. See *Speed and memory*.)

**59 of 87 transformations are handled perfectly** (all 62 seeds found across
the whole range of the amount). Every other tool manages **zero**.

Precision holds where it matters: of 1,088 false pairs, **all 1,088 are the
deliberate rearrangement traps**, leaving **no wrong pair at all** in 224,779
proposals against 15.65 million chances to be wrong — and so no wrong cluster
merge. That is the number to watch: a merge's cost is every pair the two
families imply, so it grows with the corpus while a lone bad pair does not.
The previous rule left two of them, and the count is small enough either way
that zero should be read as "none survived", not as a guarantee.

`benchmark/BASELINE.md` holds the competition's numbers,
`benchmark/VALIDATION.md` records the held-out experiment that shaped the
parameter surface, `README.md` explains the design.

What is left is not accuracy-critical: decode is ~35% of the runtime and has no
downscaled JPEG path (`zune-jpeg` exposes none, and the arithmetic for adding a
second decoder is in *Speed and memory* below — it is worth about 4%), the
mirrored and inverted query is asked only where the first pass came up short
rather than being folded into the vocabulary, and nothing has been tested above
a few thousand images.

## Layout

```
src/
  main.rs             CLI, pipeline, propagation, bridge pruning
  decode.rs           format sniffing and decode to one grayscale plane
  sift.rs             scale-invariant local features
  index.rs            vocabulary tree, inverted file, containment scoring
  verify.rs           correspondence, geometry, pixel agreement, the policy
  cache.rs            on-disk cache of the per-image analysis
benchmark/
  BASELINE.md         the competition's numbers. The bar to clear.
  VALIDATION.md       the held-out corpus, and what it says about overfitting
  COMPETITORS.md      tool survey: what exists, what was rejected and why
  TOOLS.md            install/run notes and per-tool gotchas
  bench.py            the measurement harness (cold, sequential, instrumented)
  score.py            F1 against the generated ground truth
  run_bench.sh        simpler sequential driver, no instrumentation
  runners/run_*.py    one wrapper per tool -> canonical JSON
  corpus/
    README.md         how the corpus and its ground truth are built
    transforms.py     CATALOGUE: 90 transforms, every amount drawn per seed
    make_variants.py  the generator
  out/v5/             the competitors' run: eleven tools, one session
  out/v8/             img-fp's published cost row (see BASELINE.md)
  out/v9/             img-fp after the parameter pass
                      (out/v5/DAMAGED.md records a file overwritten there;
                       give bench.py a fresh --out, it merges into the old one)
vendor/               third-party tools and venvs, gitignored
```

The corpus lives outside the repo at `/home/daniel/Documents/IMGS`: 54 seeds
in the root, 8 in `archive/`. `/home/daniel/Documents/IMGS-VAL` holds the 16
seeds that were once a separate validation set and are now folded in; it keeps
no derived tree.

**There is currently no held-out corpus.** There was one, and
`benchmark/VALIDATION.md` records what it bought — it is why the parameter
surface is as small as it is. Folding it in was a deliberate trade, taken on
the grounds that per-seed variance plus a much smaller parameter surface make
overfitting far less likely than it was. If a future change needs to be
defended rather than merely measured, build a new one from fresh seeds: the
machinery is still here, and the rule is in VALIDATION.md.

## The corpus

62 seeds through **one catalogue** of 90 transforms. It was three catalogues
over three sets of seeds — A for the corpus root, B for `archive/`, C for a
held-out validation set — which meant a tool's score depended on which folder a
photograph happened to be filed in. Now every seed faces every transformation.

**Every transformation with an amount draws it per seed.** The old catalogues
put one fixed number through every image: `scale_50` was exactly 0.50 for all
38 of them, so its 38 measurements were 38 samples of a *single point* on the
scale axis, and a threshold could settle just past a value it would never be
asked to straddle. Now `scale_small` runs from 0.09 to 0.27, `jpeg_low` from
quality 7 to 26, `rotate_small` from 0.6 to 9 degrees. One entry covers what
used to take four, which is why 90 transforms replace 196 and still test more
of every axis.

Two things keep that honest, and both must survive any edit here:

- **It is still deterministic.** The amount comes from `seed_key_of`, a hash of
  the seed's own 16x16 grey thumbnail. Same bytes on every regeneration; no
  call-time randomness.
- **The ground truth cannot drift from the pixels.** Where an amount changes
  what survives — a crop rectangle, a rotation angle, how much of a host canvas
  the photograph fills — the transform *and* its region or coverage come out of
  one factory over one key (`_crop_jit`, `_rotate_jit`, `_scales`). Check it
  the way it was checked when it was written: crop each original by the region
  the manifest records and correlate against the derived file. It is 1.000.

Regenerate with `cd benchmark/corpus && ../../vendor/venv/bin/python
make_variants.py --clean` (deterministic: same bytes, same names). Add an
image anywhere under the corpus to extend it; seeds are found recursively.

One known failure: `strip_metadata` on an AVIF seed, where exiftool produces a
file no decoder opens. The generator deletes it rather than leaving an
unreadable file the manifest does not mention.

## Ground truth: generated, never labelled

**This is the most important decision in the project and it was reached the
hard way.** An earlier version bolted on 9,285 found butterfly photographs to
supply hard negatives, which meant precision needed hand-labelling. On a
single-species corpus that is not a judgement a person can make honestly — 1,008
labelled pairs yielded 9 positives and a lot of doubt, and the user's verdict
was that it was measuring their own bias. All of that machinery (pooling,
sampling, the labelling app) was deleted.

Generated ground truth is exact in both directions and needs nobody:

- **positives** — same seed, regions say SAME or CROP (232,880)
- **negatives** — different seeds, so they were never the same photograph
  (~15.65 M); plus same-seed pairs with disjoint regions or a different
  scramble (16,561)
- **excluded** — PARTIAL overlaps (4,089)

So there is **one F1**. There always was one, but it used to span two
catalogues over two sets of seeds; now it spans one catalogue over all of them.
`exif_rot90` used to be excluded as well — it had two defensible right answers
— and no longer exists in the catalogue.

Negatives are harder than "different photographs" sounds: several seeds are
deliberate near-misses — two Excel screenshots sharing all their UI chrome,
three B&W photos, three panoramas, two products on white.

### How a pair's relation is decided

Each derived file records `region` (rectangle of the original that survives),
`coverage` (how much of the *derived* file is original content), and `scramble`
(a key naming any rearrangement). `pair_relation` applies them in order:

1. **Different scramble keys -> DIFFERENT**, whatever the regions say. The only
   rule that overrides a perfect region match. `tile_shuffle_2x2` has the
   original's exact histogram and is not the original.
2. **Region** -> SAME at IoU >= 0.95, CROP when one contains >= 95% of the
   other, DIFFERENT when disjoint, PARTIAL otherwise.
3. **Coverage** downgrades SAME to CROP when one side is under 90% of the
   other's. Otherwise "the image" and "a poster with the image on it" would be
   the same file.

## How img-fp works, and why each piece is there

`README.md` is the readable version. The parts that are easy to get wrong:

- **Local features, not a global descriptor.** This is the whole reason
  containment works. Do not add a global-embedding shortcut in front of the
  index without checking `collage_2x2` and `embed_quarter` afterwards.
- **Retrieval scores containment, not similarity.** `InvertedFile::query`
  normalises by the *query's* idf mass, so a small crop scores near 1 against
  its parent. Switching to cosine drops recall by about 5 points; it was tried.
- **A claim is a transform plus pixel agreement**, never a score over a
  threshold. That is what rejects `tile_shuffle`, where every pixel of the
  original is present in the wrong places.
- **Flat blocks abstain** in the pixel check, neither agreeing nor
  disagreeing. Counting them as agreement let a thumbnail matched into a tenth
  of a large image — where the large side is a smear — score 0.9.
- **The blocks that do not abstain are averaged, not counted.** `blk` is the
  mean of |r| over them. It used to be the *fraction* of them whose |r| cleared
  0.5, which takes two numbers to say one thing and throws away the difference
  between a block that just missed and one that matched nothing. Averaging
  removed the 0.5 and is worth 0.5 points of recall on its own, and it is what
  made `PROP_NCC` redundant — see *Parameters*.
- **The pixel check reads both sides through a mip pyramid** (`Thumb::lod`),
  each at the footprint one comparison sample covers in *that* thumbnail. It
  used to take a plain bilinear tap from both, which samples whichever side is
  finer far below its Nyquist rate; an aliased view does not correlate with a
  properly filtered one, so the check was reporting disagreement for a
  difference its own sampling had introduced. Fixing it was worth 2.1 points of
  recall, and it is why two fitted constants could then be deleted: a floor on
  comparable blocks and the per-octave inlier surcharge both existed to
  distrust comparisons across a large scale gap, and that distrust was earned
  by the sampling bug rather than by the geometry. Do not reintroduce a plain
  tap here.
- **An anchor's correspondences must bracket the middle of what it claims**
  (`verify::encloses_centre`). A transform interpolates inside the bounding box
  of its inliers and extrapolates outside it. Two different photographs laid
  out on the same page furniture match along the furniture, and the fitted
  transform then claims the whole page — and with it the photograph — on
  evidence that never touched it. Seventeen such anchors caused every
  cross-family error the tool made; requiring enclosure took wrong merges from
  four to two. The middle is taken over the *keypoints* inside the claimed
  region, not over its area, because a building under a clear sky has nothing
  to match in its top half; measuring against the frame's centre instead costs
  13 of the 50 perfect transformations. It is asked of both frames and passes
  on either, since a photograph inside a slide legitimately has all its
  evidence in one corner of the slide.
- **Propagation composes transforms and re-checks them.** It is not closure
  expansion: every propagated pair is tested against the pixels. It is worth
  about 1.5 points of F1 and costs under a second. The tree is breadth-first
  from the best-connected member; growing it best-edge-first was tried and is
  worse, because short paths matter more than strong links.
- **Three acceptance tests** (`verify::Policy`): `anchor` decides clustering,
  `propagated` judges composed transforms on pixels alone, `corroborated`
  applies only inside an existing cluster. Collapsing them into one threshold
  costs either 5 points of recall or 3% of precision. The three now differ
  only in *which* of the shared bars apply, not in their values: `propagated`
  is the anchor rule without the inlier count (no features vouch for it) and
  without the enclosure test (it claims nothing new); `corroborated` is the
  anchor rule with `CLUSTER_SLACK_*` off it. The two numbers that were the
  propagated tier's own — `PROP_NCC` and `PROP_GAP` — are gone.
- **Weak bridges are dropped** (`drop_weak_bridges`). A single match that is
  the only link between two clusters is responsible for every pair they imply.
  Two separate family merges during development cost 354 and 3,002 false pairs
  from one edge each. Every anchor must face this test, including the ones the
  mirrored and inverted pass produces — that was a real bug, and it showed up
  as a precision cliff. The test is now unconditional: a bridge whose far side
  is more than one file is dropped, full stop. Holding strong bridges to a
  higher bar instead, which is what three fitted constants used to do, is worth
  0.2 points of tuning-corpus recall and nothing at all on the held-out one.
- **The vocabulary is sized from the corpus** (`VocabParams::for_corpus`), not
  fixed. Leaf occupancy is what is held constant, because that also fixes the
  average document frequency of a word, which is what idf and the posting-list
  cap are written against. A fixed 65,536 words made img-fp nearly useless on
  small folders; do not put it back.

### What still misses

9,189 pairs. The worst rows, out of 62 seeds: `crop_micro` 41 (a twentieth of
the frame), `embed_tiny` 55, `contact_sheet` 55, `halftone` 56, `crop_strip_top`
57, `scale_small` 58, `tiled_watermark` 59, `wave_vertical` 59. The pattern is
what it has always been — very small crops, heavy downscales, and warps that
break a fitted affine model — though the warps are much less of it than they
were, because the pixel check no longer aliases across a scale gap.

**One row where a competitor still leads**, down from two: PDQ takes `halftone`
58/62 against 56. `rot180` was the other, and is now 62/62 against SSCD's 61 —
it had been a cost of the anchor's enclosure test, and the mean-correlation
block score pays it back. img-fp now leads on every warp row, `perspective_top`
and `keystone_side` included at 62 against SSCD's 61, having been level before;
`barrel_distort` is 61 against 48 and `wave_vertical` 59 against 49.

A note on the traps, since they are now the *whole* FP count and will keep
growing as recall does:
`column_roll_37` slides an image sideways and wraps, leaving 63% of it a rigid
translation of the original, so a crop landing inside that 63% genuinely *is*
present in both files. The corpus calls such pairs DIFFERENT on the scramble
rule and img-fp is caught by it for a defensible reason. Report trap hits and
real errors separately, the way `BASELINE.md` does for SSCD.

### Parameters, and the rule about them

There is one corpus now, and no held-out one, so the discipline has to come
from somewhere else: **a number earns its place by being derived from
something, not by being the value that scored best.** Every amount in the
catalogue varies per seed, so a threshold can no longer be parked just past a
fixed transformation parameter — but it can still be fitted to these 62 seeds,
and with no second corpus nothing will catch that. `benchmark/VALIDATION.md`
records how the parameter surface was cut and what the held-out corpus proved
while it existed; read it before adding a knob back.

Applying that rule took the CLI from 13 result-changing options to 7, the
acceptance policy from 9 fitted numbers to 2, and the bridge test from 3 to 0,
at equal or better F1 every time. Removing a parameter is the cheap experiment;
run it before adding one.

**How to tell whether a number is fitted, with no held-out corpus.** Two
measurements, and they answer different questions. *Sweep it and look at the
shape*: a value sitting on a plateau is not carrying corpus-specific
information, one balanced on a peak is. *Score the same runs on two disjoint
halves of the seeds* (`pair_relation` is per-seed, so a half is exactly the
corpus you would have had with only those seeds) and check that the shape
reproduces. Every knob swept this way reproduced its shape on both halves under
two different splits — so the parameter surface is not balanced on these 62
photographs, which is the thing that was worth knowing.

It also says what the sweeps are *not* evidence for. A value chosen because it
topped the F1 column is still fitted, plateau or no, and the halves cannot
catch that: both halves prefer the same over-fitted value. That is why the
thresholds below moved only where something other than F1 moved with them.

**What a threshold sweep hides: the cliff.** F1 is a poor guide here because
every acceptance threshold is monotone in it over the usable range — looser is
always better — right up to the point where two families merge and thousands of
false pairs arrive at once. So sweep for the *cliff*, not the peak, and quote
the distance to it. Measured on this corpus: `--min-inliers` is clean at 8 and
catastrophic at 7 (8,148 cross-family pairs, 9 merges); `--min-agreement` is
clean at 0.45 and merges at 0.40. The shipped values keep the same margin the
previous build had, which is why neither of them moved to the value that
scored best.

**The fourth pass, and what it removed.** Five numbers, all verified by
deleting each and measuring, then by re-measuring the survivors in the deleted
ones' absence:

- **The Lowe ratio test** (`--ratio 0.9`) and its CLI option. Swept 0.70 to
  1.0 — the whole usable range — it moved F1 by 0.001 and false pairs by single
  digits, on both halves independently. Nothing here decides anything on a
  descriptor distance: an ambiguous correspondence is one vote for a transform
  that must then explain hundreds of others and survive the pixels.
- **`PROP_NCC` (0.7)**, the propagated tier's whole-overlap correlation bar.
  Made redundant by the mean-correlation block score, which says the same thing
  better: with the old counting score, deleting it cost 2 merges and 92
  cross-family pairs; with the new one, deleting it is worth **0.8 points of
  recall and costs nothing at all**.
- **`PROP_GAP` (3.0)**, the octave-gap bar. Inert: identical output at 4, at 6
  and at infinity, and slightly *better* than at 3.
- **The per-block agreement cut (0.5)**, subsumed by averaging |r| instead of
  counting blocks over a bar.
- **The block-coverage fraction (4/5)**, replaced by "the block is wholly
  inside the overlap" — same output, no fraction.

And one value re-derived rather than removed: the geometric inlier tolerance,
**0.03 of the frame diagonal to 0.015**. Every value from 0.010 to 0.025 is
within 0.001 of the same F1 *and* leaves zero cross-family pairs, so the choice
is made on a plateau; 0.03 is off the end of it, and under the new rule it is
not merely worse but **unsafe — 2 wrong merges and 98 cross-family pairs**.
That is the one threshold here whose old value the new rule made dangerous, and
the reason to re-measure every surviving number after a removal rather than
only the removed ones.

**One that looked removable and is not, which is why combinations must be
re-tested.** `max_scale` (16.0) is inert on its own — deleting it changes
nothing measurable. Deleted *together with* the ratio test it costs 15
cross-family pairs, because it is what catches a wrong transform at an extreme
scale ratio once nothing filters ambiguous correspondences. A single-knob sweep
cannot see this. Every removal above was therefore re-measured stacked, not
just alone.

Three things that did *not* survive deletion, and why they stay:

- **Three acceptance tiers.** Folding corroboration in with propagation looks
  right and is wrong: a corroborated pair has features vouching for it, a
  propagated one does not. Two tiers cost 4.2 points of held-out recall. This
  one was **not** re-measured in the v9 pass — it is the older evidence, and
  the tiers now differ only in which bars apply, so re-testing it is the
  obvious next experiment rather than a settled result.
- **The cluster margin** (`CLUSTER_SLACK_*`). Without it, corroborated pairs
  face the anchor bar and held-out recall falls from 93.2% to 90.7%; under the
  new rule, dropping it still costs 0.4 points of this corpus's recall.
- **`encloses_centre` and the bridge test**, neither of which is a number.
  Both were re-measured against the shipped build, and both now matter far
  more than they did: without the enclosure test, **36 wrong merges and 18,334
  cross-family pairs**; without the bridge test, **4 and 3,683**. Under the old
  rule the same two deletions cost 20/384 and 2/50. That is the honest shape of
  this pass — the numbers it removed were doing less work than they looked, and
  what is left standing between the tool and a bad merge is two rules that have
  no magnitude to fit. Raising the bridge test's far-side bound from 2 to 3
  changes nothing (0 cross-family pairs either way), which is what "more than
  one file" being a statement rather than a threshold looks like.

Things that generalise badly and were fixed rather than tuned: the vocabulary
was a fixed 65,536 words at any corpus size, which made img-fp nearly useless
on a small folder (one pair in twenty-eight, on eight images). Depth now
follows the descriptor count.

### Speed and memory, and what has already been tried

(The fourth pass over the parameters, above, is **level on the clock**: six
alternating pairs on the full corpus, cooled to a common ceiling before each
run, at 769 s of CPU against 781 s. The median is **-0.7%** and four of the six
are at or below zero; the +1.6% mean is carried entirely by one +14% pair whose
baseline run was the fastest of the twelve. The pass removes the ratio test's
runner-up bookkeeping and two per-pair acceptance checks, and spends that on
the 4,425 extra pairs it finds, which propagation and corroboration then have
to carry. Do not read a real difference into it either way; the spread is why.)

The pipeline has been gone over three times with a profiler, every time for
**byte-identical output**: the same 223,673 pairs and the same 68 groups, field
for field. That constraint is what makes this list safe to trust — nothing here
traded a pair for a second, and the check is one command (`-o a.json` before,
`-o b.json` after, compare the `pairs` sets). Run it against the **whole**
corpus, not a subset: the third pass had two changes that were identical on the
636-file subset and moved 126 pairs on the full one, and one of the two was a
float multiplication reassociated by accident, which is not a thing careful
reading finds.

The third pass is worth **about 9% of the CPU and 7% of the wall clock**. Six
alternating pairs on the full corpus, die cooled to the same ceiling before
each run, three of them against the build as it finally stands and three
against the same build without its last change:

```
   wall  129.4 -> 118.2   137.8 -> 127.6   137.2 -> 124.2
    cpu    862 ->   781     892 ->   825     891 ->   792
   wall  114.8 -> 124.8   130.8 -> 121.6   128.8 -> 116.5
    cpu    816 ->   821     877 ->   800     864 ->   772
```

Five of the six pairs are 7-11% of CPU. The sixth is level, and its baseline
run is the fastest of the nine baseline runs taken that day — 114.8 s against
a spread of 115 to 138 s for the same binary on the same corpus. That spread
is what the protocol exists for, and it is also why the pairs are quoted
rather than averaged into one number. CPU seconds are the steadier of the two
columns: over all nine runs of each build, 863 s against 788 s.

Under `bench.py`'s own protocol — cold page cache, one run each — the two
builds came out at 96.2 s and 95.1 s, at 2,647 and 2,571 MHz respectively and
with two gigabytes of cold reads in both. Worth remembering before quoting
either number: the *same* binary measured 96 s under `bench.py` and 129-138 s
in the alternating runs an hour later, on a warmer machine.

The two passes before it were measured the same way and came to **163 s and
1,739 MB, then 131 s and 1,178 MB, then 105 s and 875 MB**, each figure from
`bench.py` with the old build re-run in the same session.

**Peak memory is not a property of the build alone**, and those absolute
figures should be read with that in mind. It is the steady state — a few
hundred kilobytes of descriptors and a thumbnail per image, about 650 MB here —
plus however much decoded picture the workers happen to be holding, and that
second term is bounded by `decode_budget`, which is a fraction of what the
machine reports *free*. The same baseline binary on the same corpus measured
875 MB in one session and 1,050 MB in another, because `MemAvailable` differed.
Within a session it still wanders: 861, 925, 947, 989, 1,008, 1,027 and
1,050 MB on seven runs of the same binary, set by which large files happen to
decode together. So a memory change worth
less than 10% cannot be seen at `-j 8` at all, and the only deterministic
reading is `-j 1`, where there is one decode in flight and the walk order
decides everything. Measured there, the third pass is **785 MB to 770 MB**
(803,596 and 804,668 KB against 787,192 and 790,208 KB, two runs each) — the
shared analyses and the three Gaussian planes, and about as much as those two
are worth. At `-j 8` the six runs of each average 977 MB against 914 MB, which
points the same way and proves nothing, given the spread above.

The two figures together are the shape of the thing: what the program *holds*
went down by 15 MB, and what it *peaks at* on eight threads is mostly not that.

**What this machine is limited by, which decides what is worth trying at all.**
One busy core boosts to 3.2 GHz; eight run at 1.27 GHz, and the extraction
phase is only 2.9x faster on eight threads than on one. Under a power cap wall
time follows *energy*, not cycles, and the two respond to different changes.
Removing a **stall** — a float divide, a mispredicted branch, a cache miss the
other hyperthread was glad of — is worth a clean 5% at `-j 1` and *nothing* at
`-j 8`, where the sibling thread simply takes the slot. What moves the
eight-thread clock is removing **work**: bytes not moved, instructions not
issued. So measure a change at `-j 1` to learn whether it is faster, and at
`-j 8` to learn whether it matters; several of the entries below were worth
half of what the single-threaded number promised, and the ones that survived
are the ones that move less memory.

Where the time goes now: decode ~35%, feature extraction ~45%, everything
after it ~20%. Within extraction the descriptor is the largest single item, and
within decode it is JPEG — `--features prof` prints the whole table at the end
of a run, which is how the third pass below was aimed.

That table also settles a question worth not re-asking: the corpus's five
exotic formats are *not* where the decode time is. WebP, TIFF, JXL and the
libheif formats together are 431 of 5,638 files and about 80 CPU-seconds
against JPEG's 171 — four times the cost per file and a twelfth of the total.
A faster HEIC path would be worth 3% of the run, and there is no faster JPEG
path to reach for; see the DCT-scaled entry under *Tried and rejected*.

What was worth doing in the **third** pass. Every entry here removes
instructions from a loop that was already vectorised or already tight, which
is why they are small individually and worth 10% of the CPU together. The four
that were tried and thrown away are at the end of *Tried and rejected*, and
three of the four are restructurings that looked obviously better:

- **The descriptor turned three floats into integers the expensive way.**
  `rbin`, `cbin` and `obin` have just been tested into ranges a few units
  wide, and the orientation bin is then folded into 0..8 by two branches. Rust
  spells `as i32` as a *saturating* conversion — a compare and a conditional
  move around the truncation — and the fold was six instructions where masking
  off the low three bits is one and gives the same answer for every input the
  bin can hold. That is seventeen instructions per sample, and the extractor
  takes some five billion samples over this corpus. With the same treatment of
  the Gaussian weight table's index and a float column counter in place of an
  integer one converted per sample, the descriptor is **18% faster** — the
  largest single item in the run, and it had looked finished.
- **And its row sweep was two jobs in one loop.** Working out where a sample
  falls in the grid and how much gradient it carries there is a short chain of
  multiplies, the same for every sample and nothing a compiler cannot run
  eight at a time; adding those contributions into the histogram is a scatter
  and has to go one sample after another. Interleaved, the scatter held the
  arithmetic to one sample at a time as well. Sweeping sixteen samples for the
  first and then sixteen for the second is another **11%**, and the weight is
  now taken for samples that fall outside the grid too — cheaper than the
  branch that skipped them.
- **The extremum sweep compared each pixel against eight neighbours twice.**
  "At least as large as all eight" is "at least as large as the largest of
  them": seven comparisons instead of eight tests and their seven
  conjunctions, and the same again for the minimum. A difference of two finite
  blurs is finite, so the two forms have no NaN to disagree about.
- **The pixel check located every grid sample from scratch.** The comparison
  grid is walked in A's own frame, so a sample's column in A's thumbnail
  depends only on `ix` and its row only on `iy`: ninety-six clamps and
  truncations per pyramid level rather than two thousand three hundred pairs
  of them. What is left per sample is the four reads and three interpolations
  that actually look at the picture. Watch the associativity when doing this —
  `(x * scale) * f` is not `x * (scale * f)`, and folding the two factors
  moved 126 pairs.
- **The grey reduction's lookup table was costing more than the division it
  saved.** Dividing a three-byte channel sum by three was replaced, in the
  second pass, by a table of the 766 quotients — right for one divide, wrong
  here, because a table lookup is a *gather* and it was the one thing in that
  loop the compiler could not vectorise around. Eight lanes dividing at once
  beat eight lanes waiting on eight scattered loads; the quotient is the same
  float either way, since the table held nothing but this division taken at
  compile time. The alpha blend went the same way: skipping it for opaque
  pixels is a branch per pixel, and the branch cost more than the blend.
- **The geometry stage finished hypotheses that had already lost.** Every
  correspondence proposes a transform and every transform is scored against
  every correspondence, but a wrong transform explains two or three of them
  out of hundreds. Once the correspondences still to be tested cannot carry
  the running total past the best count so far, nothing the rest of them say
  can change what the caller does.
- **The vocabulary descent asked the tree the same question for every
  descriptor.** Which of a node's sixteen children are live, and where their
  centres start, are facts settled when the tree was built; the descent was
  scanning a sixteen-wide slot table twice to rediscover them. And the
  frontier — up to forty-eight entries — was fully sorted to choose the three
  that survive, which is ordering forty-five nodes about to be discarded. A
  scan takes the three instead, and falls back to the sort in the rare case
  where an exact tie across the boundary means the distances do not decide the
  answer at all.
- **The area resampler proved a bounds four times per output pixel** to do two
  multiply-adds, and zeroed its output plane before overwriting every element
  of it.

And for memory:

- **Byte-identical files held a second copy of an analysis they share.** The
  exact pass elects one member of each group and the rest are copied from it —
  and a copy is a megabyte-scale buffer for bytes that are the same bytes.
  They share it now.
- **Gaussian layers were held after the last thing that reads them.** Three of
  the `s + 3` are dead the moment the differences are taken: the octave's own
  base, and the two that exist only to make the top two differences. That was
  eleven full-size planes per worker at the widest point of the pyramid, on
  eight workers at once, for three planes nothing would read again.
- **The working image was copied to become the base of the pyramid**, when the
  blur that makes the base could read it where it lies.
- **A decode's claim on the shared budget did not cover everything the decode
  holds** — not the file's own bytes, and not the float plane the reduction
  writes while the decoder's buffer is still alive, which for a picture
  already near the working size is four bytes a pixel against the decoder's
  three. A budget that under-counts is not a budget.

What was worth doing in the **second** pass, in order of what it returned. The
two largest move fewer bytes rather than fewer instructions, which is what the
paragraph above predicts:

- **The blur wrote its intermediate plane to memory and read it back.** A
  separable blur filters rows and then columns, and the filtered rows were a
  plane of their own: 1.2 MB out to memory and back for each of the twenty
  blurs an image costs. But the column pass of row `y` reads filtered rows
  `y-r ..= y+r` and nothing else — reflection at an edge maps a tap back inside
  that window, never outside it — so a ring of `2r+1` rows, fifty kilobytes
  that stay in cache, holds everything that is ever read. Same taps, same
  order, same floats; the plane is simply gone. With the difference below it,
  7% of the run at eight threads.
- **The difference-of-Gaussians was a pass of its own**, reading two Gaussian
  layers and writing a third. It is taken inside the blur that produces the
  upper layer now, where the output row is still in registers and the lower
  one was read a few rows ago and is still in cache.
- **The extremum sweep branched on every pixel of the pyramid.** Two thirds of
  a difference layer clears the contrast threshold and about a tenth of that
  survives its own row, so the test at the top of the sweep was a coin toss
  taken hundreds of millions of times per corpus. The row's nine comparisons
  are settled as arithmetic now, eight pixels to an instruction, and only the
  survivors take a branch.
- **The grey reduction divided by three once per source pixel** — seven and a
  half billion float divisions over this corpus, for a quantity with 766
  possible values. The table is not an approximation of the division: the entry
  *is* the division, taken once at compile time.
- **Byte-identical files were decoded and described twice.** The exact pass has
  already grouped the files whose bytes hash the same, and the analysis depends
  on nothing else; 298 of these 5,638 files are a copy of another. Each group
  elects one member and the rest are copied from it — which asserts nothing the
  run does not assert anyway, since those files are already claimed as
  duplicates of each other.
- **Gradient magnitude and orientation were two planes a page apart**, and
  every reader wants both halves of the same pixel. Interleaved, they are one
  stream instead of two.
- **`fastAtan2` branched twice per pixel**, which stopped the gradient loop
  vectorising. Both arms always divided the smaller magnitude by the larger and
  ran the same series on it; written as selects over one polynomial, the loop
  does eight pixels at a time, term for term identical.
- **The descriptor proved its bounds eight times per sample.** The trilinear
  spread writes eight corners, and `rbin`, `cbin` and `o0i` have just been
  tested into ranges that put every one of them inside the histogram. The
  argument is in the code beside the `unsafe`.

And for peak memory, which was set by an accident of directory order:

- **The decoders share a budget.** A 44-megapixel photograph is 133 MB of RGB
  and the analysis keeps 1.2 MB of it, so with eight workers reaching for one
  at once the peak of a run depended on how many large files happened to be
  adjacent in the walk. Claims are served in the order they are made, so a
  large one cannot be starved by small ones slipping past, and a file larger
  than the whole budget still decodes — alone. Nothing about the output can
  depend on it.
- **Two allocator arenas instead of sixty-four.** glibc gives a process eight
  arenas per core and lets each keep what it has freed, which suits a program
  allocating small blocks in tight loops. This one takes a handful of very
  large buffers per image, and spread over sixty-four arenas the freed decode
  buffers and scale spaces were held apart from each other and from the
  descriptors they were interleaved with — 220 MB of holes, measured as the
  gap between what the run held and what it was using. The small allocations
  that might contend for two arenas are served from each thread's own cache
  and never take the lock; the verification stage, which allocates per pair,
  measures the same either way.
- **The heap is trimmed at the two phase boundaries**, where a phase's worth of
  very large buffers has just died. (This was measured once before and judged
  worthless, and it was: the peak then stood in the middle of the analysis
  phase. With the decode budget holding that down, what is left is the plateau
  this releases.)
- **The vocabulary, the inverted file, the word lists and every descriptor are
  dropped once the last match has been made.** Propagation, corroboration and
  assembly work from thumbnails, frame sizes and verdicts already taken, and
  the descriptors are the largest thing the run holds. They were being held to
  the end.

What was worth doing in the **first** pass, in order of what it returned:

- **The vocabulary was mostly empty air.** A five-level tree has `16^5` leaves
  at 512 bytes of centre each — 536 MB — and k-means handed back a full
  sixteen-wide block from every one of the 65,536 parents of the deepest level,
  which average two or three samples apiece. Storing only live centres took
  that to 109 MB, and is most of the memory saving.
- **Distances were measured one centre at a time.** Both quantisation and
  k-means walked a descriptor against a node's sixteen children in turn, and
  each of those is a chain of 128 dependent adds: one of the machine's several
  adders busy. A parent's centres are now stored dimension-major, so all
  sixteen sums run at once. Each sum is still taken over the dimensions in
  order, so every distance is the same float to the bit. Quantisation went from
  24 s to 7 s.
- **The geometric fit read keypoints through two indirections.** Every
  correspondence proposes a transform and every transform is scored against
  every correspondence, so those four coordinates are read `n` times each.
  Copying them into four flat arrays first is 4x on that stage (107 CPU-s to
  27).
- **The pixel check ran on verdicts that had already lost.** It is the most
  expensive thing done per pair, and a third of the pairs reaching it had
  already failed on inliers or overlap. It now runs only when some tier could
  still accept the pair: 405k checks became 265k.
- **The descriptor swept a square four times the area it can use.** Its search
  window is 7.07 hist-widths across; the rotated grid that can actually receive
  a sample is 4. Solving the same inequalities for the row's span instead of
  testing every pixel keeps the identical set of samples in the identical
  order.
- **Describing did not stop when the ranking was already decided.** Candidates
  arrive in response order and `retain_best` keeps the highest `max_features`
  responses, so once that many descriptors exist and the next candidate is
  weaker than the weakest of them, nothing later can displace one. A textured
  image was describing three keypoints for every one it kept.
- **Buffers allocated zeroed and then completely overwritten** — the blur's
  intermediate and output, `halve`, `upsample`, the grey reduction.
  `reduce_to_gray` also had a runtime channel stride and a runtime divisor in
  its innermost loop, which is enough to stop it vectorising over a hundred
  megapixels a minute.
- Smaller: the word lists were held twice over, the inverted file was a `Vec`
  per word (a million allocations for a million words), the cache file was
  assembled in memory before being written, propagation kept every rejected
  hypothesis for a `--dump` nobody asked for, and the release profile had
  neither LTO nor a single codegen unit.

**Tried and rejected. Do not retry without new evidence.** Each was measured by
alternating the two builds on the same corpus, which is the only protocol that
works on this laptop:

- *A per-thread pool for the scale-space planes*, to stop the churn of
  megabyte buffers per image. No measurable change in time, and 70 MB more
  peak. The allocator was already handling it.
- *A branchless extremum test* — all 26 neighbour comparisons for eight pixels
  at a time rather than the early-exiting scalar one. **24% slower overall.**
  The early exits predict well, and 26 comparisons cost more than the branch
  they save.
- *Rewriting the blur's horizontal pass* as one whole-row pass per kernel tap:
  60% slower than the eight-column register blocking already there. The same
  blocking applied to the *vertical* pass: 25% slower than the plain row loop.
  Both directions were tried; what is in the file won both times.
- *Filling the gradient planes row by row* to avoid allocating them zeroed:
  2.5x slower. An indexed write loop vectorises and a `push` loop does not.
- *`malloc_trim` at the phase boundary.* Hands back ~430 MB at that moment and
  moves the reported peak by nothing, because the peak is not there.
- *A DCT-scaled JPEG path* (decode at 1/2 or 1/4 and skip the box reduction).
  Not attempted, and the arithmetic is why: two thirds of the corpus's JPEG
  pixels could come from a half-scale decode, but entropy decoding is
  unaffected and is most of the cost, so the ceiling is about 4% of the run —
  against a second JPEG implementation to maintain and pixels that would no
  longer be bit-identical, which is the property that makes everything else
  here checkable.
- *Pinning glibc's mmap threshold* just above the working image, so that every
  decode buffer is its own mapping and goes back to the kernel when it is
  freed. It works — peak 754 MB to 379 MB on the `Desktop` subset — and costs
  5-10% of the clock, because the kernel zeroes every page it hands back and
  that is 22 GB of zeroing over this corpus. Returning pages and reusing pages
  is a real trade, not an oversight; the arena limit takes most of the memory
  without the syscalls.
- *Quantising the corpus level by level* rather than descriptor by descriptor:
  the whole batch's frontier sorted by node, so a block of centres is read once
  for the seventy descriptors that reach it instead of once each. The
  arithmetic says 60 GB of memory traffic; the clock says nothing changed. Two
  reasons, and both are worth knowing before trying it again: one image's
  descriptors land on a few hundred nodes rather than thousands, so a
  per-image descent already gets most of that reuse out of cache — and
  batching reads each descriptor fifteen times where the one-at-a-time descent
  reads it once and keeps it in L1.
- *Abandoning a descriptor distance once it cannot beat the runner-up*, which
  half a descriptor usually settles, and a descriptor is two cache lines. No
  measurable change: most query keypoints have too few candidates for a
  runner-up to exist at all, and the two images' descriptor blocks are 76 KB
  apiece and already in L2 by the second pair that uses them.
- *Filtering the blur in vertical strips*, so that the ring of filtered rows —
  fifty kilobytes at full width, which is larger than any L1 data cache, and
  read whole for every output row — would fit in one. **29% slower.** The
  strips re-read the source plane once each and write the output in columns,
  and that costs more than the second-level hits it saves. This is the third
  time the blur has been asked to work on part of a row at a time and the
  third time the answer has been no; the plain full-width row loop wins.
- *Folding four kernel taps into one pass over the blur's accumulator row*, so
  that the running total is loaded and stored once per group rather than once
  per tap. Byte-identical and free, and worth nothing: what the column pass
  waits on is the ring, not the accumulator, which was in L1 all along.
- *Writing the descriptor's eight histogram corners as four two-wide
  read-modify-writes.* The corners are four adjacent pairs, so this looks like
  half the stores for nothing — but the compiler had already paired them, and
  saying so by hand with unaligned two-element reads and writes was slightly
  worse.
- *Sixteen and thirty-two outputs at a time in the blur's horizontal pass*
  rather than eight, for more independent accumulators per tap loop. Both
  lose: 6.4 ms for the five blurs of an octave at eight, 7.0 at sixteen, 7.1
  at thirty-two. Eight floats is one vector register and the tap loop wants
  the rest of them for the taps.
- *Handing the area resample the grey values a row at a time*, so that the
  full-resolution float plane between the two — twelve megabytes for a
  four-megapixel photograph, written once and read once — never exists. **25%
  slower on RGB**, and this is the surprise: the resample reads that plane
  strictly sequentially, which the prefetcher serves for nothing, so the
  "saved" traffic was never being waited on, while a loop boundary per row of
  the source is real. It *is* faster for greyscale, where the conversion is
  trivial and the plane is all there is — which is not what a corpus of
  photographs looks like.
- *Measuring a frontier's parents together in the vocabulary descent.* Each
  child's distance is a chain of 128 dependent additions, so three parents'
  chains ought to interleave and fill the adder's latency. **2.6x slower** —
  three sets of sixteen running sums is forty-eight floats of accumulator and
  the register file gives out. Merely gathering the three parents' blocks into
  an array of slices first, without changing the arithmetic at all, was
  already **2x slower** than reading each parent's block where it is. The
  measurement is `cargo test --release -- --ignored --nocapture quantise`,
  which runs the descent alone on one core in a few seconds.

**How to measure any of this.** Wall time and CPU seconds on this laptop swing
25% with the die temperature, and `time` does not report the clock. Build both
versions, run them alternately with a cooldown between, and compare the pairs;
a single before/after is worthless. So is `cpu_seconds x MHz` as a
clock-independent "work" figure — it looks principled and the thermal governor
makes it non-linear enough to reverse a result. The subset
`derived/Desktop` (636 files, ~13 s) is enough to compare extraction changes,
and `--cache` isolates everything after it.

Three tools, in increasing isolation, and it is worth reaching for the most
isolated one that can answer the question:

- **`--features prof`** adds a per-stage CPU-second table to the end of a run:
  decode by format, each phase of the extractor, each phase of the matcher.
  Zero-cost without the feature — the `timed!` macro expands to its argument —
  and it is what says *where* to look. Stages nest where the code nests, so
  `decode:jpeg` contains `decode:codec` and `decode:reduce` contains
  `decode:fit`; do not add the column up.
- **`cargo test --release -- --ignored --nocapture`** runs three benchmarks of
  the inner loops on synthetic data, on one core, in a few seconds each:
  `kernel_timings` (blur, extract, descriptor, orientation histogram,
  gradient), `reduce_timings` (the grey reduction at each channel layout and
  box factor, and the area resampler) and `quantise_timings` (the vocabulary
  descent). They report the *fastest* of nine runs, because the slow ones
  belong to the machine. Use these to decide whether a change is worth a
  corpus run at all — three of the four rejections above were settled here in
  a minute apiece.
- **`objdump -d`** on the release binary, after an `#[inline(never)]`, when
  the question is "did that actually vectorise". `perf` does not work on this
  box (`perf_event_paranoid` is 4) and neither does attaching a profiler
  (`ptrace_scope` is 1), so the instruction mix in the listing is the only
  direct evidence available.

**Peak memory cannot be measured in one run at `-j 8`** — see the paragraph on
it above. Use `-j 1`, which is deterministic, to see whether a change reduced
what the program holds, and take several `-j 8` runs to see whether it matters.
Note also that making *extraction* faster raises the eight-thread peak on its
own, because each worker then spends a larger fraction of its time holding a
decode buffer; that is what the decode budget is for, and why its claims have
to cover everything a decode holds.

### Tuning discipline

`--dump` writes every verdict considered, accepted or not, as CSV. Fit
thresholds against that offline instead of re-running the tool per guess. Use
`--cache` while tuning the matching stages: on the 5,638-image corpus a cold
run is ~130 s and a cached one ~30 s, and the cache is keyed on the extraction
settings so changing `--work-size` or `--features` invalidates it correctly.

Note that timings taken this way are warm-cache and run about 40% faster than
`bench.py`'s cold-cache figures. Compare tuning runs with each other, never
with `BASELINE.md`.

**Sweeping an internal constant without a rebuild per value.** The fourth pass
swept some thirty of them. A rebuild is 70 s against a cached run's 30 s, so
temporarily reading each constant from an environment variable — defaulting to
the value in the file — makes the sweep three times cheaper and keeps one
binary across the whole grid. Hard-code the conclusion and delete the
scaffolding afterwards; then **re-measure the final build**, because an
environment override is not always exactly what deleting the code does.
(`--ratio 1.0` and a deleted ratio test differ on exact-distance ties. They
agreed here, and that was worth confirming rather than assuming.)

**Two things every candidate must face before it is believed**, both cheap and
both of which caught something real in this pass:

- **Score it on two disjoint halves of the seeds**, not just the whole corpus.
  A pair belongs to a half when both its files descend from a seed in that
  half, which makes the half exactly the corpus those seeds would have given.
  It needs no extra runs — it re-scores the JSON already written.
- **Split its false pairs into traps and cross-family errors.** They are not
  the same mistake and the totals hide the difference: the change shipped here
  *raised* the false-pair count from 832 to 1,088 while taking cross-family
  errors from 2 to 0, because every one of the new ones is a `column_roll`
  trap. A rule that only watches total FP would have rejected it.

Measured trade-offs, so they need not be rediscovered (the seconds are from the
old corpus and the pre-optimisation build, so read them as ratios): `--work-size`
448 gives F1 0.975 at 80 s, 640 gives 0.980 at 86 s, 768 gives 0.976 at 147 s.
`--features` 900 is *worse* than 600. Candidate breadth (`-k`) is on a plateau,
not a peak — 200 gives byte-for-byte the same F1, precision and false-positive
count as 150 on both corpora — so 150 is safely past the knee rather than
balanced on it.

## Conventions

**Canonical runner output.** Every runner emits:

```json
{"tool": "...", "config": {...}, "runtime_seconds": 0.0,
 "groups": [["/path/a", "/path/b"], ...],
 "pairs":  [{"a": "/path/a", "b": "/path/b"}, ...]}
```

**`pairs` wins over `groups` wherever both exist.** Groups are the union-find
closure of the pairs; expanding a closure back into pairs credits a tool with
every match its chains imply rather than the ones it made. This inflated SSCD
from 9,172 claims to 238,771 once, and roughly 7,000 of a 16,267-pair labelling
queue were artifacts of it. Any new consumer of these files must follow the
same rule.

**Two venvs.** `vendor/venv` is the general one. `vendor/venv-imagededup` has
torch, so **SSCD and imagededup must run under it**; it also now has
`pillow-heif` and `pillow-jxl-plugin`. `vendor/venv-imgdupes` backs the
imgdupes binary. Getting this wrong looks like `ModuleNotFoundError: torch`.

## Benchmarking discipline

`bench.py` exists because casual timing on this machine is worthless.

- **One tool at a time, never two.** 60 s minimum cooldown, then a wait until
  the die returns to measured idle + 3 C.
- **This laptop thermally throttles.** Ryzen 7 3700U, 8 threads, idles at
  ~63 C and hits 77-92 C under load, with clocks swinging 1.2-3.0 GHz. Absolute
  timings are not comparable to a desktop's; relative ones under identical
  conditions are. Never use a fixed absolute temperature ceiling — measure the
  idle baseline first, or the cooldown silently times out every time.
- **Clear both caches.** Tool caches per tool (czkawka `-H`, imgdupes
  `--no-cache`, dupeGuru fresh temp db, SSCD denied `--embeddings`), and the
  kernel page cache with `posix_fadvise(POSIX_FADV_DONTNEED)` over the corpus.
  No passwordless sudo here, so `drop_caches` is unavailable — fadvise is
  unprivileged and better anyway, evicting only the corpus. Without it the
  1.1 GB corpus stays resident on this 5.7 GB machine and every tool after the
  first reads from RAM.
- **Peak memory must be summed over the process tree.** `ru_maxrss` reports
  only the largest single child. Report **PSS** (shared pages split
  proportionally); RSS double-counts. difPy is 1274 MB PSS vs 3851 MB RSS.
- `bench.py --only <tool>` reruns one tool and **merges** into `metrics.json`.

## Gotchas already paid for

- **czkawka** exits **11** when it finds duplicates (`-W` to suppress), and its
  `--minimal-file-size` default of 16384 bytes silently skips small images
  (`-m 1`). Its defaults cost it 15x F1 versus `-z Lanczos3
  --geometric-invariance mirror-flip-rotate90`.
- **findimagedupes** output is space-separated with no NUL mode. The corpus has
  `derived/WhatsApp Images/`, so it needs a space-free farm — **hard links, not
  symlinks**, because it resolves symlinks and prints the target. Hard links
  require the same filesystem, hence `~/.cache`.
- **imagededup** has no CLI, is not recursive, and keys results by *basename*.
  The runner builds a flat symlink farm with unique names.
- **SSCD**'s dataset must return a placeholder tensor of the batch's shape for
  unreadable files; a 1x1 placeholder kills the whole run in `default_collate`.
- **difPy** has no console script; invoke
  `vendor/venv/lib/python3.12/site-packages/difPy/dif.py` directly. Its MSE
  knob has no usable permissive setting — `-s 200` already merges an entire
  corpus into one group.
- **dupeGuru**'s `Photo` class is Qt-based; the runner supplies a PIL-backed
  subclass using the `_block` C extension's `getblocks2`.
- **A tool that cannot decode a format is indistinguishable from one that
  failed to match.** imagededup-CNN scores 0/8 on AVIF, HEIC and JXL purely
  because it cannot open them.

## Working style

- The user reads results critically and pushes back on anything that smells
  like a measurement artifact; they were right about the 16k-pair pool. Check
  numbers against a second method before reporting them.
- Do not start long tool runs without being asked. Ask, or wait to be told.
- Report failures as results, not as things to hide: findimagedupes refusing
  the corpus and SSCD crashing are both in `BASELINE.md`.
