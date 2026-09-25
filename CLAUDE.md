# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from — read it before
changing how anything is scored.

**Status: the tool exists and beats every measured competitor by a wide
margin.** On the current corpus — 5,638 files, 62 seeds, 90 transformations,
every amount drawn per seed — F1 **0.955** at 99.6% precision and 91.7% recall
at the shipped default, against SSCD's 0.762 / 92.6% / 64.8%, and under a
minute against SSCD's 1,949 s. At `--work-size 640` it is F1 **0.978** at
99.5% / 96.1%, for 44% more wall clock and 61% more CPU.

**The default sits below the accuracy knee on purpose, and 640 is the knee.**
`--work-size` scales the first four fifths of the pipeline, so it is the only
knob really connected to the clock, and what it trades is recall for time:
384 gives up **4.4 points of recall** and buys **30% of the wall, 38% of the
CPU and 18% of the peak memory**. Precision is not part of that trade — it
moves 0.07 points the *right* way, and every false pair at either size is a
deliberate rearrangement trap. But the recall it gives up is not spread
evenly, and it comes out of the one capability nothing else in the field has
at all: a photograph embedded in a bigger canvas. `embed_tiny` goes 55/62 to
**27/62** and `contact_sheet` 55 to **29**, because shrinking the canvas to 384
shrinks the photograph inside it below what the detector can describe. Fifteen
transformations leave the perfect column, and nine of the fifteen are
containment rows. **Anyone who cares about that case should pass
`--work-size 640`**; see *Measured trade-offs* for the full table.

(**The cost figures are the soft ones here, and the accuracy figures are not.**
Accuracy is a property of the build and the work size: F1 0.955 at the default
and 0.978 at 640 are both reproducible to the pair. Wall clock is a property of
the session — this laptop's idle temperature alone moves it 25% — and the
numbers people want to compare were taken in five different sessions: the
eleven competitors in `out/v5`, img-fp at 105 s in `out/v8`, the parameter pass
at 122 s in `out/v9`, the fifth optimisation pass at 103 s in `out/v10`, all
four at `--work-size 640`, and the default's own row in `out/v11`. Only
like-for-like pairs within one session mean anything: the work-size table in
*Measured trade-offs* is one such session and is the right place to read the
default against 640, and the optimisation passes were each measured by
alternating cold runs of the two builds. So quote "under a minute, against
SSCD's half hour" and do not put weight on the third significant figure. See
*Speed and memory*.

**Every parameter sweep and every figure in *Speed and memory* was measured at
`--work-size 640`**, which was the default when they were taken, and they are
left as measured rather than half-rewritten: they are measurements of
particular changes against particular builds, and re-running two hundred lines
of them at a new work size would not make any of them more true. The four
options were re-swept at 384 — the tables are under *Measured trade-offs* — and
none of them moves. Where the new default changes a *conclusion* rather than a
number there is exactly one place, and it is the `--min-pixel-correlation`
cliff, which does not exist at 384.)

**47 of 87 transformations are handled perfectly** at the default (every seed
found across the whole range of the amount), and **60 of 87** at
`--work-size 640`. Every other tool manages **zero** at any setting.

Precision holds where it matters: of 871 false pairs at the default, **all 871
are the deliberate rearrangement traps**, leaving **no wrong pair at all** in
214,373 proposals against 15.65 million chances to be wrong — and so no wrong
cluster merge. At 640 the same is true of all 1,088 of them in 224,868
proposals. That is the number to watch: a merge's cost is every pair the two
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
  group.rs            pairs -> groups, around a representative
  cache.rs            on-disk cache of the per-image analysis: where it lives
                      when nothing says, and how a record is packed
  problems.rs         what was skipped, what could not be done, the exit code
                      that says so, and --log-file
  progress.rs         one progress bar for the whole run: each stage owns a
                      stretch sized by its estimated cost (Forecast), revised
                      as the run learns; files weighted by a header probe
benchmark/
  BASELINE.md         the competition's numbers. The bar to clear.
  VALIDATION.md       the held-out corpus, and what it says about overfitting
  COMPETITORS.md      tool survey: what exists, what was rejected and why
  TOOLS.md            install/run notes and per-tool gotchas
  bench.py            the measurement harness (cold, sequential, instrumented)
  score.py            F1 against the generated ground truth
  analyse.py          score.py plus the three things the tuning rule needs:
                      false pairs split into traps and cross-family errors,
                      the family merges those imply, and the same run
                      re-scored on two disjoint halves of the seeds
  replay.py           replays the anchor tier and drop_weak_bridges off one
                      --dump, to count cross-family anchors *before* the
                      bridge test — the margin an output table cannot show
  run_bench.sh        simpler sequential driver, no instrumentation
  runners/run_*.py    one wrapper per tool -> canonical JSON
  corpus/
    README.md         how the corpus and its ground truth are built
    transforms.py     CATALOGUE: 90 transforms, every amount drawn per seed
    make_variants.py  the generator
  out/v5/             the competitors' run: eleven tools, one session
  out/v8/             img-fp's published cost row (see BASELINE.md)
  out/v9/             img-fp after the parameter pass
  out/v10/            img-fp at the old default of --work-size 640
                      (out/v5/DAMAGED.md records a file overwritten there;
                       give bench.py a fresh --out, it merges into the old one)
  out/v11/            img-fp at the shipped default, --work-size 384
  out/v11-worksize/   the one-session cold --work-size sweep behind the table
                      in *Measured trade-offs* (metrics only; the run JSONs
                      are 50 MB apiece and were not kept)
