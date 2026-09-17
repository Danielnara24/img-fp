# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from — read it before
changing how anything is scored.

**Status: the tool exists and beats every measured competitor by a wide
margin.** On the current corpus — 5,638 files, 62 seeds, 90 transformations,
every amount drawn per seed — F1 **0.969** at 99.6% precision and 94.4% recall,
against SSCD's 0.762 / 92.6% / 64.8%, in 105 s against SSCD's 1,949 s.

**50 of 87 transformations are handled perfectly** (all 62 seeds found across
the whole range of the amount). Every other tool manages **zero**.

Precision holds where it matters: of 832 false pairs, 830 are the deliberate
rearrangement traps, leaving **2 wrong pairs in 220,657 proposals** against
15.65 million chances to be wrong. Those two are two wrong cluster merges,
which is the number to watch: a merge's cost is every pair the two families
imply, so it grows with the corpus while a lone bad pair does not.

`benchmark/BASELINE.md` holds the competition's numbers,
`benchmark/VALIDATION.md` records the held-out experiment that shaped the
parameter surface, `README.md` explains the design.

What is left is not accuracy-critical: decode is ~30% of the runtime and has no
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
  out/v4/             the baseline run: metrics.json, baseline.json, *.json
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
  costs either 5 points of recall or 3% of precision.
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

13,055 pairs. The worst rows, out of 62 seeds: `crop_micro` 40 (a twentieth of
the frame), `contact_sheet` 54, `embed_tiny` 54, `halftone` 56, `crop_strip_top`
57, `wave_vertical` 57, `barrel_distort` 58, `picture_in_picture` 58. The
pattern is what it has always been — very small crops, heavy downscales, and
warps that break a fitted affine model — though the warps are much less of it
than they were, because the pixel check no longer aliases across a scale gap.

Two rows where a competitor still leads: PDQ takes `halftone` 58/62 against 56,
and SSCD `rot180` 61 against 60. `rot180` is a cost of the anchor's
enclosure test and is worth revisiting if it grows. img-fp no longer trails on
the warps — `perspective_top` and `keystone_side` are 61/62, level with SSCD,
and it leads on `barrel_distort` 58 against 48 and `wave_vertical` 57 against
49.

A note on the traps, since they dominate both FP counts and will keep doing so:
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

Applying that rule took the CLI from 13 result-changing options to 8, the
acceptance policy from 9 fitted numbers to 2, and the bridge test from 3 to 0,
at equal F1 on the corpus the numbers came from and better F1 on one they had
never seen. Removing a parameter is the cheap experiment; run it before adding
one.

The acceptance policy is now down to **zero** fitted numbers beyond the CLI's
own defaults and `CLUSTER_SLACK_*`: `MIN_BLOCKS` and `inliers_per_octave` were
both deleted once the pixel check stopped aliasing, each verified by deleting
it and measuring. Neither cost anything; the surcharge's removal was worth
0.4 points of recall on its own. What replaced them is not a threshold —
`encloses_centre` asks about position, not degree, so it has no magnitude to
fit.

Two things that did *not* survive deletion, and why they stay — both measured
against the held-out corpus while it still existed:

- **Three acceptance tiers.** Folding corroboration in with propagation looks
  right and is wrong: a corroborated pair has features vouching for it, a
  propagated one does not. Two tiers cost 4.2 points of held-out recall.
- **The cluster margin** (`CLUSTER_SLACK_*`). Without it, corroborated pairs
  face the anchor bar and held-out recall falls from 93.2% to 90.7%.

Things that generalise badly and were fixed rather than tuned: the vocabulary
was a fixed 65,536 words at any corpus size, which made img-fp nearly useless
on a small folder (one pair in twenty-eight, on eight images). Depth now
follows the descriptor count.

### Speed and memory, and what has already been tried

The pipeline has been gone over twice with a profiler, both times for
**byte-identical output**: the same 223,673 pairs and the same 68 groups, field
for field. That constraint is what makes this list safe to trust — nothing here
traded a pair for a second, and the check is one command (`-o a.json` before,
`-o b.json` after, compare the `pairs` sets). Measured by `bench.py` with the
old build re-run in the same session: **163 s and 1,739 MB, then 131 s and
1,178 MB, now 105 s and 875 MB.**

Read that clock figure with the MHz column beside it, as `BASELINE.md` insists.
Three alternating pairs on the full corpus gave 131.4 s to 107.8 s, 137.2 s to
120.2 s and 136.2 s to 105.5 s; only the middle pair had the two builds at the
same mean clock (2199 against 2245 MHz), and it is the smallest of the three at
12%. The controlled measurement is the 636-file subset, four alternating pairs
with the die cooled to idle + 3 C before each: **12.41 s to 11.13 s of wall and
80.3 s to 72.1 s of CPU**, about 10%. The full corpus gains more than that
because finishing sooner also means running cooler for longer — real on this
laptop, and not a claim about the work removed. Peak memory needs no such
caveat: 1,178 MB to 875 MB, and the new figure is steady across runs (866-886)
where the old one wandered between 1,114 and 1,516 MB depending on which large
files happened to decode together.

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

Where the time goes now: decode ~30%, feature extraction ~50%, everything
after it ~20%. Within extraction the descriptor is the largest single item and
is at its practical limit.

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

**How to measure any of this.** Wall time and CPU seconds on this laptop swing
25% with the die temperature, and `time` does not report the clock. Build both
versions, run them alternately with a cooldown between, and compare the pairs;
a single before/after is worthless. So is `cpu_seconds x MHz` as a
clock-independent "work" figure — it looks principled and the thermal governor
makes it non-linear enough to reverse a result. The subset
`derived/Desktop` (636 files, ~13 s) is enough to compare extraction changes,
and `--cache` isolates everything after it.

### Tuning discipline

`--dump` writes every verdict considered, accepted or not, as CSV. Fit
thresholds against that offline instead of re-running the tool per guess. Use
`--cache` while tuning the matching stages: on the 5,638-image corpus a cold
run is ~130 s and a cached one ~30 s, and the cache is keyed on the extraction
settings so changing `--work-size` or `--features` invalidates it correctly.

Note that timings taken this way are warm-cache and run about 40% faster than
`bench.py`'s cold-cache figures. Compare tuning runs with each other, never
with `BASELINE.md`.

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