vendor/               third-party tools and venvs, gitignored
```

The corpus lives outside the repo at `/home/daniel/Documents/IMGS`: 54 seeds
in the root, 8 in `archive/`. One seed, `beach`, has no extension, so any run meant
to be compared with the published figures needs `-x '*'` — a walk leaves an
extensionless file out otherwise, as `vid-fp`'s does. `bench.py` passes it. `/home/daniel/Documents/IMGS-VAL` holds the 16
seeds that were once a separate validation set and are now folded in; it keeps
no derived tree.

**The found corpus is `/home/daniel/Downloads/archive`** — the "found corpus"
every cost table above is measured against, 9,285 files of the kind a camera
roll holds. The path is the whole of it: run it on `~/Downloads` instead and
one stray image from a neighbouring folder joins in, which reads 9,286 images
and **4,875 pairs against the real corpus's 4,581**, because that one file
lands in a family. A cost number taken that way is still a cost number; a pair
count is not.

It is **not labelled and has no ground truth**, so it scores nothing and never
will: it is for *cost* — wall, CPU-seconds, peak RSS — and for the
byte-identity check, where all that is asked of it is that two builds agree
with each other. Keep it for its shape rather than its size, since that is what
the benchmark corpus cannot supply: small images, upsampled before they are
analysed, and almost no duplicates, which is what puts 46% of a run in the
second look and 39% in the vocabulary descent. Quote it as a cost corpus and
never as evidence about accuracy.

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
  expansion: every propagated pair is tested against the pixels. The tree is
  breadth-first from the best-connected member; growing it best-edge-first was
  tried and is worse, because short paths matter more than strong links.

  **It is worth 7.1 points of F1**, which is more than every threshold in the
  tool put together, and the figure of "about 1.5 points" that stood here until
  now predates both the single catalogue and the mip-pyramid pixel check.
  Re-measured on the shipped build by running with and without the pass
  (`--no-propagate`, which is now a `--features prof` flag and not a CLI
  option — see below):

  | at `--work-size 640` | F1 | precision | recall | pairs | FP | groups |
  |---|---|---|---|---|---|---|
  | **propagation on** | **0.9775** | **99.52%** | **96.05%** | **224,779** | **1,088** | **122** |
  | `--no-propagate` | 0.9061 | 99.60% | **83.11%** | 194,330 | 780 | 199 |

  All of it is recall — **13 points** of it — and precision is a rounding error
  either way: the 308 false pairs propagation adds are all `column_roll` traps,
  not one of them crossing a family. Without it a family fragments into more
  representatives than it has (199 groups for 62 seeds) and corroboration takes
  over some of the work, 5,106 pairs against the usual 598, which is why the
  loss is 13 points of recall rather than the 30 the pair count suggests.

  It costs **7-11 CPU-seconds**, some 6-8% of a cached run, and 0-2 s of wall.
  Three alternating pairs with a 45 s cooldown, cached, `-t 8`: CPU 139.5 /
  127.9 / 127.6 against 121.3 / 120.5 / 120.9, wall 23.3 / 20.6 / 19.7 against
  18.7 / 19.0 / 19.9. The median pair is -7.5 CPU-s; the first pair's -18.2 is
  the day's hottest run and is why these are quoted as pairs. So the exchange
  rate is seven points of F1 for six percent of the clock, and nothing else in
  the tool is close.

  **Which is why `--no-propagate` is gone.** "Less time for less recall" is a
  legitimate thing to want, and this was the worst available way to buy it —
  not a worse point on the speed/accuracy frontier but nowhere near it, because
  propagation lives in the last fifth of the pipeline while `--work-size`
  scales the first four fifths. The two knobs' own tables share their shipped
  row, so they compare directly:

  | at `--work-size 640` | F1 | recall | cold wall | cold CPU |
  |---|---|---|---|---|
  | **640, propagation on** | **0.9777** | **96.09%** | **82.4 s** | **579 s** |
  | no propagation | 0.9061 | 83.11% | — | -7 to -11 CPU-s |
  | `--work-size 512` | 0.9724 | 95.03% | 69.4 s | 476 s |
  | `--work-size 448` | 0.9658 | 93.75% | 60.7 s | 413 s |
  | **`--work-size 384` (now the default)** | **0.9547** | **91.68%** | **57.4 s** | **360 s** |

  (The `--no-propagate` row is the one line here still carried from the earlier
  build; the work-size rows are the one-session sweep in *Measured trade-offs*.
  Nothing about the argument depends on the difference.)

  Every work-size row **dominates** it: 384 gives back 4.9 points of F1 and 8.6
  points of recall *and* saves 219 CPU-seconds where turning propagation off
  saves 9. There is no corpus and no setting on which a user wants the flag, so
  the pass is now unconditional. To re-measure the table above, put the `if`
  back around the propagation loop in `main.rs`; that is a two-line edit and a
  rebuild, and it is the right price for something no run should be doing.
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
- **A group is a representative and everything that matched it** (`group.rs`),
  not a connected component and not a maximal clique. Matching is not
  transitive and this tool manufactures the counterexamples by design:
  retrieval scores *containment*, so a photograph matches both the slide and
  the poster it appears in while those two match nothing of each other, and a
  left crop and a right crop are both matches for the whole and disjoint from
  each other. (The corpus does not exercise the host case — `collage_cell` and
  `contact_sheet` fill their other cells with synthetic patterns, so every edge
  measured here is inside one seed's family.) So every file in a group was
  verified against the file at its head, and nothing else is claimed. The
  representative is chosen greedily — the file still accounting for the most
  ungrouped files, ties to the lowest index — which is a statement rather than
  a threshold, and it is the file to keep.

  **The measurements that chose it**, all by re-grouping `out/v9`'s pairs:

  | rule | groups | untested pairs claimed | of those, wrong | one bad edge costs |
  |---|---|---|---|---|
  | connected components | 68 | 13,773 | 10,208 | ~7,400 |
  | maximal cliques | 6,991 | 0 | 0 | ~0 |
  | **representative (shipped)** | **122** | **0** | **0** | **58** |

  A closure is fragile — a single wrong pair between two families merges them
  entirely — and it invents an order of magnitude more false pairs than the
  tool itself makes, because a family is not a clique: `crop_strip_top` and
  `crop_strip_bottom` are both matches for the original and are DIFFERENT to
  each other. Cliques fix that and are what the sibling `vid-fp` uses
  (`src/clustering.rs` there is the fuller argument), but a video library is
  sparse and a photograph with ninety transformations is not: 6,991 cliques for
  62 families, one file in 577 of them, and enumeration that needs a probe
  budget and an abandonment path because `3^(n/3)` is reachable. A star keeps
  the clique's honesty at the component's group count, is `O(E log V)` with no
  worst case to defend, and takes 11 ms against the clique search's 43.

  Two things follow. Groups **overlap** — a file matched by two representatives
  is under both, which is how a file only one of them reached gets reported at
  all (4,374 of 5,512 files here). And a group is **not an all-pairs claim**:
  two members that both matched the representative were never compared with
  each other.

  Measured alternatives that were tried and are worse, should this come up
  again: 2-edge-connected and biconnected components survive exactly *one*
  false edge and collapse at two (the bridge test's own limitation, already in
  the notes below); a k-truss — every link corroborated by k-2 files that
  matched both ends — is immune at k=4 even to twelve clustered false edges and
  costs nothing on this corpus, but it needs density to work at all and is
  useless where a group is two or three files, so it would not transfer to
  vid-fp. A partition (each file under one representative only) strands the
  leftovers: 15 files dropped, and a tail of scraps rather than families.
- **The vocabulary is sized from the corpus** (`VocabParams::for_corpus`), not
  fixed. Leaf occupancy is what is held constant, because that also fixes the
  average document frequency of a word, which is what idf and the posting-list
  cap are written against. A fixed 65,536 words made img-fp nearly useless on
  small folders; do not put it back.

  **Its centres are stored as bytes, and that is the one lossy step in the
  index.** A centre is the mean of a cluster of descriptors, and descriptors are
  bytes; the tree is *built* in floats and then rounded to the nearest byte,
  because the descent is bound by how much of a hundred-megabyte tree it can
  drag through the caches rather than by its arithmetic. The rounding is
  derived, not fitted: a centre is the mean of a cluster drawn from a *sample*
  of the corpus — 160,000 descriptors of several million — so its own sampling
  error is on the order of a whole unit, a hundred times the half-unit the
  rounding adds, and at the deepest level, where most nodes hold one member,
  the centre is that member's own bytes and the rounding is exact. Measured on
  the benchmark corpus it is worth 0.0002 of F1 *upwards* with the false-pair
  count unchanged to the pair; see *Speed and memory*.

  **And occupancy has to be held by the branching, not only by the depth.**
  This was a real defect and it stood for three versions. With `branching`
  pinned at 16 the only reachable sizes are `16^depth`, so the occupancy the
  rule claims to hold constant actually swung by a factor of sixteen — 2
  descriptors per word just above a step, 32 just below one — and the rule's
  own target, 32, was the far end of that swing. The coarse end merges
  families: two photographs of different beaches match along what they share,
  and several such edges arrive together, so the bridge test cannot drop them.
  Measured on the benchmark corpus by forcing the depth, with nothing else
  changed: the shipped configuration at 2.3 descriptors per word makes **no**
  cross-family pair, and at 26 it makes **2,738** and merges `4.jpeg` with
  `galaxy.jpeg`.

  The trap was reachable by being an ordinary size, which is what makes it
  worth this much text. The step sits at 2,097,152 descriptors — about 4,930
  images at stock settings — and this corpus has 5,638, some 15% the safe side
  of it. A folder of 3,965 files, every flag at its default, landed at
  occupancy 26 and merged three families: 1,751 cross-family pairs, precision
  97.97% against the usual 99.5%. `for_corpus` now picks the depth as before
  and then *narrows the branching* to the smallest tree of that depth that
  still holds the target, which keeps occupancy in [2.2, 2.9] from a thousand
  descriptors to eight million where the old rule ranged over [2.0, 30.5].

  **And a word is a live leaf, not a leaf number.** The two are worth keeping
  apart because the tree is trained on a *sample* — 160,000 descriptors — so
  the number of leaves that can ever be reached is bounded by that, while the
  numbering runs to `branching^depth`: 1,771,561 numbers over 156,519 live
  leaves on the found corpus, 537,824 over 139,691 here. `quantise` returns the
  live leaf's centre slot, which rises with its node number (centres are laid
  down parent by parent and child by child), so every order downstream is
  unchanged and the inverted file's three per-word arrays are a tenth the size.

  What that is worth, all at stock flags: the benchmark corpus is **pair-for-
  pair identical**, verdict fields included (it already had 16^5 words, which
  is what the new rule picks for it); the 3,965-file folder goes to **0
  cross-family pairs and 99.54% precision**; 91 files improve (F1 0.988 to
  0.991); 8 files and 455 files are unchanged. The target of 3 is not a fitted
  number — it is the occupancy the measured build already runs at, 2,404,926
  descriptors into 1,048,576 words. Swept on the 3,965-file folder, 3 and 6 are
  both clean and 12 merges two families, so the shipped value sits a factor of
  four from the cliff. Do not raise it because 6 scores a hair better.

### What still misses

**19,378 pairs at the default, 9,100 at `--work-size 640`** — the default's
extra 10,278 misses are what the speed is bought with, and they are not spread
evenly. The worst rows, out of 62 seeds, with 640 in brackets: `embed_tiny` 27
(55), `contact_sheet` 29 (55), `crop_micro` 39 (41), `picture_in_picture` 45
(60), `crop_strip_top` 54 (57), `magazine_spread` 54 (60), `halftone` 55 (56),
`tiled_watermark` 55 (59), `wall_poster` 55 (61), `pdf_page` 56 (62),
`slide_deck` 56 (62), `wave_vertical` 56 (59).

Two patterns, and only one of them is new. The old one is what it has always
been — very small crops, heavy downscales, and warps that break a fitted affine
model, though the warps are much less of it than they were, because the pixel
check no longer aliases across a scale gap. The new one is **containment**, and
it is the default's doing: the rows that fall furthest are the ones where the
photograph is a small part of a larger canvas, because 384 is the long side of
the *canvas* and the photograph inside it is a fraction of that. Nine of the
fifteen transformations that leave the perfect column between 640 and 384 are
containment rows. This is the cost worth knowing about before recommending the
default to anyone whose corpus is screenshots, slides or contact sheets.

**One row where a competitor still leads**, at either work size, and it is the
same row: PDQ takes `halftone` 58/62 against 55 at the default and 56 at 640.
Checked against all nine competitor columns from `out/v5`, no other row changes
hands at 384 — the margin narrows, and on the containment rows it narrows a
lot, but 27/62 against SSCD's 0/62 is still the whole field. `rot180` was the other, and is now 62/62 against SSCD's 61 —
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

Applying that rule took the CLI from 13 result-changing options to 5, the
acceptance policy from 9 fitted numbers to 2, and the bridge test from 3 to 0,
at equal or better F1 every time. Removing a parameter is the cheap experiment;
run it before adding one.

**Every sweep table below was measured on the build before the fifth
optimisation pass**, whose byte-rounded vocabulary centres moved the shipped
point from F1 0.9775 to 0.9777 — 224,779 proposals to 224,868, the same 1,088
false pairs, the same zero cross-family — and the tables are left as measured
rather than half-rewritten. Nothing in them is a shape a tenth of a point of
recall could change, and the two that matter, the *distance to the cliff* for
`--min-pixel-correlation` and `--min-aligned-points`, are properties of the
acceptance rule and not of the vocabulary. Re-sweeping either is a run per
value; do it when a threshold is actually in question.

**And the names are load-bearing too.** `--min-frame-overlap` and
`--min-pixel-correlation` were `--min-overlap` and `--min-agreement`, which
read as a loose and a tight version of one bar — a misreading `verify.rs` used
to spend fourteen lines of comment trying to prevent. The nouns now carry it:
one is **frames**, pure geometry with no pixel read, and the other is the
**pixels** in them. `--min-aligned-points` was `--min-inliers`, which is RANSAC
jargon for "correspondences that agree on one transform". A name that needs a
comment to stop a misreading is the wrong name; the comment got shorter.

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
the distance to it. Measured on this corpus: `--min-aligned-points` is clean at 8 and
catastrophic at 7 (8,148 cross-family pairs, 9 merges); `--min-pixel-correlation` is
clean at 0.45 and merges at 0.40. The shipped values keep the same margin the
previous build had, which is why neither of them moved to the value that
scored best.

**`--min-frame-overlap` and `--min-pixel-correlation` are not two strengths of one bar.**
They sit one line apart in `Verdict::accepted`, which is why they read like a
loose and a tight version of the same idea, and they are not: each is the only
defence against a failure mode the other cannot see at any setting. The note on
that function carries the per-mode numbers — the two Excel screenshots match at
median overlap **1.000** and median agreement **0.000**, a `column_roll` against
a crop of its own original at median overlap **0.562** and median agreement
**0.971**. Swept on the full corpus, everything else at stock, false pairs split
into rearrangement traps and cross-family errors:

| `--min-frame-overlap` | F1 | precision | recall | FP | trap | cross | merges |
|---|---|---|---|---|---|---|---|
| 0.50 | 0.9638 | 95.91% | 96.85% | 9,626 | 9,625 | **1** | 1 |
| 0.60 | 0.9724 | 97.72% | 96.76% | 5,261 | 5,261 | 0 | 0 |
| 0.70 | 0.9763 | 98.84% | 96.44% | 2,626 | 2,626 | 0 | 0 |
| 0.75 | 0.9763 | 98.88% | 96.40% | 2,535 | 2,535 | 0 | 0 |
| 0.80 | 0.9777 | 99.39% | 96.20% | 1,368 | 1,368 | 0 | 0 |
| **0.85** | **0.9775** | **99.52%** | **96.05%** | **1,088** | **1,088** | **0** | **0** |
| 0.90 | 0.9741 | 99.61% | 95.31% | 871 | 871 | 0 | 0 |
| 0.95 | 0.9642 | 99.65% | 93.40% | 770 | 770 | 0 | 0 |

| `--min-pixel-correlation` | F1 | precision | recall | FP | trap | cross | merges |
|---|---|---|---|---|---|---|---|
| 0.30 | 0.8945 | 83.39% | 96.45% | 44,725 | 1,185 | **43,540** | 12 |
| 0.35 | 0.9683 | 97.23% | 96.42% | 6,387 | 1,158 | **5,229** | 2 |
| 0.40 | 0.9732 | 98.30% | 96.36% | 3,879 | 1,129 | **2,750** | 1 |
| 0.45 | 0.9785 | 99.49% | 96.26% | 1,146 | 1,146 | 0 | 0 |
| **0.50** | **0.9775** | **99.52%** | **96.05%** | **1,088** | **1,088** | **0** | **0** |
| 0.55 | 0.9758 | 99.54% | 95.70% | 1,020 | 1,020 | 0 | 0 |
| 0.60 | 0.9729 | 99.59% | 95.09% | 920 | 920 | 0 | 0 |
| 0.70 | 0.9600 | 99.61% | 92.64% | 849 | 849 | 0 | 0 |

Four things to read off those, and the first is why the pair is worth this much
text. **Loosening overlap merges nothing and adds traps; loosening agreement
merges families and adds almost no traps.** Of the 2,847 pairs that are false at
agreement 0.40 and were not at 0.50, **2,750 are a single family merge** — the
two Excel screenshots. Of the 1,544 that are false at overlap 0.70 and were not
at 0.85, the transforms naming them are led by `column_roll` and `tile_shuffle`,
and **none of them crosses a family at all**.
**Their margins are not comparable**: agreement's cliff is one step below
shipped, where overlap has none in the usable range and 0.35 of clear air below
it. **Both score best one step loose** — overlap 0.80 by 0.0002, agreement 0.45
by 0.0010 — and neither should move, because that is buying F1 with the distance
to a cliff, which is the whole of the rule above. And **the seed halves cannot
see this cliff**: they track each other everywhere on the safe side, but at
agreement 0.30 half A is 0.8990 against half B's 0.9785, because the merges all
land in one half. Halves test whether a *shape* generalises, not whether a value
is safe.

**Neither of them is a cost knob, and the one cost effect runs backwards.**
`--min-frame-overlap` looks like one: it gates `pixel_check`, the most expensive thing
done per pair, at `verify.rs:1149` and again on propagation. It saves almost
nothing, because the quantity it gates is bimodal — of 210,133 direct verdicts
eligible on inliers, **189,439 (90.2%) already have an overlap of 0.9 or more**,
so the floor turns away 193,430 checks at 0.85 against 196,462 at 0.70 and
183,653 at 0.95. Measured with `--features prof` and propagation disabled, direct
`pixel_check` is flat at 26.7-32.3 CPU-seconds across every setting of *either*
knob. What does move is propagation, and it moves the wrong way: a round
proposes every **unmatched** pair inside a component, so *tightening* a bar
leaves more of them and makes more work. Composed hypotheses go 70,887 ->
**75,986** -> 147,876 as agreement goes 0.35 -> 0.50 -> 0.70, and 57,562 ->
**75,986** -> 131,989 as overlap goes 0.70 -> 0.85 -> 0.95. Agreement at 0.70
doubles them and costs 8-10 CPU-seconds of `pixel_check` (42.4 and 43.6 against
the shipped 35.3 and 32.8, two reps each) — the only cost signal either knob
produced that cleared this machine's noise floor. The overlap points did not:
the same configuration measured 41.4 s and 33.1 s on two reps, which is the 25%
swing *Speed and memory* warns about. Set both for what the tool should claim,
never for what it costs.

**`--min-aligned-points` is the third bar in that `if`, and the only one propagation
can overrule.** `Policy::new` gives the propagated tier `min_aligned_points: 0`
(`verify.rs:228`) — no features vouch for a composed transform, so the count is
not evidence about it — while overlap and agreement are inherited by all three
tiers. A pair the inlier bar rejects can therefore come back through
propagation; a pair the other two reject is gone. That asymmetry is most of why
this knob behaves differently from the other two at the tight end.

| `--min-aligned-points` | F1 | precision | recall | FP | trap | cross | merges | wall |
|---|---|---|---|---|---|---|---|---|
| 5 | 0.9535 | 93.19% | 97.61% | 16,620 | 1,263 | **15,357** | 55 | 69.2 s |
| 6 | 0.9668 | 96.02% | 97.34% | 9,390 | 1,232 | **8,158** | 13 | 49.8 s |
| 7 | 0.9654 | 96.02% | 97.07% | 9,361 | 1,213 | **8,148** | 10 | 32.1 s |
| 8 | **0.9809** | 99.48% | 96.74% | 1,181 | 1,181 | 0 | 0 | 23.2 s |
| 9 | 0.9795 | 99.51% | 96.44% | 1,101 | 1,101 | 0 | 0 | 21.5 s |
| **10** | **0.9775** | **99.52%** | **96.05%** | **1,088** | **1,088** | **0** | **0** | ~25 s |
| 11 | 0.9745 | 99.52% | 95.46% | 1,077 | 1,077 | 0 | 0 | 27.8 s |
| 12 | 0.9714 | 99.55% | 94.84% | 1,002 | 1,002 | 0 | 0 | 20.3 s |
| 13 | 0.9680 | 99.55% | 94.19% | 990 | 990 | 0 | 0 | 33.5 s |
| 14 | 0.9655 | 99.55% | 93.71% | 983 | 983 | 0 | 0 | 20.2 s |
| 15 | 0.9642 | 99.56% | 93.48% | 973 | 973 | 0 | 0 | 27.2 s |
| 16 | 0.9623 | 99.56% | 93.11% | 968 | 968 | 0 | 0 | 20.4 s |
| 17 | 0.9591 | 99.56% | 92.52% | 954 | 954 | 0 | 0 | 27.1 s |
| 18 | 0.9563 | 99.56% | 92.01% | 945 | 945 | 0 | 0 | 25.1 s |
| 20 | 0.9514 | 99.57% | 91.08% | 908 | 908 | 0 | 0 | 31.0 s |

The cliff above reproduces to the pair — 8,148 cross-family pairs at 7 — and so
does the reason not to move: **8 scores 0.0034 better than shipped with nothing
cross-family**, and sits one step from catastrophe where 10 sits three. Unlike
agreement, whose cliff arrives one family at a time (1, then 2, then 12), this
one arrives whole. (Above the cliff the wall column is thermal noise; below it,
it is the merge tax, and it scales with how bad the merge is.)

**Cliff or plateau depends on which column you read, and that is the point.**
Recall is a pure **ramp** — strictly monotone across all fifteen points, no knee,
about 0.47 points per unit. Precision is a **step**: 7 -> 8 jumps 3.46 points,
and then 8 -> 20 moves **0.09 points across twelve units**, a dead-flat plateau.
F1 is the product of the two, so it inherits the cliff below 8 and the ramp
above it: strictly monotone decreasing at every one of the twelve steps above
the cliff, with its only inversion at 6 -> 7, inside the catastrophe where the
ordering is meaningless.

So **there is no F1 plateau anywhere in the safe range**, and the plateau test
from the rule above returns an unambiguous verdict: 10 is not sitting on a
plateau, it is sitting on a slope, and F1 says to move it to 8. Compare the
geometric inlier tolerance, where every value from 0.010 to 0.025 is within
0.001 of the same F1 — that is what a plateau looks like, and this knob has
nothing resembling one. Two things make the answer *keep it at 10* anyway.
**The F1-optimal safe value and the cliff edge are the same value**, so
following F1 leaves not a thin margin but none at all. And above the cliff the
knob buys nothing to begin with: 8 -> 20 removes 273 false pairs, **every one of
them a trap**, for 0.09 points of precision and 5.66 points of recall. Whatever
`--min-aligned-points` does for precision, it does entirely in the single step from 7
to 8.

The halves behave the way they did for agreement: identical to four places at 13
and close everywhere above the cliff, wildly apart below it (at 5, half A is
0.9855 against half B's 0.9422). They confirm the shape and cannot see the
edge.

**Its cost is the merge tax, not the gate.** As a gate it is the weakest of the
three: `n_in` is even more skewed than overlap — 156,016 of 236,549 direct
verdicts carry 50 inliers or more — so the shipped value turns away 193,430
checks against 204,254 at 6 and 177,283 at 20, a band of ±6%. But it is the only
one of the three whose *clock* moves, and it moves at the loose end only,
because that is where the families merge: a merged component is enormous and a
propagation round proposes every unmatched pair inside one.

| `--min-aligned-points` | composed hypotheses | `pixel_check` CPU-s, 2 reps | no propagation |
|---|---|---|---|
| 6 | **340,008** | 57.0 / 44.9 | 44.2 / 29.4 |
| **10** | **75,986** | **37.1 / 33.6** | **28.1 / 27.3** |
| 20 | 81,742 | 31.8 / 34.0 | 25.3 / 26.2 |

At 6 the hypothesis count is **4.5x** the shipped one and the sweep's run took
49.8 s of wall against the usual ~25 s. At 20 it is +7.6% and the clock is level
or better — which is where the propagated tier's `min_aligned_points: 0` shows up, since
the pairs a tight inlier bar rejects are exactly the ones propagation gets back.
Tightening `--min-pixel-correlation` to 0.70 doubles the hypotheses because nothing gets
them back. So the rule for all three holds, with this one for a different
reason: a setting that costs real time is telling you it has merged something.

**Making it self-adjusting: derivable, measured, and worse.** The bar is a
*count*, so unlike the other two it has no natural scale — which makes it the
obvious candidate for deriving from the corpus instead of fitting. There is a
clean derivation available. `from_single` builds a 4-DoF similarity from one
correspondence and maps its own anchor exactly, so under a null model of
randomly-placed correspondences an observed count is `1 + Binomial(n_match - 1,
p)`, where `p` is the chance a random correspondence lands within tolerance:

```
tau = max(0.015 * diag(B), 3)   p = pi*tau^2 / area(B) = pi * 2.25e-4 * (r + 1/r)
```

That is 1.47e-3 at 4:3 and depends on aspect only through `r + 1/r` — 2.0
square, 2.34 at 16:9 — so it varies by well under one step. Every
correspondence is tried as a hypothesis, so the run's expected number of
coincidental anchors is `V * min(n_match,600) * P(X >= k-1)`; set that below
alpha and solve for k. Every term is known at runtime: `n_match` is already a
`Verdict` field and `V` is the candidate-pair count.

**It reproduces both numbers it should.** At a typical rich pair and this
corpus's 236,549 verified pairs it returns **10**, the shipped value; at
alpha=1, where coincidence stops being expected, it returns 8-9, which is the
measured cliff. Two unrelated methods landing on the same two numbers is as
much confirmation as this project gets without a held-out corpus. It is also
insensitive in the right way: 1000x in alpha moves the bar two steps, and so
does 1000x in corpus size (8 at six files, 12 at 5.6M).

**And it is worse.** Replayed over the shipped build's own verdicts, carried
through `drop_weak_bridges` and component assembly:

| rule | anchors | cross-family anchors | after bridge test | families merged |
|---|---|---|---|---|
| flat 6 | 199,179 | 20 | 7 | **2** |
| flat 8 | 195,820 | 6 | 0 | 0 |
| **flat 10 (shipped)** | **192,390** | **3** | **0** | **0** |
| flat 12 | 189,277 | 2 | 0 | 0 |
| derived alpha=1 | 201,778 | 25 | 5 | **1** |
| derived alpha=0.01 | 201,007 | 13 | 1 | **1** |
| derived alpha=0.001 | 200,156 | 9 | 1 | **1** |

It merges `13.webp` with `low-light.avif` at **every** alpha, including one
where it is stricter in aggregate than the flat bar. Tightening cannot fix it,
because alpha moves the bar one step per three decades while the offending
pairs are sparse and the model discounts sparse pairs by construction.

**Why, and this is the part worth keeping.** Every cross-family anchor it
admits and the flat bar rejects is sparse — `n_match` 8 to 42, `n_in` 5 to 9,
two of them screenshots. The null model bounds **coincidence**, and coincidence
was never the binding constraint. Correspondences between two different
photographs exist *because* the images share real structure — a logo, a
horizon, page furniture — so conditioned on there being few of them they are
*more* likely to be geometrically consistent, not less. The model is loosest
exactly where the evidence is least random. So the bar is flat because the
failure mode it defends against does not scale with `n_match`, and any rule
that hands an individual pair a discount walks into it. A *per-corpus* scaling
is untouched by this result, since it never discounts a single pair; it is
untested, not refuted.

**What the same replay says about the margin.** Counting cross-family anchors
*before* `drop_weak_bridges` shows something the output table above cannot: the
precision plateau from 8 to 20 is real in the output, but the margin behind it
is not flat. **8 leaves six cross-family anchors for the bridge test to absorb
where 10 leaves three**, and the bridge test survives exactly one false edge.
That is an argument against F1's preference for 8 that does not depend on F1 —
and it is why the one merge that survives at alpha=0.001 gets through at all:
its far side is a single file, which is the one shape `drop_weak_bridges` keeps
by design.

**Does the level transfer?** `--work-size 384` is a fair proxy for a
keypoint-poor corpus — same ground truth, same content, about a third of the
descriptors. Swept at both sizes:

| bar | **ws 640** F1 | recall | cross | merges | **ws 384** F1 | recall | cross | merges |
|---|---|---|---|---|---|---|---|---|
| 6 | 0.9668 | 97.34% | **8,158** | 1 | 0.9755 | 95.60% | **4** | 2 |
| 8 | **0.9809** | 96.74% | 0 | 0 | **0.9656** | 93.69% | 0 | 0 |
| 10 | 0.9775 | 96.05% | 0 | 0 | 0.9553 | 91.78% | 0 | 0 |
| 12 | 0.9714 | 94.84% | 0 | 0 | 0.9434 | 89.56% | 0 | 0 |

Three readings. The **safe optimum is 8 at both** and the cliff sits between 6
and 8 at both, so the level survived a 3x change in descriptor count — which
was the specific worry, and it did not fire. The **price of the margin triples**:
choosing 10 over 8 costs 0.69 points of recall at 640 and 1.91 at 384, buying
nothing either time. And the **cliff's depth varies by three orders of
magnitude** — one step below safe produces 8,158 cross-family pairs at 640 and
4 at 384. That last one is the reason the margin is wider than F1 wants: the
cost of going over the edge is not a corpus-independent quantity, so the
distance to it should not be shaved to the minimum that happens to work here.

**How all of that was measured, because it is reusable.** `--dump` sets the
pixel-check gate to `(3, 0.2)`, so `blk` is a real measurement for every
verdict with three inliers or more and *any* bar at or above 3 can be replayed
offline against one run — no rebuild, no re-run per value. Replaying the anchor
tier this way reproduced the run's own count to **186,616 against 186,614**,
the gap being the dump's four-decimal rounding. `drop_weak_bridges` is a
sixty-line Tarjan and replays the same way, which is what turns an anchor count
into a merge count. Sweeping a CLI flag costs a run per value; this costs one.

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
follows the descriptor count — and so does the branching, because depth alone
could only size the tree to within a factor of sixteen and the coarse end of
that merges families at ordinary corpus sizes. See *How img-fp works*.

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
**byte-identical output**: the same 223,673 pairs and the same groups, field
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
less than 10% cannot be seen at `-t 8` at all, and the only deterministic
reading is `-t 1`, where there is one decode in flight and the walk order
decides everything. Measured there, the third pass is **785 MB to 770 MB**
(803,596 and 804,668 KB against 787,192 and 790,208 KB, two runs each) — the
shared analyses and the three Gaussian planes, and about as much as those two
are worth. At `-t 8` the six runs of each average 977 MB against 914 MB, which
points the same way and proves nothing, given the spread above.

The two figures together are the shape of the thing: what the program *holds*
went down by 15 MB, and what it *peaks at* on eight threads is mostly not that.

**What this machine is limited by, which decides what is worth trying at all.**
One busy core boosts to 3.2 GHz; eight run at 1.27 GHz, and the extraction
phase is only 2.9x faster on eight threads than on one. Under a power cap wall
time follows *energy*, not cycles, and the two respond to different changes.
Removing a **stall** — a float divide, a mispredicted branch, a cache miss the
other hyperthread was glad of — is worth a clean 5% at `-t 1` and *nothing* at
`-t 8`, where the sibling thread simply takes the slot. What moves the
eight-thread clock is removing **work**: bytes not moved, instructions not
issued. So measure a change at `-t 1` to learn whether it is faster, and at
`-t 8` to learn whether it matters; several of the entries below were worth
half of what the single-threaded number promised, and the ones that survived
are the ones that move less memory.

Where the time goes now: decode ~35%, feature extraction ~45%, everything
after it ~20%. Within extraction the descriptor is the largest single item, and
within decode it is JPEG — `--features prof` prints the whole table at the end
of a run, which is how the third pass below was aimed. (Those three shares have
not moved across five passes, which is a coincidence rather than a law: the
fifth pass took 16% off the matcher and 8% off decode, and the ratio came out
where it started.)

**Those three shares are at `--work-size 640` and the default is no longer
there.** `--work-size` scales the first four fifths of the pipeline and the
decoders it scales the *least*, since a file is decoded at its own size
whatever the working size is — so lowering the default raised decode's share
rather than lowering it. Measured at 384: decode is about **48%** of a
benchmark run, extraction **39%** and the matcher the rest. That is the reason
the seventh pass below is worth twice as much on the found corpus as here, and
the reason anything further aimed at this corpus has to be aimed at the image
crate's decoders — where, per the paragraph below and the DCT-scaled entry
under *Tried and rejected*, there is not much to reach for.

That table also settles a question worth not re-asking: the corpus's five
exotic formats are *not* where the decode time is. WebP, TIFF, JXL and the
libheif formats together are 431 of 5,638 files and about 80 CPU-seconds
against JPEG's 171 — four times the cost per file and a twelfth of the total.
A faster HEIC path would be worth 3% of the run, and there is no faster JPEG
path to reach for; see the DCT-scaled entry under *Tried and rejected*.

**Those proportions are this corpus's, and a found one is not like it.** The
same table over 9,285 photographs a camera roll might hold — 224x224 JPEGs,
almost none of them duplicates — reads decode **1.7%**, extraction **46%**,
everything after it **52%**, of which the mirrored and inverted second look
alone is **41%**. (Before the fifth pass those read 1.5%, 40% and 58%, with the
second look at 44%: cutting the descent moved the balance back towards the
pixels.) Two things make the difference and neither is the pixels.
The second look runs on files the first pass did not anchor twice, which is
349 of 5,638 here and **8,769 of 9,285** there, so its cost scales with how
many files have *no* duplicate — the normal case. And a small image is
enlarged before it is analysed (`upsample_below`), so those 224-pixel files
are described at 448 and cost four times the scale space: priced by turning
the enlargement off, the run goes 292 s to 94 s and the pairs go 4,581 to
2,179, which makes it the largest speed/accuracy knob there is on a corpus of
small images and not a defect.

Priced by deletion it was **117 s of wall and 832 CPU-seconds for 633 pairs**
on that corpus, against **14 thread-seconds for 5,774 pairs** on this one; the
fifth pass takes the first figure to **565 CPU-seconds** without changing what
it buys. Before reaching for it, read *What the second look costs* below: the
three obvious ways to make it cheaper were measured and all three are worse
than they look.

What was worth doing in the **matcher pass**, which is the first one aimed at
the retrieval and verification half rather than at the pixels. All three are
byte-identical — the same 227,838 pairs and 122 groups on this corpus, the
same 4,581 pairs and 874 groups on the found one, checked over six alternating
runs of each build.

- **The candidate ranking sorted the whole corpus to keep a hundred and fifty
  of it.** A query touches nearly every file that shares a word with it, so
  `scored` arrives holding thousands of entries, and it was fully sorted
  before being truncated to `-k`. The comparator is a total order — scores tie,
  image indices cannot — so the `k` that survive and the order they survive in
  are settled by the comparator alone, and `select_nth_unstable_by` before
  sorting the survivors gives the same answer. Measured at **2.2 ms a query**,
  which was more than the retrieval it ranked; on the found corpus the stage
  went **77.2 CPU-seconds to 10.5**, and it is paid once per image in the first
  pass and three times more for every image the second look re-asks.
- **The vocabulary descent's wide path was dead code on nearly every corpus.**
  The innermost loop had a specialisation for a node with `MAX_BRANCH` live
  children and a runtime-width loop for everything else — and since
  `for_corpus` narrows the branching to the smallest tree that holds the target
  occupancy, the only corpora reaching sixteen are those whose descriptor count
  sits just above a power of it. Everything else ran a loop whose trip count
  the compiler could not see, which neither unrolls nor vectorises. Measured on
  the descent alone at depth 4: width 15 cost **9,461 ns/descriptor against
  width 16's 5,431**, for less arithmetic. Dispatching on the width as a
  constant puts every width on one curve — 8 is **2.2x** faster, 12 **1.65x**,
  11 **1.20x**, 15 **1.13x** — and the arithmetic is unchanged, each centre's
  sum still taken over the dimensions in order. `cargo test --release --
  --ignored --nocapture quantise_branching` is the measurement.
- **The word-list intersection walked both lists.** Two images' lists hold
  about 1,300 entries each over a million-odd words and share some tens, so
  the merge spent one three-way comparison per entry of both lists on data
  that gives the predictor nothing. Galloping to the next candidate instead of
  walking to it is the same intersection in the same order. It is the smallest
  of the three — about **4%** of that function — because the function turns out
  not to be bound by its loop; see below.

**A second matcher pass, aimed at the vocabulary descent.** Re-profiled on the
current build at eight threads, the descent — `quantise` plus the second
look's `variant:quantise` — is **602 of 1,525 CPU-seconds (39%)** on the found
corpus and 63 of 727 (9%) on this one, which makes it the largest single item
across the two. Both changes below are byte-identical: the same 227,838 pairs
and 122 groups here, the same 4,581 pairs and 874 groups there, over six
alternating runs of each build on each corpus.

- **The distance loop was handed all 128 dimensions at once, and would rather
  have four spans of 32.** The sum does not change — the terms are still added
  in dimension order, so a distance finished in four pieces is the same float
  to the bit — but the shorter, fixed-length run compiles to something much
  better at every width `for_corpus` picks but two. Paired on the isolated
  descent, cooled before each: width 9 costs **2,639 ns/descriptor against
  3,728**, 11 **3,799 against 5,357**, 13 **4,444 against 7,076**, 15 **5,296
  against 8,442** — 20 to 37 per cent off, for arithmetic that is not merely
  equivalent but identical. At 8 and 16 the width is a whole number of vector
  lanes, the whole-array loop was already compiling to the right thing, and
  the split costs 6 to 8 per cent; those two keep it. That last line is the
  one part of this which is a fact about the compiler rather than about the
  tree, so re-run `quantise_branching` against both forms after a toolchain
  change rather than trusting the rule.
- **The frontier selection ran after all of a level's distances were taken.**
  It picks the best `max_paths` of up to forty-eight entries, and it was a
  second pass over an array that had just been written. Run as each child's
  distance arrives, it reads the distance where the distance already is —
  worth about 4 per cent at width 16, where the split above does not apply. It
  is why the parents are now copied out of the frontier before a level starts:
  the frontier is being rebuilt while it is still being read. `keep` lost its
  `min(max_paths, n_next)` in the process, which was never doing anything: the
  two differ only when a level offers fewer children than `max_paths`, and
  then the scan never fills up and never reads the bound.

Together, on the found corpus, three alternating cold pairs: CPU **1,646.9 ->
1,574.5**, **1,733.1 -> 1,633.7**, **1,712.8 -> 1,621.5**, with wall 241.3 ->
233.5, 255.7 -> 243.1 and 251.7 -> 240.7. That is **-4.4, -5.7 and -5.3 per
cent of the CPU** and -3.3 to -4.9 of the clock. Nothing else in the run
changed, so the descent's 602 seconds became about 512 — **15 per cent in
place against 29 in the bench**, which is the gap this machine always shows:
eight threads reading a hundred-odd megabytes of tree at random are waiting for
memory, and the bench measures the arithmetic they are waiting with. On this
corpus the same three pairs are **-0.7, -1.8 and +0.3 per cent** — level,
because 5,638 images size the vocabulary to 16^5 and sixteen is one of the two
widths that keep the old loop.

**A fourth pass over the extractor, aimed at the descriptor and the blur.**
Together `sift:describe` and `sift:blur` are 314 of 727 CPU-seconds (43%) on
this corpus and 505 of 1,525 (33%) on the found one, which is what is left of
the pixels after three passes over them. Both changes are byte-identical over
the whole of both corpora: the same 227,838 pairs and 122 groups here, the
same 4,581 pairs and 874 groups there, representatives included.

- **The descriptor's wide sweep was not wide.** The row is swept twice on
  purpose — once to work out where each sample lands and how much gradient it
  carries there, once to add those contributions in — because the first half
  is the same short chain of multiplies for every sample and the second is a
  scatter. The first half then compiled to one sample at a time anyway, and
  the listing says why: the Gaussian weight is a table lookup, which is a
  *gather*, and the sweep's own index is `u as f32` on a `usize`, which the
  compiler does by pulling each lane out of the 64-bit counter. Either is
  enough to stop a loop vectorising, and they sat in the middle of it.
  Taking the table's **index** in the wide sweep and leaving only the read for
  a sweep of its own, and spelling `0.0, 1.0, .. 15.0` as a constant table,
  puts the rotation, the grid position, the three floors, the fractions and
  the bin index into `vroundps`/`vcvttps2dq`/`vpmulld` eight samples at a
  time. Every value is the same float it was, computed from the same terms in
  the same order. The scatter is now the only scalar part, and it no longer
  takes an unpredictable branch either: the middle sweep collects the samples
  that landed inside the grid while it is there.
- **And the sweep was sixteen samples wide where a row is sixty.** Sixteen
  was chosen as "two vectors' worth"; what it actually does is decide how
  often the three sweeps hand each other a block, and each handover is a
  narrow load waiting on a wide store that has not retired. A descriptor's
  search row is some sixty samples at the scales this runs at, so sixty-four
  is usually the whole row and there is no handover at all. Paired on the
  isolated descriptor, 8 / 16 / 32 / 48 / 64 cost **30.9 / 26.9 / 24.1 / 22.8
  / 22.4 ms** for two thousand of them.
- **The blur built each output row somewhere else and then copied it in.**
  The vertical pass ran its `r + 1` tap passes over a scratch row and then
  `extend_from_slice`d that row into the plane — a whole plane read and
  written again for every blur, twenty times an image. It accumulates into
  the plane itself now, through `spare_capacity_mut`, and the scratch row is
  gone from `BlurScratch` with it.
- **And it searched for each tap's row.** `reflect101` is a loop and
  `% ring_rows` is a real division — `ring_rows` is `2r + 1` for whatever
  sigma this blur is, not a constant — and the pair of them was taken twice
  per tap per output row, twenty-two times a row at the widest. Away from the
  two edges no tap reflects at all and a tap's slot follows from the row's by
  one conditional wrap; only the first and last `r` rows still take the
  search.

What they are worth, and the measurement is worth as much as the numbers.
Single-threaded, on the benchmarks: **two thousand descriptors 29.1 / 29.5 /
29.1 / 29.4 ms against 23.4 / 23.4 / 23.3 / 23.0**, four alternating pairs
cooled before each, and `extract 640x480` **30.8 / 30.7 / 30.5 / 30.2 against
27.3 / 26.5 / 26.8 / 26.2** — 20% off the descriptor and 12% off the whole
extractor.

In place at eight threads it is less, and the honest way to see how much is
**within a single run**, where a stage faces the same clock and the same
contention as its siblings. Against `sift:ori`, which nothing here touched:
`sift:desc` goes from 4.74 of it to 3.64 on this corpus and from 5.60 to 4.45
on the found one — **−23% and −20%** — and `sift:blur` from 4.00 to 3.70 and
from 3.24 to 3.16, which is **−7% and −2%**. That gap is the usual one: the
descriptor's change removes instructions, and the blur's removes a copy that
was living in the first-level cache, where at eight threads the sibling
thread takes the slot anyway.

End to end, **−2.5% of the CPU** on the 636-file subset (11 alternating
pairs, both orderings, 70.63 s against 68.88 s in the mean). On the two full
corpora the effect is inside this machine's drift and the pairs are quoted
rather than averaged: the benchmark corpus at 670 -> 711, 720 -> 729, 819 <-
759, 819 <- 783 CPU-seconds and the found one at 1,602 -> 1,589 and 1,661 <-
1,563, where `<-` marks the pairs run in the other order. Over a session
these runs drifted 22% on the *same* binary, and whichever build ran second
in a pair measured about 5% slower, which is larger than what is being
measured; reversing the order and averaging the two gives −1% and −3%. The
per-stage figures above are the ones to believe.

**What is left in the blur is the two planes it writes, not the filtering.**
Worth knowing before anyone reaches for it again: at 640x480 with r=10 a
`blur_dog` costs **5.31 ns a pixel and a `blur` without the difference costs
3.75**, so the second plane is 29% of the call — and at 320x240, where both
planes stay in cache, the two are **4.08 and 3.94**, which is to say the
difference costs nothing at all. The filtering itself is 4x unrolled `ymm`
and cannot use an FMA, since `acc + (a + b) * kv` in one rounding is not the
float the two roundings give. So the remaining lever is the write-allocate
traffic on `dst` and `dog`, which means non-temporal stores, which means
alignment and `std::arch` — and the next blur reads `dst` straight back, so
some of what a streaming store saves it would pay again.


**A fifth pass, aimed at the descent's memory and three loops around it.** The
first change here is **not byte-identical** — it rounds the vocabulary's centres
— so unlike every pass above it, its accuracy is measured rather than asserted.
On the benchmark corpus: F1 **0.9775 -> 0.9777**, precision 99.52% either way,
recall 96.05% -> 96.09%, false pairs **1,088 either way and every one of them
still a trap**, cross-family pairs **0 -> 0**, and the per-transform table gains
a row (`video_call_frame` 61/62 -> 62/62) and loses none. The found corpus goes
from 4,581 pairs to 4,578, with 301 dropped and 298 gained: a 6% churn in the
pair set for a net of minus three, which is what a changed vocabulary looks like
on a corpus whose matches are mostly marginal. The other three changes are
byte-identical on both corpora, checked against the run before them.

- **The tree's centres are bytes, and the descent measures them as integers.**
  This is the whole of the pass and the rest is trimming. The descent reads
  three paths' worth of a node's children per level — up to sixteen centres of
  512 bytes each — out of a tree whose deep levels are 30 and 78 MB on this
  corpus, and it does that for every descriptor in the corpus and three times
  more for every image the second look re-asks. It was never waiting for its
  arithmetic. The new `quantise_threads` is what says so: against a 93 MB tree,
  float centres cost **7,460 ns a descriptor on one core and 2,943 on eight**
  — a per-thread penalty of **3.16**, where the same descent against a 2 MB
  tree paid **2.12**, and the gap between those two is the memory. With byte
  centres the penalty is **1.88 at every tree size**, which is to say the
  memory is no longer in the way at all, and the eight-core figure is **903
  ns**. `quantise_depth`'s cost per child-distance, which used to read 19 ns
  against a tree that fits in cache and 31 ns against one of 40 MB, is now flat
  at **15.8 either way**.

  **The kernel is what makes it work, and it is why this failed when it was
  tried before.** Widening bytes to floats per *dimension*, in a block where a
  node's children are interleaved, costs more than the traffic saves — that is
  the measurement in *Tried and rejected*, and it stands. Child-major instead,
  so a centre is 128 contiguous bytes and two cache lines: `|a - b|` from two
  saturating subtractions, sixteen-bit lanes, and `_mm256_madd_epi16` to square
  and sum adjacent pairs. That is the same instruction count as the float kernel
  for a quarter of the bytes, and the sum is over integers — exact in any order,
  bounded by 8.3 M, and the portable fallback gives the same number the vector
  path gives. The width dispatch the float kernel needed is gone with it: a
  child's distance no longer cares how many siblings it has.

  In place, profiled cold: `quantise` plus `variant:quantise` go from **522 to
  241 CPU-seconds** on the found corpus and from **71 to 31** here, which is
  -54% and -56% of the descent. The tree also shrinks from 108 MB to 27 MB. That
  does not move the peak — the peak is set by the decode budget — but it does
  mean the whole vocabulary now fits in the memory one large decode used to
  hold, and the found corpus's measured peak came down 8% with it.

- **The word-list intersection asks eight words at a time whether it needs to
  look at all.** Galloping, the previous winner, reduced the *steps* of the
  merge and left every one of its branches exactly as unpredictable, which is
  why it was worth 4%: the function was never bound by its comparisons but by
  being wrong about them. Two images' lists hold about 1,300 entries each over a
  million-odd words and share some tens, so the answer to "do the next eight
  words of each side have anything in common" is almost always no, and one
  vector of eight all-against-eight comparisons gives it in about one
  instruction per word. The side whose block ends first then goes whole, and a
  scalar merge takes over only for the block pair that does meet. The words come
  out in the same order, so `cap` cuts in the same place, and
  `the_block_filter_intersects_exactly_as_a_merge_does` holds the pair together
  over every shape — empty lists, lists shorter than a block, runs on one side
  and both, and a cap small enough to bite. Isolated (`shared_timings`, 1,300
  entries over 1.7 M words): **13.9 microseconds a call -> 2.4**. In place on
  the found corpus, where it is called five million times, measured on cached
  runs with nothing else changed: the direct verification phase **7.0 -> 4.9
  seconds** of wall and the second look **106 -> 77**. Profiled cold, where the
  vocabulary change is in the figure too, `shared` reads **215 -> 137
  CPU-seconds** — a long way short of the isolated 5.8x, because a call on real
  lists is a merge *and* a sort of what the merge emitted, and only the merge got
  faster.

  Two benches were added to keep the next attempt honest, because the first one
  measured the wrong thing twice. `shared_pool` runs the intersection against a
  pool too large to cache — 85 MB of word lists — and says the cost is the same
  as against 1 MB, so the function is not waiting for the lists. `shared_overlap`
  varies how much the two sides share, and says the cost doubles from 40 shared
  words to 300: past that the emitted pairs and the sort at the end dominate,
  and a call on a real corpus is a merge plus a sort in roughly equal parts.

- **The geometry stage stopped recording which correspondences a losing
  hypothesis explained.** `count_inliers` wrote one byte per correspondence, and
  that scattered store was the only narrow thing in a loop that otherwise maps,
  subtracts and compares eight positions at a time. The caller reads the mask
  for the hypothesis it keeps and for no other, and a hypothesis is kept only
  when it beats every one before it, so `mark_inliers` takes a second pass over
  the handful that win and the thousands that lose are only counted.

- **The grey reduction de-interleaves explicitly.** `grey_of` is a few integer
  operations and one division, and a whole row of them is nothing a compiler
  cannot run eight at a time — except that a pixel's three bytes sit at `3x`,
  and a loop whose loads are that shape neither unrolls nor vectorises. It held
  the reduction to 6.2 cycles a pixel where the greyscale layout, the same loop
  with one load and no division, ran at 3.6. Undoing the interleave with a
  permute and a shuffle, eight pixels at a time, is bit for bit what the scalar
  form gives: the channels are summed as integers, which is exact, and the one
  division is the same IEEE division in a lane as in a register — as is the
  alpha blend, which must stay three roundings rather than becoming an FMA.
  Measured by `reduce_timings`: **rgb8 4000x3000 23.4 -> 15.5 ms**, **rgba8 35.4
  -> 19.1**, l8 13.7 -> 12.0. The `k == 1` path — 92% of this corpus's files but
  45% of its pixels, since the box factor only rises above one past 2,560 pixels
  — was already being vectorised by the compiler and is level.

**What the pass is worth end to end**, cold, with the order within each pair
reversed so that neither build always ran second — which is worth a few per cent
on its own here. Four runs of each build on the benchmark corpus and two on the
found one:

| benchmark corpus | CPU-seconds | wall |
|---|---|---|
| **before** | 722.6 / 722.3 / 719.7 / 728.2 | 114.2 / 113.4 / 112.4 / 113.4 s |
| **after** | 609.1 / 673.9 / 671.1 / 673.8 | 95.0 / 104.1 / 104.4 / 105.2 s |

| found corpus | CPU-seconds | wall | peak RSS |
|---|---|---|---|
| **before** | 1,506.5 / 1,516.0 | 219.1 / 219.2 s | 1,338 MB |
| **after** | 1,179.6 / 1,165.3 | 173.3 / 169.6 s | 1,232 MB |

That is **-7% of the CPU pair for pair on the benchmark corpus** and -9% on the
means, with -10% of the wall clock — and **-22% of both on the found one**,
where the descent is a third of the run rather than a twelfth. The found
corpus's four runs agree to within 1% on each build, which is the tightest
either corpus has measured in this file and is what a change this size looks
like when it is larger than the noise. The old build is remarkably steady — 723 ± 4 CPU-seconds over four
runs three hours apart — and the new one is not, for a reason worth knowing: its
fastest run is the one that started with the most memory free (peak RSS 1,024 MB
against 833), because the decode budget is a fraction of `MemAvailable` and a
larger budget keeps more decodes in flight. The same two binaries measured 653
and 666 CPU-seconds earlier the same evening, when the die was ten degrees
cooler, which is the usual warning about absolute figures on this machine.

**A sixth pass, over what a verdict costs when it is going to be thrown away.**
The second look takes **3.94 million verdicts to find 459 pairs** on the found
corpus, and one verdict in nine hundred reaches the ten aligned points an anchor
needs. So the question is not what a *match* costs but what a *near-miss* costs,
and the answer was: rather more than the geometry that decided it. All three
changes are byte-identical over both corpora — the same 227,927 pairs and 123
groups here, the same 4,578 and 864 there.

- **Two of the three tests a verdict faces ran before anything could reject
  it.** `encloses_centre` transforms every keypoint of both images — 1,200 of
  them — to ask whether the inliers bracket the middle of what the transform
  claims, and at the shape this pass really sees (56 candidate pairs, 51
  correspondences) it costs **3.5 microseconds against the geometry fit's 3.7**,
  measured by the new `match_timings`. It is read by one tier, for verdicts that
  clear every other bar. `distinct_inliers` likewise sorted a vector per verdict.
  Both now sit behind the bar that rejects 99.9% of them: `n_in` cannot exceed
  the number of correspondences the transform explains, and `best_transform`
  already knows that count, so a verdict that cannot reach `gate.0` aligned
  points returns before either test. `overlap` went behind the same bar for the
  same reason. The stages vanish from the profile: `encloses` **20.5 CPU-seconds
  to nothing**, `overlap` 4.1 to nothing.
- **The matcher's two cold gathers now ask ahead.** `correspond` is not bound by
  `dist2` — sixteen integer lanes measure 128 bytes in about twenty cycles — it
  is bound by *finding* the descriptor: `b`'s block is 76 KB, the candidates land
  in it at random, and on a corpus of nine thousand images none of it is in
  cache. Measured against a pool too large to cache, the same call costs two to
  three times what it costs warm. So the walk hands the next eight candidates'
  descriptors, and their keypoints, to the prefetcher while the current distance
  is still being summed, and `best_transform`'s coordinate gather does the same
  one pair ahead. Within-run ratios against the untouched descent:
  `correspond` **-26%**, `best_transform` **-20%**.
- **`best_transform` stopped allocating a mask per verdict.** It returned the
  inlier mask as a `Vec<bool>` of its own — an allocation, a copy and a free for
  a buffer the caller drops eight lines later, four million times. The mask stays
  in the scratch the function already has.

**What the three are worth, and how to measure a change this size at all.**
Everything here is in the matcher, so the honest measurement is a cached run —
that is what `--cache` is for — and nine of them, three per build in a Latin
square over the slots, separate the two halves of the pass:

| found corpus, cached | CPU-seconds | wall |
|---|---|---|
| **before** | 456.3 / 451.0 / 464.6 | 75.2 / 67.1 / 69.1 s |
| **the gate alone** | 446.3 / 442.2 / 450.8 | 68.1 / 68.9 / 67.1 s |
| **gate and prefetch** | 416.1 / 415.1 / 414.6 | 62.9 / 62.5 / 62.3 s |

That is **-2.4% for the gate and -9.2% for both**, with -11% of the wall clock,
and the three runs of the final build agree to 0.4%. Profiled, `encloses` and
`overlap` vanish, `correspond` reads -26% and `best_transform` -20% against the
untouched descent in the same run.

**The cold end-to-end runs cannot see it, and that is worth knowing too.** The
matcher is a third of a cold run on the found corpus and a fifth here, so -9% of
it is -3% and -2% respectively, against a spread of ±2% over four runs. Measured
anyway, balanced for slot: the found corpus **1,189.2 CPU-seconds before against
1,190.1 after** — level — and the benchmark corpus **667.7 against 658.2**, which
is -1.4% and about what the arithmetic predicts. A change worth three per cent of
a two-minute run is below this machine's resolution; the way to measure it is to
run the phase it lives in on its own.

**A seventh pass, over the addresses the matcher waits for and two numbers it
was working out again.** Every change is byte-identical on both corpora — the
same 217,380 pairs and 128 groups here, the same 4,578 pairs and 864 groups on
the found one, every verdict field and every representative compared — and what
they have in common is that none of them removes arithmetic. Five of the seven
change *when* a byte is asked for or *how many* bytes there are; two delete work
that was being repeated per pixel and could be done per row.

- **A word is now the leaf's centre slot, not its node number, and the leaves
  are one in ten.** The tree is trained on a sample of 160,000 descriptors, so
  no more than that many leaves can ever be live — and the numbering went up to
  `branching^depth`, which is 1,771,561 on the found corpus for 156,519 live
  leaves and 537,824 here for 139,691. Everything downstream is indexed by
  word, so the document frequencies, the idf and the posting offsets were three
  arrays of 21 MB, read at one scattered word per posting run, holding two
  megabytes of anything. Numbering the words densely is not an approximation
  and not even a change of order: centres are laid down parent by parent and,
  within a parent, child by child, so a leaf's slot rises with its node number,
  and every consumer of a word list reads only that order. The descent stopped
  needing `node_of` in its inner loop with it — the frontier carries the slot,
  and the node number is looked up for the three survivors of a level rather
  than for the hundred-odd children scored.
- **The descent asks for all three parents' children at once.** A level is
  three dependent misses deep — the slot's node number, that node's entry in
  `head`, the block of centres it points at — and the three parents' chains are
  independent. Walked a parent at a time they do not overlap, because a
  parent's distances are more instructions than the machine can look past. The
  `head` entries are read together now and each block's head is handed to the
  prefetcher while that is happening.
- **A query asks for a word's postings a few words early.** This was the
  query's whole memory problem: a run is a few hundred contiguous bytes
  somewhere in tens of megabytes, too short for the hardware prefetcher to lock
  on to before it ends, and its address is not known until `off[w]` has
  arrived. Two dependent trips to memory per word, a thousand words a query,
  35,592 queries on the found corpus.
- **The matcher's descriptor distance is the vocabulary's.** `correspond` kept
  a portable copy — sixteen `u32` lanes, which the compiler widens a dimension
  at a time — on the grounds that the loop around it waits for memory rather
  than for arithmetic. It does, and it also runs four million times in a second
  look; the byte kernel the fifth pass wrote for the descent is a quarter of
  the instructions for the same integer sum.
- **`shared` was paying for its output, not for its merge**, and it took
  `shared_overlap` to see it. The merge is two microseconds and does not care
  how much the two images have in common; the list it emits does, and the
  candidates a query returns are by construction the images sharing the most
  words with it. At 800 shared words the call is 34 microseconds and a
  comparison sort of the 3,190 pairs it emits is 66 of the 68 a single core
  charges for it. A keypoint index is smaller than `max_features`, so a pair is
  twenty bits and two counting passes put it in the same order — a
  least-significant-digit radix sort is stable and sorts on the whole key.
  Measured on lists of the shape this really sees, sort against radix: 56 pairs
  0.9 against 2.2 microseconds, 400 pairs 7.5 against 4.0, 1,200 pairs 23.6
  against 10.2, 3,200 pairs 66.5 against 23.8. So the crossing is near 150 and
  the shipped threshold is 192; below it, and for any pair index that does not
  fit the digit, the comparison sort still runs.
- **The gradient plane is not zeroed before it is written.**
  `vec![[0.0; 2]; w * h]` is a `calloc`, and a `calloc` of a chunk the
  allocator already holds is a `memset` — eight bytes a pixel wiped and then
  overwritten. An octave's three gradient planes are as large as the octave and
  the pyramid holds all of them at once for the description pass. The border is
  what the zeros were for and is written as zeros directly; everything else is
  written by the same indexed loop as before, which is the shape this has to
  keep — filling the planes by pushing rows was measured at 2.5x and is still
  under *Tried and rejected*.
- **The enlargement worked out its columns once per row.** Which two source
  columns an output column reads, and how far between them it sits, depends on
  the column and nothing else — a divide, a floor, two clamps and a
  subtraction, done again for every row of the picture. On the found corpus,
  where every image is enlarged, `sift:base` reads **58.8 CPU-seconds before
  and 42.3 after** in profiles either side of it.
- **And the pixel check did the same thing twice, with divisions in it.** A
  grid sample's column depends on `ix` alone and its row on `iy` alone, and so
  do the two products the transform takes of them, since `apply` is one term
  per axis and a constant. Both were worked out per sample, and the column
  carried `ix / 47` with it — 2,304 real divisions per call, and 576 more of
  `ix / 24` in the bounding-box probe above it, where a division is ten cycles
  and nothing else in the loop is. The terms are added in the order `apply`
  added them, because a probe that lands a bit either side of the frame is a
  different bounding box and not a rounder one. The whole-overlap correlation
  also stopped being a pass of its own: the blocks tile the grid exactly, so
  the same samples are summed a block at a time rather than a row at a time.
  That moves the last bits of `ncc`, which is reported and dumped and read by
  no rule.
- **The box reduction's inner loop got its factor as a constant.** `k` is a
  property of the picture, so the compiler could neither unroll the loop nor
  see that `ox * k + k` never leaves the row, and the sums ran one float at a
  time. Two, three and four are spelled out; the sums are the same sums in the
  same order, since `acc` started at zero and a grey value is never negative.

**What the pass is worth end to end**, cold, cooled to measured idle + 3 C
before each run and with the page cache evicted, in the order A B B A B A so
that neither build always runs second:

| found corpus | CPU-seconds | wall | peak PSS |
|---|---|---|---|
| **before** | 1,091.8 / 1,085.2 / 1,086.2 | 164.4 / 163.0 / 163.0 s | 1,209 / 1,209 / 1,206 MB |
| **after** | 1,009.1 / 1,010.1 / 1,013.0 | 149.8 / 150.7 / 151.0 s | 1,194 / 1,194 / 1,194 MB |

| benchmark corpus | CPU-seconds | wall | peak PSS |
|---|---|---|---|
| **before** | 426.8 / 400.5 / 400.3 | 68.0 / 63.2 / 62.9 s | 770 / 733 / 730 MB |
| **after** | 388.5 / 387.9 / 386.0 | 61.8 / 61.5 / 61.1 s | 767 / 868 / 739 MB |

That is **-7.1% of the CPU and -7.9% of the wall** on the found corpus, where
the six runs agree to better than 1% each and every one of them ran between
2,101 and 2,124 MHz — the tightest either corpus has measured in this file. On
the benchmark corpus it is **-3.3%** taking the two slot-matched pairs (387.9
against 400.5 and 386.0 against 400.3; the first `before` run is the session's
cold one, at 2,247 MHz against the others' 2,434 to 2,496, and quoting the
means instead would claim -5.3%), with **-2.6%** of the wall. The gap between
the two corpora is the obvious one: half of a benchmark run is inside the image
crate's decoders, and nothing here touches them.

**Memory is down where the words are and level everywhere else.** The found
corpus's peak PSS is 1,194 MB against 1,208, which is the 19 MB of posting
offsets, document frequencies and idf the dense numbering gives back, less the
radix scratch — half a megabyte a worker at the very worst, and far less in
practice. The benchmark corpus's PSS column is the usual decode-budget noise
and says nothing either way; the deterministic reading, `-t 1` on the 636-file
subset, is **264,120 and 263,964 kB before against 264,080 and 264,296 after**,
which is level to a tenth of a per cent.

**One measurement tool changed with it.** `--features prof` timed every stage
with `Instant::now()`, and the kernel can only serve that from the vDSO when
the clocksource is the TSC. This laptop's is the **HPET** — a memory-mapped
platform device shared by every core — so a clock read costs about 1.5
microseconds here and a `timed!` pair costs 2.9. That is longer than most of
what the table measures, and the charge lands in proportion to *call count*, so
the stages it named loudest were partly the ones called most often: `shared`,
`correspond` and `best_transform` are five million calls apiece on the found
corpus, which is some 45 CPU-seconds of clock reads in a run of a thousand.
`timed!` now takes `rdtsc` — twenty-odd cycles, no kernel, no lock — and the
report calibrates it against one wall-clock interval taken over the whole run.
The CPU says `constant_tsc` and `nonstop_tsc`, so it ticks at a fixed rate
whatever the core clock is doing. Check `current_clocksource` before trusting a
profile taken on another machine.

**An eighth pass, and the rule it was measured by: at eight threads this
machine charges for µops, not for waiting.** Every change is byte-identical on
both corpora — the same 217,376 pairs and 128 groups here, the same 4,576 pairs
and 862 groups on the found one, every verdict field compared — and the pass is
worth **-6.6% of the CPU and -7.3% of the wall on the found corpus**, **-4.6% and
-4.3% on this one**, and 48 MB of the found corpus's peak.

The rule first, because it decided what was worth finishing. The first three
changes below — the query, the descent's kernel, the pixel check — were
measured in place with `--features prof`, and the query's stage alone fell from
84 CPU-seconds to 43; an A/B of the three together took the found run down by
**27**. That is not a contradiction. A stage's CPU-seconds under `prof` are
thread-time, and thread-time includes waiting for memory; at eight threads the
other hyperthread runs while one waits, so a wait removed is mostly time the
core was already spending on someone else. What the run's CPU-seconds follow is
what the core *executes*: the query change alone removed some 35 G µops — a
branch, a compare and a push per posting, 8.7 billion postings — which at the
one to two G µops a CPU-second this chip manages on eight threads is most of
the 27. A stage table that halves is therefore a claim about stalls until an
end-to-end A/B says otherwise, and the A/B is the only figure quoted as the
pass's worth.

- **A posting is four bytes, and the query's inner loop is a read, a multiply
  and an add.** The found corpus's queries walk **8.7 billion postings** —
  245,000 a query, 35,583 queries — out of some fifty megabytes no cache
  holds. A posting was a pair of `u32`s; it is now the image in 24 bits and the
  count in 8, with the rare count past 254 kept in a side table (`wide`) and
  asked only when the query's own count is as large. The loop also tested every
  posting against the query image and against "never touched", to keep a
  touched list; the query image's slot is now cleared at the end and the
  touched images are found by one scan of the accumulator, `n` floats against a
  quarter of a million postings. A query word occurring once — most of them —
  adds `idf2` itself, since `1.0 * idf2` is `idf2`. Scores are the same products
  added in the same order. **One exception, kept exactly:** a word in every
  image has idf 0, which is indexable only when the posting cap reaches the
  corpus — 32 images or fewer — and there an image can be touched and score
  nothing; the old loop listed it and the second look verified it, so such an
  index takes the old loop (`query_touched`).
  `packed_postings_score_exactly_as_pairs_did` holds both paths against the
  old query, saturated counts and zero-idf words included.
- **The candidate ranking selects on integers.** A positive float's bits sort
  as the float does, so `(!score_bits << 32) | index` is one `u64` whose order
  is the comparator's; the selection over nine thousand candidates stops
  paying a `partial_cmp`, an `unwrap` and a tie-break per comparison. Any score
  a key cannot carry falls back to the comparator. `cand:rank` 14.2 -> 6.7
  CPU-seconds on the found corpus.
- **The descent measures `|q|^2 + |c|^2 - 2 q.c`.** `|c|^2` is stored beside
  each centre at build time and the widened query is taken once per
  descriptor, so a centre costs a load, a widening and a multiply-add, and four
  centres share one horizontal reduction. It is the same integer. And a level
  now opens with one dependent read rather than three: `down[l][slot]` is where
  a centre's children are, folded at build time out of `node_of` and `head`,
  and it is prefetched the moment a child takes a place in the frontier.
  `quantise_threads` now has a depth-6, branching-11 tree, which is the found
  corpus's shape: about **-10% at eight threads**, level on one, with the bench
  swinging nearly as much between runs — a direction, not a figure. A
  single-thread breakdown of that descent says why it is not more: the kernel is
  a quarter of it, the frontier's bookkeeping another quarter, and the rest is
  waiting for the deep levels. `query_distances_are_dist2` holds the kernel to
  `dist2` at both ends of the byte range.
- **The pixel check reads both thumbnails eight samples at a time.** Reading
  them was nearly all of the check — `pixel_timings`: some forty of fifty
  microseconds, three quarters of it the rotated B side — and the block
  statistics were noise. Two four-byte gathers per level per eight samples cover
  a bilinear tap's two rows (the top read starts at the top-left pixel, the
  bottom one ends at the bottom-right, and `split` never lets either leave the
  plane); the clamps are compares and blends that send a NaN where the scalar
  comparisons send it, and every product is rounded on its own. `pixel_timings`
  prints a checksum over every figure the check returns: unchanged, and
  **57 -> 29 microseconds** a call near identity. In place on this corpus,
  `pixel_check` **34.9 -> 20.7** CPU-seconds and `pixel_check:prop` 8.4 -> 4.4.
- **The octave's top Gaussian is not written.** It exists to make the last
  difference, carries no gradient and starts no octave, and it was written out
  and dropped unread (`blur_top`). At eight threads the blur is waiting on
  memory — `blur_threads` reads **x5.2** per thread against one, where a
  cache-resident descriptor reads x1.9 — so a plane not written is worth
  having: the octave's five blurs went **16.3-16.7 -> 14.6-14.8 ms** a thread.
- **The extremum sweep skips dead pixels eight at a time**, reading its flags
  as a word; a tenth of a row survives, and each dead pixel was a load and a
  branch. **The orientation histogram got the descriptor's split**: the weight
  argument and the bin are worked out for a row eight samples at a time, and
  only the table read and the add go one by one.

`extract_threads` is new, and it is the check to run on anything in `sift.rs`:
the whole extractor at the two shapes the corpora hand it, one core and all of
them, with a checksum over every keypoint field and descriptor byte. Both
changes above leave it at `48a0f69e907049e6` / `6efa669a9cb66fb9`.

**What the pass is worth end to end**, cold, cooled, cache evicted, in the
order A B B A A B:

| found corpus | CPU-seconds | wall | peak RSS |
|---|---|---|---|
| **before** | 971.3 / 954.5 / 948.3 | 142.9 / 138.7 / 138.9 s | 1,220 MB |
| **after** | 900.8 / 896.6 / 887.4 | 131.7 / 129.7 / 128.4 s | 1,172 MB |

| benchmark corpus | CPU-seconds | wall |
|---|---|---|
| **before** | 380.0 / 379.4 / 378.1 | 59.4 / 59.4 / 59.2 s |
| **after** | 367.8 / 354.2 / 363.0 | 57.6 / 56.3 / 56.5 s |

Slot-matched, the found corpus's three pairs are -7.3%, -6.1% and -6.4%, and
this corpus's -3.2%, -6.6% and -4.0% (CPU is user plus sys throughout). The gap
between the two is the usual one: half of a benchmark run is the image crate's
decoders, and a found run is a third retrieval, which is where most of this
pass landed. The same A/B taken after only the first three changes read -2.9%
and -4.8%, so the rest of the list is worth about four points on the found
corpus and nothing this corpus can resolve.

**Threads, which is the largest lever in this file and not a change to it.**
The same binary on this corpus at `-t 4` costs **247 CPU-seconds against 375
at `-t 8`** (two runs each, cooled: 251.1 / 242.8 against 367.7 / 383.5), for
**+15% of the wall** (68.3 s against 59.1). Four physical cores under a power
cap: the second hyperthread on each buys a sixth more throughput and is
charged as a whole second CPU. So "how many CPU-seconds does a run take" and
"how long does a run take" have different best answers here, and the default
(`-t 0`, every logical core) is the answer to the second.

Tried in this pass and rejected, each measured:

- *The descriptor's trilinear corners, eight samples at a time*, leaving the
  scatter nothing but adds. Exact, and no faster at one thread or eight:
  the scatter is bound by its **eight stores a sample** on a core that retires
  one a cycle, not by the fourteen operations that feed them — which is what
  the entry below on four-at-a-time weights found from the other side.
- *Holding the eight bins a sample adds into in registers* while consecutive
  samples share them, writing back only when the set changes — exact, since
  every bin receives the same additions in the same order. **13% slower**
  single-threaded: consecutive samples change bins more often than the
  store-forwarding chain it removes costs.
- *Computing gradients lazily*, only where a keypoint's window reads them.
  Counted before building it: windows cover **69% of the gradient planes' 16x16
  tiles here and 77% on the found corpus** (85% and 97% of rows), so the most it
  could save is some eighteen CPU-seconds across both, for keeping every
  Gaussian alive through description.
- *glibc tunables* (`trim_threshold` 1 GB, `mmap_threshold` 32 MB): minor
  faults down a quarter, sys time down 2-3 s of a 370 s run, inside the noise.
- *Dropping the mirror+invert variant* from the second look, since the corpus
  has no mirrored negative. It costs **3.2 points of recall** (F1 0.9547 ->
  0.9368, 44 perfect transforms against 47) — see *What the second look costs*.

### What the second look costs, and the seven ways not to fix it

It is **41% of a found corpus's run** and 1.5% of this one's. Profiled on the
found corpus after the sixth pass below, its 431 CPU-seconds divide as **the
descent 150, the word-list intersections 85, the geometry and its
correspondences 86, retrieval 55**, and the rest is ranking, the permutation
itself and the loop. The shape behind those
numbers is worth stating, because every attempt to cut the pass runs into it:
**3.94 million verdicts to find 459 pairs**, each verdict over a median of 56
candidate keypoint pairs spanning 48 distinct query keypoints, of which
**4,551 — one in nine hundred — reach ten aligned points**. The work is spread
thinly over an enormous number of pairs that are nearly, but not quite, nothing.

Seven cuts have been measured and every one of them is worse than the cost:

- **Narrowing its descent.** The second look's query does not need the same
  breadth as the index it queries, since the index side already multi-assigns
  — or so it seemed. Swept, `max_paths` 3 -> 2 -> 1 takes the found corpus's
  mirror anchors from **467 to 283 to 151** while the benchmark corpus barely
  notices (5,774 -> 5,726 -> 5,410). Those matches are marginal and they need
  every path there is.
- **Dropping a variant.** The two corpora disagree completely on which one
  pays: **all 467** of the found corpus's anchors come from `mirror`, and
  `invert` and `mirror+invert` yield **zero**; on this corpus `invert` yields
  **4,915 of 5,774** and mirror 276. Neither can go, and a corpus that
  measured only one of them would have concluded otherwise. Nor can the third,
  though the catalogue has no mirrored negative: asking only `mirror` and
  `invert` costs **3.2 points of recall** here (F1 0.9547 -> 0.9368, 47
  perfect transforms -> 44), which is a third of the pass's cost on the found
  corpus bought back at a price this corpus will not pay.
- **Cutting its candidate list.** On the found corpus only **40 of 467**
  anchors sit beyond rank 25, so `-k 25` would cost almost nothing there. On
  this one **3,678 of 5,774** do, at a median rank of 40 — because an inverted
  file matches its whole family and the deep ranks are the rest of the family.

- **An exact ceiling on the inlier count**, carried from retrieval into
  verification, and a **second on the correspondence count**. Both are free and
  neither fires. The retrieval ceiling is under *Tried and rejected*; the other
  is simpler still — a pair cannot reach ten aligned points with fewer than ten
  correspondences, so `verify` could stop before the geometry. Measured on the
  found corpus: **0.0% of the 3.94 M variant verdicts have fewer than ten**, and
  the mean is 47.7. Both ceilings fail for the same reason, and it is the reason
  to stop looking for a third: the top hundred and fifty candidates are *by
  construction* the images sharing the most words, so everything cheap about them
  is already large. What separates a duplicate from a butterfly is the geometry,
  and the geometry is the expensive part.
- **Dropping variant words the corpus does not contain.** A word no image holds
  can intersect with nothing, so removing it from a variant's word list is
  exactly equivalent and makes every one of its 150 intersections shorter.
  Measured: **477 of 40,861,489** variant word entries — 0.001%. With 160,000
  live words over five million descriptors, a leaf is never empty.
- **Relabelling the words instead of descending for them.** This is the one that
  looked like the answer, and it is the most instructive failure here. The
  descent for permuted descriptors is the pass's largest item; the permutations
  are fixed; so for every live leaf, permute *its own centre*, descend that once
  at build time, and a variant's word list becomes the image's own list read
  through a 160,000-entry table — 30 microseconds against 6.8 milliseconds, and
  the descriptor distances and the geometry still run on the real permuted
  descriptors. The reasoning for why it should be safe was that a word only has
  to bring a pair into the candidate list and hand the geometry some keypoint
  pairs to test, and a descriptor whose permutation lands in a different leaf
  than its permuted centre did costs one correspondence out of hundreds.
  **It costs 21% of the mirrored pass's pairs**: 5,792 -> 4,584 on this corpus,
  F1 **0.9777 -> 0.9770**, TP 223,780 -> 223,449, and five transformations drop
  out of the perfect column (`collage_cell`, `keystone_side`, `pdf_page`,
  `photo_of_screen`, `video_call_frame`) against two that join it. So the words
  are not merely a candidate filter: the pairs this pass exists to find are the
  marginal ones, and a marginal pair needs most of its correspondences, not some
  of them. The exact form of the idea — a vocabulary whose centres are *closed*
  under the three permutations, so that the relabelling is not an approximation
  at all — is untouched by this result and is written up in
  *An equivariant vocabulary* below — where it is measured and closed, for a
  reason that is about photographs rather than about the idea.

**The descent is no longer bandwidth-bound**, which is what the fifth pass was
about: byte centres and an integer kernel took it from waiting on 31 ns a
child-distance to a flat 15.8 in cache or out. What is left of it is arithmetic,
and the only way to remove that is to stop asking for it — which is what the
last two entries above are both trying to do.

### An equivariant vocabulary, and the measurement that closes it

The mirrored pass's largest single item is the **descent for permuted
descriptors** — 150 of its 431 CPU-seconds on the found corpus — and it exists
only because quantisation is not equivariant: the word of `M(d)` has nothing to
do with the word of `d`, so 600 permuted descriptors have to be descended per
image per variant. There is an exact way to remove it, it is elegant, and it
does not work here. Both halves are worth writing down.

**The three permutations form a group.** `mirror` and `invert` are involutions
on the descriptor's 128 bins and they commute — `mirror` flips the grid's rows
and negates the orientation bins, `invert` flips both axes and leaves the
orientations — so with the identity and their composition they are `Z2 x Z2`, a
group `G` of four elements, every one of them a permutation of coordinates and
therefore an **isometry**: `||P(d) - P(c)|| = ||d - c||`.

That is the lever. If the *set* of centres at each level of the tree is closed
under `G`, the nearest centre to `P(d)` is the `P`-image of the nearest centre to
`d`, at every level, and therefore `word(P(d)) = P̂(word(d))` for a permutation
`P̂` of the leaf numbers that the build knows exactly. A variant's word list
becomes the image's own list read through a table — 30 microseconds against 6.8
milliseconds — with no approximation, and the multi-path set survives too, since
the distances are identical. The build would follow: the root is the only fixed
point of `G`, so its children are `branching / 4` k-means centres and their four
images each (16 and 12 are both divisible by four); every deeper node sits in an
orbit of four, so only representatives are k-meansed and their siblings' centres
are the permuted copies, each representative trained on
`∪_g g⁻¹ . members(g . r)` — its orbit's members mapped back into its own frame,
which sums to the sample size over a level, so the build costs what it costs now.

**And the corpus will not have it.** An equivariant tree has as many live leaves
as the tree it replaces, but they come in orbits of four, so it is fitted to the
*symmetrised* descriptor distribution rather than the real one. That is testable
without building any of it: extend the sampling pool with the permuted images of
every descriptor it holds and leave everything else alone. Measured on the
benchmark corpus, against the shipped 0.9777 / 96.05% / **0 cross-family**:

| sampling pool | F1 | recall | TP | cross-family FP |
|---|---|---|---|---|
| **as shipped** | **0.9777** | **96.05%** | **223,780** | **0** |
| + all three permutations | 0.9760 | 95.86% | 223,235 | **42** |
| + `mirror` only | 0.9764 | 95.81% | 223,108 | 0 |
| + `invert` only | 0.9771 | 95.93% | 223,413 | 0 |

The full group **merges families** — 42 cross-family pairs where the shipped
build makes none — and even the half of it that natural-image statistics make
plausible costs 0.24 points of recall. The reason is the one *How img-fp works*
already documents about occupancy: a vocabulary fitted to a distribution the
corpus does not have spends its words on regions the corpus does not occupy, so
the words that carry the corpus get coarser, and coarse words match two
different beaches along what they share. Descriptors are simply not
`G`-symmetric in distribution — `invert` least of all, which is no surprise,
since natural images are not symmetric in intensity.

So the second look keeps descending, and the reason it must is a property of
photographs rather than of this code. Anyone reaching for this again should
re-run the three rows above first; they cost one cached run each.

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
  depend on it. It could also *hang*, for a reason that has nothing to do with
  the queue; see *The decode budget could hang* below.
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
  that to 109 MB, and is most of the memory saving. (A quarter of that again
  since the centres became bytes — 27 MB — see the fifth pass.)
- **Distances were measured one centre at a time.** Both quantisation and
  k-means walked a descriptor against a node's sixteen children in turn, and
  each of those is a chain of 128 dependent adds: one of the machine's several
  adders busy. A parent's centres were laid out dimension-major so that all
  sixteen sums ran at once, each still taken over the dimensions in order and so
  the same float to the bit. Quantisation went from 24 s to 7 s. (That layout
  is now k-means' alone: the *descent* reads bytes, and a byte centre is short
  enough that one child at a time in sixteen integer lanes beats sixteen
  children at a time in floats — see the fifth pass.)
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

- *Narrowing the second look's descent*, *dropping one of its three variants*
  and *cutting its candidate list* — all three measured, all three worse than
  they look. The numbers are under *What the second look costs* above, with the
  accuracy each would have cost.
- *Byte centres in the vocabulary descent* was here too, and **it is now what
  ships** — see the fifth pass under *Speed and memory*. The entry is worth
  keeping as a lesson about where a measurement was taken rather than about the
  change: it was tried in a *dimension-major* block, where a node's sixteen
  children are interleaved and widening them means unpacking sixteen bytes per
  dimension, and it was measured end-to-end on this corpus, where the descent
  is 9% of the run. Both halves of that hid it. Child-major with a pairwise
  multiply-add is the same instruction count as the float kernel, and the
  corpus where the descent is 31% of the run is the found one. The old
  conclusion — "whatever cuts those bytes has to keep the floats" — was exactly
  backwards; what it has to keep is the *width* of the widening.
- *Pruning the descent against a partial distance.* Written against the old
  dimension-major float block, where the first quarter of the dimensions was
  the first quarter of the block, so scoring that quarter for all of a node's
  children gave an exact lower bound on every one of them — the terms still to
  be added are squares — and a node whose closest child already lost could be
  abandoned unread. It worked, it was byte-identical, and it bought nothing.
  The reason is worth keeping: the bound is a *quarter* of a distance tested
  against a *whole* one, so a node has to be four times worse to be dropped
  after the first quarter and a third worse after the third, and nodes that bad
  are rare — the multi-path descent exists precisely because the winner is often
  not under the nearest parent. Measured on the isolated descent it was level at
  some widths and **up to 12% slower** at others, the minimum-over-lanes
  costing more than the blocks it saved. The layout has since changed and a
  child's bytes are now contiguous, so the same idea could abandon a *child*
  rather than a node — but the descent is no longer waiting for memory, which
  is the only thing such a bound saves.
- *A ceiling on the inlier count, carried from retrieval into verification.*
  There is an exact one and it is nearly free: sum, over the query's words that
  a candidate also holds, of how many of the query's keypoints carry that word.
  Every correspondence `shared` can offer is such a keypoint, `correspond`
  keeps one per keypoint and `distinct_inliers` one per position, so
  `n_in <= that sum` — and a tier wanting more aligned points than the ceiling
  can never accept the pair, whatever the pixels say. It cost one `u32` per
  slot in the query's accumulator and it **skipped nothing**: on the found
  corpus, 0 of 1,077,507 candidate pairs and 0 of 3,946,050 second-look
  candidates fell below the anchor's ten, and on this corpus 30,950 of 592,691
  fell below the corroborated tier's eight — 5%, and those are the cheapest
  pairs in the set. The reason is worth keeping, because it is about retrieval
  rather than about the bound: the top hundred and fifty candidates are *by
  construction* the images sharing the most words, and sharing ten word
  incidences is a far weaker condition than agreeing on one transform. The
  words a pair shares are simply not scarce enough to be evidence. (The
  heavy-word leak the ceiling has to cover — words too common to index, which
  `shared` finds and the query cannot — turned out to be empty: the median
  query carries none, and on the found corpus not one of 9,285 carries any.)
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
- *And the trilinear weights that feed them, four at a time.* The eight
  corners are a magnitude split between two rows, then two columns, then two
  orientations, so the last two levels are a four-element multiply and a
  four-element subtraction — written as arrays rather than as fourteen named
  scalars, which is the same products and the same differences in the same
  order and ought to let the compiler use one register for each level. Three
  alternating pairs on the isolated descriptor: 22.44 / 23.63 / 24.46 ms
  against 23.22 / 22.92 / 22.40. It wins one pair of three and the run was
  heating; there is nothing there. The scatter is not bound by the arithmetic
  that feeds it.
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

  It times with **`rdtsc`**, not with the clock, and that is not a micro-
  optimisation: this machine's `current_clocksource` is `hpet`, the kernel
  serves `clock_gettime` from the vDSO only for the TSC, and a clock read here
  therefore costs about 1.5 microseconds — 2.9 for the pair `timed!` takes.
  That is longer than most of what the table measures and it is charged in
  proportion to *call count*, so a stage called five million times was being
  charged fifteen seconds of clock reads. `rdtsc` is twenty-odd cycles and the
  CPU reports `constant_tsc` and `nonstop_tsc`, so one calibration against the
  wall at the end of the run turns cycles into seconds. Read
  `/sys/devices/system/cpu/.../current_clocksource` before trusting a profile
  taken anywhere else; on a TSC clocksource the old form was fine.
- **`cargo test --release -- --ignored --nocapture`** runs benchmarks of the
  inner loops on synthetic data, in a few seconds each: `kernel_timings` (blur,
  extract, descriptor, orientation histogram, gradient), `reduce_timings` (the
  grey reduction at each channel layout and box factor, and the area
  resampler), `quantise_timings`, `quantise_depth` and `quantise_branching`
  (the vocabulary descent against tree size and branching), and
  `shared_timings` (the word-list intersection) and `match_timings` (the
  per-candidate half of the matcher: correspondence, geometry and the enclosure
  test, at the shape the second look really asks for). They report the *fastest*
  of several runs, because the slow ones belong to the machine. Use these to
  decide whether a change is worth a corpus run at all — three of the four
  rejections above were settled here in a minute apiece.

  **Three more exist because a single-core bench on small data hid something
  twice.** `quantise_threads` runs the descent on one core and on all of them,
  because at eight threads the descent is mostly waiting for memory and one core
  cannot see that: the fifth pass above is **3.3x** against the tree a real
  corpus builds at eight threads, 1.9x on one core, and 1.5x against a tree
  small enough to cache — and its first, scalar version was *slower* in cache
  while already being much faster out of it, which is the shape that got byte
  centres rejected the first time.
  `shared_pool` runs the word-list intersection against 85 MB of lists rather
  than 1 MB, and `shared_overlap` varies how much the two sides share — the
  original bench's lists shared one word where a real pair shares tens, so it
  was measuring a function the matcher does not call.
- **`objdump -d`** on the release binary, after an `#[inline(never)]`, when
  the question is "did that actually vectorise". `perf` does not work on this
  box (`perf_event_paranoid` is 4) and neither does attaching a profiler
  (`ptrace_scope` is 1), so the instruction mix in the listing is the only
  direct evidence available.

**Peak memory cannot be measured in one run at `-t 8`** — see the paragraph on
it above. Use `-t 1`, which is deterministic, to see whether a change reduced
what the program holds, and take several `-t 8` runs to see whether it matters.
Note also that making *extraction* faster raises the eight-thread peak on its
own, because each worker then spends a larger fraction of its time holding a
decode buffer; that is what the decode budget is for, and why its claims have
to cover everything a decode holds.

### The decode budget could hang, and the shape that did it

**One run in twenty-five deadlocked**, and the mechanism took enough chasing
that it is written down here rather than left in a commit message. A 0.2.0 run
on the benchmark corpus stopped dead: all nine threads parked in
`futex_wait`, zero CPU for the thirty-five minutes before it was killed,
174 MB of the process swapped out — on the run of that session which started
with the least memory free (1,952 MB).

The budget's queue is not at fault. Twenty thousand claims at a 4 MB budget
with 3 MB claims, eight threads, every claim therefore serialised, never
hangs. What hangs is one thread claiming the budget **twice**:

```
decode -> decode_jxl -> jxl_oxide::render_frame
       -> jxl_color::ColorTransform::run_with_threads
       -> JxlThreadPool::for_each_vec        dispatches to the GLOBAL rayon pool
       -> rayon WorkerThread::wait_until     waits for its own sub-tasks
       -> WorkerThread::execute              and steals a job while it waits
       -> img_fp::analyse -> decode -> reserve    claims 39.6 MB for another
                                                  file, holding the first claim
```

`jxl-oxide` has `default = ["rayon"]` and its default pool is `rayon_global()`
— the same pool the per-image walk runs on. A decode dispatching there leaves
its own worker free to steal, rayon hands it another image, and that image
decodes. `reserve` grants when `held == 0 || held + want <= limit`, and this
thread *is* `held`, so a second claim that does not fit waits for room only it
can give back, with every later ticket queued behind it. The budget is
`MemAvailable / 8`, so on a tight machine almost nothing fits — which is how a
rare re-entrancy became a certain hang on the tightest run of the day.

Rare, because three things must coincide: a colour transform dispatching to the
pool, rayon stealing an *outer* task at that moment, and the stolen task's
claim not fitting. Instrumented to report re-entrant claims, the benchmark
corpus produced **one in three runs**; the hang itself, one in twenty-five.

Two changes, both in `decode.rs`:

- **`decode_jxl` gets a pool of its own** (`JxlThreadPool::none()`). Nothing
  here wants a second level of parallelism — the images are already one per
  thread, and JXL is 62 files of 5,638.
- **A thread already holding a claim never queues.** It takes what it asks for
  and goes, because the room it would wait for is room it is itself holding, so
  waiting can only deadlock; the overshoot is bounded by the nesting depth, and
  a budget that hangs is worse than a budget briefly exceeded. That guard is
  the part worth keeping, because the rule generalises: **any decoder that runs
  its work on the global rayon pool can do this**, and it fails as a hang with
  no CPU, no message and nothing in the output.

Verified byte-identical on both corpora — the same 227,838 pairs and 122 groups
here, the same 4,581 pairs and 874 groups on the found one, every verdict field
compared, representatives included. Level on the clock: three cooled pairs at
`-t 8` put it at **-1.8% of CPU** once the second-slot penalty is fitted out
(which was **12%** that day, larger than the thing being measured), and the
found corpus — which contains no JXL file at all and so cannot be affected —
reads **-2.1%**, which is the honest size of this machine's noise after
pairing. That corpus was then measured a third way, because two uncooled pairs
on it had read **+6%** and **+8%** and a corpus the change cannot touch is the
one place a regression cannot be real: a cooled, cache-evicted profiled pair
puts it at **1,549.4 CPU-seconds against 1,557.5**, wall 225.1 against 226.6,
peak RSS 304 kB lower, with the per-stage table **9 stages up and 10 down
inside ±5%**, several of them stages whose machine code cannot have changed at
all. The lesson there is the protocol rather than the fix: an uncooled second
run on this machine invents an 8% regression out of nothing.

Peak RSS at `-t 1`, the deterministic reading, is **790,336 kB against
790,508** at matched `MemAvailable`; the fourth run of that set reads 776,240
and started with 330 MB less available, which is the budget moving, not the
build.

### Tuning discipline

`--dump` writes every verdict considered, accepted or not, as CSV. Fit
thresholds against that offline instead of re-running the tool per guess. The
cache is on by default and is what makes tuning the matching stages practical:
on the 5,638-image corpus a cold run at the default is ~57 s and a cached one
~12 s (at 640, ~82 s and ~17 s), and the cache is keyed on the extraction
settings so changing `--work-size` invalidates it correctly — which also means one cached extraction serves a
whole threshold sweep at a given work size, and that is how the 384 sweeps in
*Measured trade-offs* were taken. A sweep's runs after the first also write
nothing: an unchanged record set is not rewritten.

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

Measured trade-offs, so they need not be rediscovered. **`--work-size` is a
knee, not a peak**, and the shipped default is deliberately below it. Re-swept
on this build, all six sizes in **one session** on `bench.py`'s own protocol —
cold page cache, cooled to measured idle + 3 C before each — and run in the
order 384, 896, 448, 768, 512, 640, so that the session's thermal drift does
not track the thing being measured:

| `--work-size` | F1 | precision | recall | wall | CPU | peak PSS | cross-family |
|---|---|---|---|---|---|---|---|
| **384 (default)** | **0.9547** | **99.59%** | **91.68%** | **57.4 s** | **360 s** | **697 MB** | **0** |
| 448 | 0.9658 | 99.59% | 93.75% | 60.7 s | 413 s | 765 MB | 0 |
| 512 | 0.9724 | 99.55% | 95.03% | 69.4 s | 476 s | 737 MB | 0 |
| **640 (the knee)** | **0.9777** | **99.52%** | **96.09%** | **82.4 s** | **579 s** | **849 MB** | **0** |
| 768 | 0.9783 | 99.55% | 96.18% | 95.7 s | 670 s | 896 MB | 0 |
| 896 | 0.9729 | 98.52% | 96.09% | 126.4 s | 841 s | 1,041 MB | **2,207** |

CPU is the column to read: it is the steadier of the two, and the wall column
carries the session — 896 ran second, at a mean 2,137 MHz against 384's 2,765,
so some of its 126 s is the die rather than the work. Cost is near enough
linear in the long side. F1 climbs steeply to 640 and then stops: 640 -> 768
buys **0.0006** for 16% more CPU, and above that it goes backwards. Downwards
it is expensive but bounded — 512 costs 0.005, 448 costs 0.012, 384 costs
**0.023**, and every bit of it is recall.

**The default is 384 and the knee is 640, which is a choice rather than a
measurement.** What 384 buys is 38% of the CPU, 30% of the wall and 18% of the
peak memory; what it costs is 4.4 points of recall, concentrated in the
containment rows — see the header. The knee is where to go when recall matters
more than the wait, and nothing above the knee is ever worth asking for.

**There *is* a cliff, and it is at the top rather than the bottom.** This
reverses what stood here before. `--work-size` sets the descriptor count, which
sizes the vocabulary, and the note in *How img-fp works* says that route can
merge families; the previous sweep found zero cross-family pairs at every size
and concluded it had not fired. On this build **896 fires it**: precision
98.52%, **2,207 cross-family pairs and two merged families** — `beach` with
`panoramic3.jpg`, which is the "two different beaches match along what they
share" failure exactly, plus one stray `city2.jpg`/`earth.jpeg` pair. It
reproduces to the pair over two runs (230,196 pairs both times), and it is not
the occupancy rule failing: `for_corpus` holds occupancy between 1.8 and 2.7
across this whole range, so the extra descriptors are buying real matches
between genuinely similar photographs rather than a coarser vocabulary. 768 is
clean, 640 is clean, and everything below is clean — and by the one margin
measurement that sees behind the output, 384 is the cleaner of the two sizes
whose anchors were counted: **one cross-family anchor for the bridge test to
absorb against 640's three**. So the direction the default moved is the safe
direction, and the sizes to re-check after any change to
`VocabParams::for_corpus` are the large ones.


**What moving the default did to the other four, which is nothing.** Every
result-changing option was re-swept at 384, on one extraction cached once, so
each point is a 12 s run rather than a 57 s one. None of them moves, and two of
them for a better reason than "the sweep says so".

| `--min-aligned-points` | F1 | precision | recall | FP | trap | cross | merges |
|---|---|---|---|---|---|---|---|
| 6 | 0.9754 | 99.59% | 95.58% | 922 | 918 | **4** | 2 |
| 7 | 0.9720 | 99.59% | 94.92% | 902 | 900 | **2** | 2 |
| 8 | **0.9656** | 99.61% | 93.70% | 856 | 856 | 0 | 0 |
| 9 | 0.9604 | 99.62% | 92.70% | 827 | 827 | 0 | 0 |
| **10** | **0.9547** | **99.59%** | **91.68%** | **871** | **871** | **0** | **0** |
| 11 | 0.9493 | 99.60% | 90.68% | 841 | 841 | 0 | 0 |
| 12 | 0.9434 | 99.65% | 89.56% | 731 | 731 | 0 | 0 |

The shape is the one the ws-384 column of the big table above already showed,
and the argument is unchanged: F1 prefers **8**, by 1.09 points, and 8 is one
step from a cliff that starts at 7. The price of the margin roughly triples at
this work size — 2.02 points of recall against 0.69 at 640 — which is the one
thing that could have justified moving it, and the replay says not to. Counting
cross-family *anchors* before `drop_weak_bridges`, the measurement that decided
this value at 640 and the only one that does not go through F1:

| bar | anchors | cross-family anchors | after the bridge test | families merged |
|---|---|---|---|---|
| 6 | 182,266 | 14 | 2 | **2** |
| 7 | 179,620 | 9 | 2 | **2** |
| 8 | 177,003 | **3** | 0 | 0 |
| 9 | 174,410 | 3 | 0 | 0 |
| **10** | **172,125** | **1** | **0** | **0** |
| 12 | 167,466 | 1 | 0 | 0 |

**8 leaves three cross-family anchors for the bridge test to absorb where 10
leaves one**, and the bridge test survives exactly one false edge. That is the
same 3:1 shape it had at 640 (6 against 3 there), so a 40% cut in the
descriptor count did not change what the margin is worth. The replay
reproduces the runs' own merge counts exactly — 2 at bar 6 and 7, none from 8
up — which is what makes it worth believing.

**`--min-pixel-correlation`'s cliff is gone at 384, and this is the one
conclusion the new default changes.** At 640 it merges families at 0.40 and
catastrophically at 0.30, which is why it ships one step above its cliff. At
384 there is no cliff in the swept range at all:

| `--min-pixel-correlation` | F1 | precision | recall | FP | cross | merges |
|---|---|---|---|---|---|---|
| 0.20 | 0.9564 | 99.56% | 92.02% | 939 | 0 | 0 |
| 0.30 | 0.9562 | 99.57% | 91.98% | 919 | 0 | 0 |
| 0.40 | 0.9559 | 99.59% | 91.90% | 885 | 0 | 0 |
| **0.50** | **0.9547** | **99.59%** | **91.68%** | **871** | **0** | **0** |
| 0.60 | 0.9508 | 99.61% | 90.94% | 827 | 0 | 0 |

Not one cross-family pair anywhere from 0.20 to 0.60, where 0.30 at 640
produces 43,540 of them across twelve merges. The reason is the same one that
costs the default its recall: with 40% fewer descriptors the two Excel
screenshots never reach ten aligned points, so at this work size the inlier bar
is the only thing holding that family apart and the agreement bar is not
binding. It stays at 0.50 regardless — the value is derived as the midpoint of
what the statistic can report, not read off a corpus, and F1 is flat to 0.0017
across the whole range. But **do not carry "no cliff" back to 640**, and do not
read this as the bar being useless: it is the only defence against that failure
mode at any size where the pairs do reach the inlier bar.

`--min-frame-overlap` keeps the shape it has at 640 — no cliff in the usable
range, F1 best one step loose (0.80 at 0.9557 against 0.85's 0.9547) and that
step buying 112 more trap false pairs for 0.001 of F1, so it does not move.
`-k` is flatter than ever: **100, 150, 200 and 300 are identical to the pair**
at 384, and 50 costs 0.0003. The knee is below 100 and the shipped 150 is
still half the range clear of it.

`--features` is gone: measured over 300 to 900 at a fixed vocabulary it moves
F1 by 0.007 (0.971, 0.975, 0.975, 0.976, 0.976, 0.978, 0.977) and 900 costs 11%
of the run for nothing, so 600 is a constant in `main.rs`. It had looked
load-bearing, and that was the vocabulary step above, not the detector.
Candidate breadth (`-k`) is on a plateau, not a peak, and swept the whole way
it is the flattest surface in the tool — F1 0.9765, 0.9767, 0.9775, 0.9775,
0.9775, 0.9775 at 25, 50, 100, 150, 200 and 300, with the top four differing by
three pairs out of 224,779 and not one cross-family pair anywhere in the range.
The knee is below 100, so the shipped 150 is not merely past it but half the
range clear of it, and even starving the thing to 25 costs 0.001. Worth
remembering before reaching for `-k` to fix anything: on this corpus it is not
connected to a result.

## Conventions

**Canonical runner output.** Every runner emits:

```json
{"tool": "...", "config": {...}, "runtime_seconds": 0.0,
 "groups": [["/path/a", "/path/b"], ...],
 "pairs":  [{"a": "/path/a", "b": "/path/b"}, ...]}
```

**`pairs` wins over `groups` wherever both exist.** For every competitor,
groups are the union-find closure of the pairs; expanding a closure back into
pairs credits a tool with every match its chains imply rather than the ones it
made. This inflated SSCD from 9,172 claims to 238,771 once, and roughly 7,000
of a 16,267-pair labelling queue were artifacts of it. Any new consumer of
these files must follow the same rule.

img-fp's own `groups` are **not** a closure — each is a representative plus the
files that matched it, and it names the representative (see `src/group.rs`):

```json
"groups": [{"representative": "/path/a", "files": ["/path/a", "/path/b"]}]
```

`files` is the key every consumer reads, and `score.py` already handles that
shape. The rule still holds for img-fp too, for a different reason than for the
others: a group's members were each tested against the representative but not
against each other, and files in several groups would be counted once per
group. `score.py` prefers `pairs`, which is why the F1 figures above are
unaffected by how grouping works.

**Exit codes**, the same four `vid-fp` uses: `0` clean, `1` fatal (anyhow's
own path out of `main`), `2` finished but something failed, `130` Ctrl-C
(and SIGTERM, SIGHUP), which keeps every analysis finished so far in the cache
and exits **at once** — within 60 ms of the signal on this laptop, which is
what a `SIGKILL` takes as well, so it is the kernel tearing the process down
and not the handler. It is instant because it writes nothing: each record is
appended to the cache by the worker that made it (`cache::Store`), so the
handler waits for the one append in flight, if any, and exits (`interrupt` in
`main.rs`). A slow save would train people to press Ctrl-C twice, and the
second press would cost them what the first was saving; a second press is the
default disposition anyway, and the worst it can leave is half a record at
the end of the file, which the next run cuts off without calling it damage.

**The summary is `vid-fp`'s, and so is the split it draws.** `Skipped:` then
`Problems (N total):`, each category a count and up to ten examples, the same
five-wide right-aligned count and the same `- ... and N more` elision. The
split is the load-bearing part and it is what keeps exit `2` worth testing: a
skip is what the tool was never going to read and touches nothing, a problem is
what the run was asked for and did not get. Pointed at a home directory img-fp
passes over most of it and still exits `0`.

Skips: a file whose extension `-x` does not take — by default one that is not
an image format or has none, as in `vid-fp` (the one thing that can hide a
photograph — a JPEG named `.txt` is passed over without being sniffed); under
a wildcard `-x`, a file whose bytes are no picture and whose name never said
they were (it is also dropped from the exact groups, so two copies of a README
are not a pair);
a symlink met during a walk (a link and its target are one set of bytes; a path
*named* on the command line is still followed), a file reached twice through
overlapping roots. Problems: an image that would not decode, a path the walk
could not read (a mistyped root arrives as one `ENOENT`, which is what stops a
two-root run silently scanning one of them), an image that described to **no
features at all**, and a cache that could not be read, written or created.

None of them changes a pair, which is the point — the results line reads the
same either way, and the exit code is the only part of the difference a script
can see. Two of the four are worth their own note. A *stale* cache is not
counted: discarding every record when `--work-size` changes is the format doing
its job and happens on every sweep, where a damaged one costs a full
re-analysis every run until someone notices. And **featureless is a problem
rather than a skip**, although nothing failed: the file decoded, described to
nothing, and can now only match a byte-identical copy of itself. It is precise
rather than noisy — 4 of 5,638 here, all of them crops of night sky or
low-light seeds (`crop_strip_top` of `earth.jpeg`, two `low-light2` crops, a
`panoramic.avif` strip), which is a diagnosis of four of that row's misses —
and **0 of 2,786** photographs from the found corpus.

**`--log-file PATH`** is the unabridged list: every skip and every problem as
it is recorded, plus the stage timings whether or not `-v` printed them, plus
the summary at the end. Truncated per run, written a line at a time with no
buffer (a killed run keeps its tail, which is the case the flag is for), and
`-v` and `--log-file` never decide each other — `vid-fp`'s `verbosity` note
records what it cost to learn that. A path that cannot be created is fatal
before any work starts.

**So every benchmark run now exits 2**, because `Foto 38622.png` in the corpus
is truncated and will not decode. `bench.py` knows (`OK_EXIT`, which lets
`imgfp` return 0 or 2) and still records the code in `metrics.json`; anything
else that shells out to img-fp needs the same treatment, or it will read a
finished run as a failed one.

**The cache is on by default, and it is one file for the machine.**
`$XDG_CACHE_HOME/img-fp/analysis.bin`, `~/.cache/img-fp/analysis.bin` without
that variable, `/tmp/img-fp/` without `HOME` — the same lookup `vid-fp` does
for `fingerprints.redb`, and `--cache PATH` overrides it the same way, naming
the file unless it names a directory or ends in a slash. Three things follow
and only the first is obvious:

- **A cost measurement has to say `--no-cache`**, or it is measuring a cache
  read. `bench.py` passes it; anything else that times img-fp must, and a
  timing that came out four times too fast is this. Accuracy runs do not care
  — a cached record *is* the analysis the run would have done, and the pairs
  are identical either way, which is checkable in one command.
- **A run writes back what it did not look at.** One cache serving every
  directory on the machine would otherwise be worse than none: scanning
  `~/Pictures` and then `~/Downloads` would leave the first one's analysis
  destroyed, silently, and the user never asked for a cache to begin with. So
  `cache::carry_over` keeps every loaded record for a path this run did not
  cover — and drops it if the file is no longer on disk, which is what keeps
  the file from growing forever and is why there is no `--prune-cache` here.
  A record dropped by mistake, from an unmounted drive say, costs a
  re-analysis of a few milliseconds an image, where in `vid-fp` the same
  mistake costs an afternoon. **`--prune-cache`** is the stronger version and
  is a flag for that reason: it keeps only what the scan in front of it found,
  and it gives itself up — loudly, with a problem and exit 2 — when the walk
  could not read something it was pointed at, since pruning against a partial
  scan throws away records for files that are still there.
  **`--clear-cache`** deletes the file before the run; with `--no-cache` it
  deletes it and starts nothing new.
- **The settings header still invalidates the whole file**, and now that is
  every corpus's records rather than one's. It is the format doing its job
  (see `Settings`) and a `--work-size` sweep pays it every time, which is
  another reason a sweep should hand itself a `--cache` of its own. A cache
  written by a build with a different *format* is discarded the same way and
  just as silently — the magic carries a version, and a file with the right
  prefix and the wrong version is stale rather than damaged.

**A cached record is moved into the run, not copied into it.** Every walked
file's record is `remove`d from the loaded map before the analysis pass, so
what the pass finds is a record it can take; what is left in the map is exactly
the set `carry_over` wants. Cloning it instead — which is what the first
version did — made the largest thing the run holds exist twice over during the
phase that already sets the peak, and then charged again to free the
originals. On the found corpus that was **1.9 s of dropping a nine-thousand-
record map**, a similar amount cloning into it, and **164 MB of peak** (1,371
to 1,207 MB): the pre-matching half of a cached run went **6.6 s to 2.2 s**
and the whole run 59.2 s to 55.8 s. CPU-seconds are level (367 against 377,
which is this machine's noise), because what was removed is memory traffic and
a free, and the run is dominated by a stage this did not touch.

**And a run writes its records as it goes, not at the end.** Each analysis is
packed by its worker and appended to the file the moment it exists — which is
what makes Ctrl-C keep the work for free, see *Exit codes* — so the file holds
every record worth keeping by the time the analysis ends. It is rewritten only
when it also holds something not worth keeping: a record superseded by a later
one for the same path, a file that has gone, a `--prune-cache`. Then the
rewrite is a **copy** of the records worth keeping, byte for byte and sorted,
with nothing unpacked or deflated again. So a sweep's cached runs write
nothing at all, and a cold run no longer has a save phase: the deflate that
used to run over the whole corpus after the analysis runs on each worker as it
finishes an image. Measured on `derived/Desktop`, interrupted twice and then
finished, the pairs and groups are identical to a run that was never
interrupted, and the resumed file is the same size to the byte.

**Two venvs.** `vendor/venv` is the general one. `vendor/venv-imagededup` has
torch, so **SSCD and imagededup must run under it**; it also now has
`pillow-heif` and `pillow-jxl-plugin`. `vendor/venv-imgdupes` backs the
imgdupes binary. Getting this wrong looks like `ModuleNotFoundError: torch`.

### What a cached run still has to do

**The cache holds the per-image analysis, and on a found corpus that is under
half the work.** Asked why a cached rescan of 9,285 images takes ~47 s against
a cold ~116 s, the stage table answers it exactly (cached, `-t 8`, cooled, on
a warmer day than the question was asked on — 55.8 s total):

| stage | wall |
|---|---|
| walk, exact-duplicate hash | 0.2 s |
| **cache load** (660 MB, inflate + mip pyramids, parallel) | **2.0 s** |
| decode and describe | **0 — this is what the cache buys** |
| vocabulary built from the corpus's own descriptors | 1.7 s |
| quantise 4.88 M descriptors into words | 4.6 s |
| inverted file, retrieval of 1.08 M candidate pairs | 2.9 s |
| verify candidates -> 1,962 anchors | 4.6 s |
| **the mirrored and inverted second look** | **38-47 s** |
| bridges, propagation, corroboration, grouping | 0.4 s |

So the answer is not the cache and not the format: **everything after
extraction depends on the corpus as a whole and has to run every time.** The
vocabulary is trained on this corpus's descriptors, a word's meaning changes
when the corpus does, and a candidate list is a statement about other images —
none of it is a property of one file that a per-file cache could hold.

And two thirds of what is left is the **second look**, for the reason
*What the second look costs* gives: 8,766 of these 9,285 images have no
duplicate, so nearly every one of them is re-asked mirrored and inverted —
3.94 M verdicts to find 459 pairs. That is the shape of a found corpus, and it
is why the same cache makes the *benchmark* corpus about five times faster
(57 s cold to ~12 s) where it makes this one twice: there, extraction is 80%
of a cold run and the second look is 14 thread-seconds.

The lever nobody has pulled is a flag to skip the second look. It would be
worth ~40 s of this run for **459 of its 4,578 pairs**, which is a real trade
rather than a dominated one — unlike `--no-propagate`, which is why that flag
is gone. It has not been built because nothing has asked for it.

### How big the cache is, and why it is not smaller

**A record is the analysis, and the analysis is ~148 bytes per keypoint**: 128
of descriptor, 20 of keypoint, plus a thumbnail of up to 128x128. The found
corpus is the worst case rather than the best — its files are 224x224, which
`upsample_below` enlarges to 448 before describing, so each yields some 530
keypoints and 93 KB of analysis from a 25 KB JPEG. The file being bigger than
the corpus is not the encoding going wrong; it is how much analysis a small
picture produces, and the knob connected to *that* is the enlargement, which
is worth 4,581 pairs against 2,179 and is not a storage decision.

**What the encoding was wasting was a quarter, and that is now taken.**
Measured on 300 files of the found corpus — descriptors 71.5% of a record, the
thumbnail 17.2%, the keypoints 11.2%:

| stream | as written before | as written now | how |
|---|---|---|---|
| descriptors | 1.000 | **0.758** | deflate, and nothing else helps |
| keypoints | 1.000 | **0.763** | the five f32 fields split into byte planes |
| thumbnail | 1.000 | **0.681** | PNG's Paeth predictor, then deflate |

Whole file: **0.755 on the found corpus** (93.1 KB an image to 70.3) and
**0.720 on the benchmark one** (48 KB to 34.6), pair-for-pair identical output
on both, packed and unpacked in parallel batches so the clock does not notice.

**The descriptors are the wall, and the measurements that say so are worth
keeping** — they are what stops the next attempt:

- Their bytes carry **5.81 bits of order-0 entropy**, so deflate's 0.758 is
  already the Huffman bound. `zstd -1` gets 0.728, `zstd -19` 0.675 at 12
  seconds per 20 MB, and the best context model tried (previous bin plus
  orientation index) lowers the entropy only to **5.48 bits, or 0.685**. A
  range coder is 150 lines that have to be exactly reversible, for 7% of the
  file.
- **Not one descriptor in 159,701 was a duplicate of another**, within an
  image or across the corpus, so there is nothing for an LZ to find. That is
  also why transposing them is *worse* (0.766 against 0.752): dimension-major
  breaks what little locality there is.
- Grouping the three streams across a whole batch of records rather than per
  record is **0.7521 against 0.7526**, which is nothing — the streams are long
  enough already, and per-record keeps the format streamable and the packing
  parallel.
- Dropping `response`, which nothing outside the extractor reads, would take
  the keypoint stream to 0.638 and the file to 0.741. Declined: it is 1.9% of
  the file in exchange for a cached `Keypoint` that differs from a computed
  one in a field, which is exactly the kind of thing a later reader would
  trip over.

Concurrent runs are last-writer-wins: the loser's records are lost and nothing
is corrupted, because the temporary file a save renames into place carries the
process id. Two runs sharing one cache is not a case worth locking for — the
cost of losing is one re-analysis — but two runs sharing one *temporary file*
would be a damaged cache, which is a case worth a suffix.

## Benchmarking discipline

`bench.py` exists because casual timing on this machine is worthless.

- **One tool at a time, never two.** 60 s minimum cooldown, then a wait until
  the die returns to measured idle + 3 C.
- **This laptop thermally throttles.** Ryzen 7 3700U, 8 threads, idles at
  ~63 C and hits 77-92 C under load, with clocks swinging 1.2-3.0 GHz. Absolute
  timings are not comparable to a desktop's; relative ones under identical
  conditions are. Never use a fixed absolute temperature ceiling — measure the
  idle baseline first, or the cooldown silently times out every time.
- **Clear both caches.** Tool caches per tool (img-fp `--no-cache`, czkawka
  `-H`, imgdupes `--no-cache`, dupeGuru fresh temp db, SSCD denied
  `--embeddings`), and the
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
