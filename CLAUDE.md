# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from — read it before
changing how anything is scored.

**Two more corpora now exist, and they changed what the headline means** —
see *Near-miss families* under *How img-fp works*. IMGS has no photographs of
one scene taken a moment apart, so its "no wrong pair at all" was a property
of the corpus: on IMGS2 and IMGS3, which are made of such photographs, the
build before the clean-anchor rule ran at 87-92% precision.

**Status: the tool exists and beats every measured competitor by a wide
margin.** **The baseline is now all four corpora together** — IMGS, IMGS2,
IMGS3 and IMGS4 as one, `IMGS-ALL`: 27,659 files, 304 seeds, 90
transformations — since IMGS2-4 are spent and will defend no further rule
(`benchmark/BASELINE.md`; the field in `out/v17-all4`, img-fp's row re-measured
on the 0.30.0 release in `out/v23-plain`). There the released binary scores F1
**0.979** at 99.6% precision and 96.2% recall at the shipped default,
`--work-size 512`, against SSCD's 0.748 / 85.3% / 66.6%, in about 5.5 minutes
and 2,300 CPU-seconds against SSCD's 2.5 hours and 55,000. **46 of 87
transformations are handled perfectly** — every one of 304 seeds found across
the whole range of the amount — and every other tool manages **zero**. Its 258
cross-family false pairs are no merge: three mirrored `algiu2` variants joining
`algiu1`'s family through the second look (166 pairs), one `crop_micro` of
`Segovia1` (83), and nine single pairs. At 640 it is F1 0.981 (99.5% / 96.8%),
42 perfect. On IMGS alone — 5,638 files, 62 seeds — it is F1 **0.965** at
99.6% / 93.6% at the default, 41 perfect, and **0.972** at 640 (99.6% / 94.9%,
51 perfect); IMGS2, IMGS3 and IMGS4 alone score 0.981, 0.985 and 0.984 at the
default and 0.982, 0.987 and 0.987 at 640. All of those are `out/v23-plain`;
the competitors' IMGS-only rows are `out/v5`.

**Grey has been BT.601 luma since the CPU pass** (*Speed and memory*, the
pass over the four-corpus baseline's CPU), not the mean of RGB, and a JPEG's
grey is its own Y plane. The figures above are on that build; tables in this
file older than `out/v23-plain` predate it and are left as measured.

**And the release is the plain x86-64 build since 0.30.0**, which decodes JPEG
XL a rounding apart from the native build `target/release` makes here (*Speed
and memory*). `bench.py` measures `target/plain` by default for that reason,
and every figure from `out/v23-plain` on is the shipped binary's: checked byte
for byte against the downloaded release on all four corpora, the found corpus
and `derived/Desktop`.

**0.31.0 is the audit after 0.30.0** (`out/v24-constants`, `def_*`, measured
on the plain build of the same source). Six fixes — files that decode to one picture
matched as one, an EXIF orientation applied after the reduction, cut-off JPEGs
called damaged, no `rayon` in `image`, more extensions and extensionless files
sniffed (since taken back: see *Conventions*), libheif loaded at run time — move IMGS-ALL at 512 to F1 **0.9781**
(99.58% / 96.11%, 45 perfect) from 0.9788, and its cross-family false pairs
**from 258 to 3**: the three mirrored `algiu2` variants and the `Segovia1`
`crop_micro` no longer join, and nothing else does. IMGS alone goes 0.9652 ->
**0.9687** (41 -> 52 perfect), IMGS2 0.9806 -> 0.9811, IMGS3 0.9849 -> 0.9843,
IMGS4 0.9838 -> 0.9827; every move but the cross-family one is the vocabulary
sample changing, since the twins no longer sample twice. The figures in the
paragraphs below are 0.30.0's until a `bench.py` row re-baselines them.

**0.32.0 changes no pair on the benchmark** (IMGS under `-x '*'`, the way
`bench.py` runs it, is pair-for-pair and group-for-group 0.31.0's). A list
walk no longer takes a file with no extension, which 0.31.0 had started doing
and which reached browser caches; under `-x '*'` the exact pass no longer
reads whole files that are no picture; the walk's `stat` is the only one; the
cache is rewritten only once a quarter of it is unused; and `img-fp-gui`
answers `--help` and `--version`.

**0.33.0 is a fourth audit, and none of it moves a pair**
(IMGS pair-for-pair, group-for-group and cache byte-for-byte the same).
`-o`, `--dump` and `--log-file` refuse to write over a picture — `-o
photos/a.jpg` wrote the report over the image it then named as a
representative; hidden folders are skipped during a walk unless `--hidden`
(and the window's *Include hidden folders*) asks; a cache of another format
version is left alone instead of emptied; a cached run no longer copies out
the records it does not unpack; a PNG larger than `max_alloc` is read a row at
a time instead of whole; and the window's Trash dialog names the groups in
which every image is marked. `CLUSTER_SLACK_*` and the decode budget's
divisor were swept (`out/v25-slack-budget`; see *Parameters* and *Speed and
memory*). Every one is under its section below.

**Every report now suggests KEEP, DELETE or REVIEW per file** (0.34.0,
*Keep, delete, review* under *How img-fp works*): on IMGS-ALL no deletion loses
picture content but three synthetic composites in 17,877, against the
generated ground truth. It reads two new facts
per file measured at decode, so the cache format is `IMGFPC11`; the pairs are
unchanged. The window shows it on each card, tints REVIEW, and marks the
DELETEs on request (*Mark suggested deletions*).

**The shipped `--min-pixel-correlation` is 0.6**, raised from 0.5 in 0.12.0 for
what a user wants grouped rather than for F1: at 0.5 the tool grouped merely
similar photographs on real folders, a category IMGS has no negatives for.
Tables in this file older than `out/v14-fullsweep` call 0.50 shipped and were
taken there; they are left as measured.

**The default is 512, one step below the knee at 640.** `--work-size` scales
the first four fifths of the pipeline, so it is the only knob really connected
to the clock, and since enlargement stopped at the working size it bounds what
every picture costs (*Measured trade-offs*). 512 is where that limit stops
binding: a small picture gets the whole of the enlargement the sweep chose, so
a library of 224-pixel photographs is analysed as fully at 512 as at 640. On
0.30.0, 640 buys 1.3 points of IMGS recall (0.55 of IMGS-ALL's) for about 21%
more CPU on photographs and 1% on small images; 384, the default until 0.20,
gives up 2.7 points of IMGS recall for 27% less CPU on photographs and 74% less
on small images, and loses 18% of the found corpus's pairs (`out/v23-plain`;
on 0.20.0 those were 0.7 points for 40%, and 3.6 points and 30% of the pairs
for a third and four fifths). Precision is not part of that trade. The recall
the default gives up is concentrated in the one capability nothing else in the
field has at all, a photograph embedded in a bigger canvas: on IMGS
`embed_tiny` 45/62 against 53 at 640, `contact_sheet` 49 against 55, and on
IMGS-ALL 257 and 259 of 304 against 288 and 289. **Anyone who cares about that
case should pass `--work-size 640`.**

**The weak-anchor join is fixed, and what is left open is the clean-anchor
merges.** `admit_anchors` now refuses a weak-anchor join between two clusters
that each close a *cycle* of clean anchors, unless one file of the larger has a
weak anchor to every file of the smaller (the argument is under *Parameters*,
after the aligned-points replay). Every shipped setting is pair-for-pair
identical to 0.21 on all four corpora — IMGS, IMGS2, IMGS3 and IMGS4 at 384,
512 and 640 — and every weak-anchor merge is gone: Segovia on IMGS3 at 512 from
bar 9 down to 3 (bar 9: F1 0.9770 -> 0.9855, cross-family 6,794 -> 165), and at
640 across the whole non-monotone correlation band (0.55: 0.9787 -> 0.9878,
7,058 -> 2). `--min-aligned-points` at 512 goes from one step from a merge to
five (IMGS2's busts at 5). Measured in `out/v16-weak-join`.

What it does not touch, because it is a different mechanism: *clean*
cross-family anchors that survive the bridge test — on 0.21, `bust1`/`bust2` on
IMGS2 (bar 5 at 512, 8 at 640), `field2`/`field3` at 640 bar 3, `docks1`/`docks2`
at 640 overlap 0.6 and below, `Acueducto3`/`Acueducto4` at 512 overlap 0.3. **On
0.30.0 two are left** (`out/v23-plain`, swept by runs at 512 and 640 on all four
corpora and IMGS-ALL): `bust` at 640 only, bar 7 on IMGS2 and 8 on IMGS-ALL, and
`docks` at overlap 0.5 and below at both sizes. Nothing merges at 512 through
`--min-aligned-points`, `--min-pixel-correlation` or `-k` anywhere in the grid. Nor
the stray-file allowance at fragment scale: at 384, shipped settings, a
three-file `game2` fragment (a chain, no cycle) joins `game3`, 249 pairs,
which is in the published 384 row. The documented "Segovia at 384 bar 7" case
does not exist on 0.21, whose 384 changed with the enlargement limit.

(**The cost figures are the soft ones here, and the accuracy figures are
not.** Accuracy is a property of the build and the work size and reproduces to
the pair. Wall clock is a property of the session — this laptop's idle
temperature alone moves it 25% — so only like-for-like pairs within one
session mean anything. Do not put weight on the third significant figure. The
per-change figures in *Speed and memory* were measured at `--work-size 640`,
the default when they were taken, and are left as measured.)

Precision holds where it matters. Every false pair on IMGS at the default is a
deliberate rearrangement trap — 889 of them on 0.30.0 — and so there is no
wrong cluster merge. IMGS2 makes three cross-family pairs at the default,
IMGS3 87 and IMGS4 none: stray files joining the sibling photograph's family,
which is the lone-file allowance of the clean-anchor rule. Watch merges, not pairs: a merge's cost is
every pair the two families imply, so it grows with the corpus while a lone
bad pair does not. Zero should be read as "none survived", not as a guarantee
— *Parameters* has how close each option sits to one.

`benchmark/BASELINE.md` holds the competition's numbers,
`benchmark/VALIDATION.md` records the held-out experiment that shaped the
parameter surface. `README.md` is for users only: how to install and run it,
with no benchmark figures, comparisons or local measurements, and the same
holds for the `--help` text and man page generated from `Args`.

What is left is not accuracy-critical: decode is ~35% of the runtime and has no
downscaled JPEG path (`zune-jpeg` exposes none, and the arithmetic for adding a
second decoder is in *Speed and memory* below — it is worth about 4%), the
mirrored and inverted query is asked only where the first pass came up short
rather than being folded into the vocabulary, and nothing has been tested above
a few thousand images.

## Layout

```
src/
  lib.rs              the pipeline, propagation, bridge pruning, and the two
                      entry points: `cli_main` and the window's `worker_main`
  main.rs             `img-fp`: three lines calling `cli_main`
  gui/                `img-fp-gui` (feature `gui`, GTK 4.10+); see *The window*
  decode.rs           format sniffing and decode to one grayscale plane
  heif.rs             libheif, loaded at run time (libloading), not linked
  sift.rs             scale-invariant local features
  index.rs            vocabulary tree, inverted file, containment scoring
  verify.rs           correspondence, geometry, pixel agreement, the policy
  group.rs            pairs -> groups, around a representative
  walk.rs             roots -> files: --from-file / -0 / `-` lists, -x,
                      --exclude, --follow-symlinks, and one path per
                      (device, inode)
  cache.rs            on-disk cache of the per-image analysis: where it lives
                      when nothing says, and how a record is packed
  report.rs           the results as text, CSV or JSON (vid-fp's three), and
                      which one -o / --format asked for
  suggest.rs          KEEP / DELETE / REVIEW for every grouped file: the
                      `action` column of all three reports; see *Keep,
                      delete, review*
  problems.rs         what was skipped, what could not be done, the exit code
                      that says so, and --log-file
  progress.rs         one progress bar for the whole run: each stage owns a
                      stretch sized by its estimated cost (Forecast), revised
                      as the run learns; files weighted by a header probe
  simd.rs             which kernels a run uses: x86-64-v3 or portable, decided
                      at build time where the build can assume AVX2 and at
                      start-up where it cannot (`dispatched!`, `v3()`)
build.rs              sets `cfg(dispatch)`: an x86-64 build without AVX2
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
  suggest_score.py    scores a report's `action` column against the ground
                      truth: deletions that lose picture content (must be
                      none), and which families still keep their original
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
  out/v11/            img-fp at the then default, --work-size 384
  out/v11-worksize/   the previous build's one-session cold --work-size sweep
                      (metrics only; the run JSONs were not kept)
  out/v12-thumbstretch/ the thumbnail-stretch A/B at the shipped default,
                      correlation 0.6, three cold runs of each build (metrics)
  out/v13-propagation-floor/ propagation honouring --min-frame-overlap: the
                      same A/B at the default and at overlap 0.3 (metrics)
  out/v14-fullsweep/  0.20.0: every option swept on IMGS, IMGS2, IMGS3 and
                      all three together (results.jsonl, tables.txt), the
                      anchor replays (replay_ap.py, replay_corr.py), the
                      cooled work-size cost session (cost-worksize/), and the
                      scripts that made them
  out/v15-enlargement/ the enlargement rule swept (results.jsonl, tables.txt);
                      work-size-limit/ is the shipped rule, enlargement no
                      further than --work-size: accuracy at every size it
                      changes, every threshold at 512, the IMGS3 anchor replay
                      at 512, and the one-session cost table (cost/)
  out/v16-weak-join/  the cycle rule of admit_anchors, replayed on all four
  out/v17-all4/       THE BASELINE: twelve tools on IMGS-ALL, one session
                      (metrics.json, score.txt, analyse.txt, per_transform.csv)
  out/v23-plain/       0.30.0 re-baselined on the released plain binary: the
                      release/plain/native identity checks (compare.py), the
                      IMGS-ALL rows at 512 and 640 (bench512, bench640), every
                      option swept at 512 and 640 (and 384) on all four corpora
                      and IMGS-ALL, the work-size table, the plain-vs-native
                      cost A/B (run.py, score1.py, tables.py, results.jsonl)
  out/v24-constants/  the audit after 0.30.0: the fixed build on all four
                      corpora and IMGS-ALL at 512 (def_*), and the pixel
                      check's four constants swept on IMGS-ALL (build.sh makes
                      one binary per value, run.py, tables.py, results.jsonl)
  out/v25-slack-budget/ the cluster slack swept on IMGS-ALL at 512 from v24's
                      cache (run.py, results.jsonl), and the decode budget's
                      divisor on IMGS, cold (budget.py, budget.jsonl); one
                      binary reading both from the environment (img-fp-exp),
                      the scaffolding since removed from the source
vendor/               third-party tools and venvs, gitignored
```

**The baseline corpus is `/home/daniel/Documents/IMGS-ALL`**, built by
`benchmark/corpus/make_combined.py`: IMGS, IMGS2, IMGS3 and IMGS4 as hard
links under one root, and `IMGS-ALL/derived/` holding the four ground truths
merged, paths rewritten and seeds prefixed with their corpus. `bench.py`,
`score.py` and `analyse.py` all default to it. The links go stale when a corpus
is regenerated, so re-run the script (`--clean`) after `make_variants.py`.

The first corpus lives outside the repo at `/home/daniel/Documents/IMGS`: 54 seeds
in the root, 8 in `archive/`. One seed, `beach`, has no extension, so any run meant
to be compared with the published figures needs `-x '*'` — a walk leaves an
extensionless file out otherwise, as `vid-fp`'s does. `bench.py` passes it. `/home/daniel/Documents/IMGS-VAL` holds the 16
seeds that were once a separate validation set and are now folded in; it keeps
no derived tree.

**The found corpus is `/home/daniel/Downloads/archive`** — the "found corpus"
every cost table here is measured against, 9,285 files of the kind a camera
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

**Two more corpora, built the same way from different seeds.**
`/home/daniel/Documents/IMGS2` (71 seeds, 6,460 files) and
`/home/daniel/Documents/IMGS3` (101 seeds, 9,191 files) are photographs from
Wikimedia chosen in sets of the same scene at a slightly different moment,
distance or angle — two busts of one statue, a boat's deck a minute apart,
two Red Rocks entrance signs. The generator treats every seed as its own
photograph, so each set is a family of **hard negatives**, the case IMGS has
none of. Score them with `benchmark/analyse.py --corpus=DIR` (repeatable;
seeds are named per corpus when there are several). IMGS2 is a design
corpus. **IMGS3 was held out** and is now spent: once to validate the
clean-anchor rule, then in every run of the `out/v14-fullsweep` sweep. The
next rule to be defended needs fresh seeds. The old held-out set
(`benchmark/VALIDATION.md`) was folded into IMGS long ago; its rule is still
the rule.

`/home/daniel/Documents/IMGS4` (70 seeds, 6,370 files, generated the same way;
near-miss sets `baseball1-5`, `house1-5`, `aerial*`, `notes1/2`,
`Cmentarz1/2`) was **held out for the cycle rule** of `admit_anchors` and is
now spent. Read what it proved narrowly: the rule is identical to 0.21 there
at every setting replayed, and the shipped rule makes **no family merge on
IMGS4 anywhere in the grid** and never attempts a join between two cyclic
clusters — so it shows the rule costs nothing on fresh near misses, and
nothing about whether it prevents merges. That evidence is still IMGS3's.

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

The parts that are easy to get wrong:

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

  **What "flat" is measured against is the picture's own contrast.** The bar
  is a block standard deviation of 4 codes, and it used to be 4 codes out of
  255 whatever the picture spanned — so a dim or foggy photograph, spanning a
  few dozen codes, had every block abstain and scored 0 against an exact
  re-encode of itself: 27-34 aligned points, overlap 1.0, whole-overlap
  correlation 0.99, rejected. `Thumb::build` now stretches each thumbnail from
  its own darkest to its own brightest value before rounding it to bytes. That
  is not a new constant, it commutes with inversion, and a picture already
  spanning the range rounds exactly as before. Four photographs dimmed to a
  global spread of ~4 codes and saved as PNG and JPEG: three of the four pairs
  went from missed to found (the fourth never reaches verification — too few
  features). On the benchmark corpus, which has almost no dim pictures, it is
  +198 / -50 pairs, **TP 212,205 -> 212,351, FP 831 -> 829, cross-family 0 ->
  0**, F1 0.9518 -> 0.9521, both seed halves up by the same amount, and level
  on the clock: three cold alternating pairs at 46.5 / 46.4 / 43.3 s and 308 /
  308 / 304 CPU-s before, 44.9 / 45.1 / 43.9 s and 304 / 308 / 306 after, peak
  PSS 626-769 MB either way (`out/v12-thumbstretch`). The cache format went to
  `IMGFPC04` with it, since the stored thumbnails changed.
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

  **A round after the first proposes again only in components that gained a
  pair.** One that gained nothing has the same edges, tree and poses, so it
  would propose the same hypotheses and get the same answers; components never
  merge in propagation, since a composed pair joins two files already in one.
  Pair-for-pair and group-for-group identical on IMGS; three alternating cached
  pairs, cooled: `pixel_check:prop` 3.72 / 3.74 / 3.72 -> 3.30 / 3.34 / 3.45
  CPU-s, the run 49.7 / 49.7 / 50.0 -> 49.0 / 48.8 / 49.0. Small, because five
  rounds run on this corpus and most families gain something in each of the
  first two.

  **Which is why `--no-propagate` is gone.** "Less time for less recall" is a
  legitimate thing to want, and this was the worst available way to buy it —
  not a worse point on the speed/accuracy frontier but nowhere near it, because
  propagation lives in the last fifth of the pipeline while `--work-size`
  scales the first four fifths. Measured at 640 on earlier builds, no
  propagation was F1 0.9061 and recall 83.11% for 7-11 CPU-seconds saved;
  every work-size row of `out/v11-worksize` **dominates** it — 384 gave back 2.3
  points of F1 and 4.4 of recall *and* saved 219 CPU-seconds. There is no
  corpus and no setting on which a user wants the flag, so
  the pass is now unconditional. To re-measure it, put the `if`
  back around the propagation loop in `main.rs`; that is a two-line edit and a
  rebuild, and it is the right price for something no run should be doing.
  **What a round may spend is a budget, not a cap on component size.** A
  round proposes every unmatched pair of a component, which is quadratic in
  it; until now a component over 2,000 files — a literal in the first commit,
  never derived — was not propagated at all. Measured on 2,100 variants of one
  photograph, that was 218,256 pairs and 26 overlapping groups for one
  picture, where the uncapped run gave 2,104,842 pairs and one group. Now a
  round may put as many composed pairs to the pixels as the direct pass
  verified candidates (`prop_budget`), which keeps propagation in the units
  everything else costs, files times `-k`. Components are taken cheapest
  first, and one that does not fit is **starred**: each file is compared with
  the root alone, linear in the component, and since the root is the best-
  connected file it is the one `group::find` elects — the 2,100 come out as
  one group, 219,979 pairs, in 7.8 s. What a star gives up is the pairs
  between two non-root members that direct matching missed, which no group
  shows. On IMGS the budget is 586,329 a round against 81,737 proposed over
  all rounds, and the output is pair-for-pair identical. Where it binds is a
  folder that is mostly one family of more than a few hundred files: a
  family needs about `F^2/2` checks against a budget near `F * k`.

  **And the work is spread by row, not by component.** A component was one
  task, so a large family ran on one thread: the 2,100 took 76 CPU-seconds in
  70 s of wall. A unit is now one row of a component's pair triangle (or one
  pair of a star), and units come back in plan order, so a component's pairs
  are in the order its own loop made them. IMGS plus 1,000 variants of one of
  its seeds, cached, cooled: round 1 16.6 s -> 4.9 s of wall, the run 31.0 ->
  19.4 s, pair-for-pair identical (785,537 pairs).
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
  a threshold. It is **not** the file to keep: measured on IMGS it is the
  pristine original in 33 of 62 families, and the README says as much to
  users ("don't interpret the representative as the source image").

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

- **Near-miss families: clusters are made of clean anchors.** An anchor is
  *clean* when no detailed block of its overlap correlates below 0.85
  (`Verdict::blk_min`, `Policy::clean`), and `admit_anchors` lets the rest
  join two clusters only when **every file of the smaller has a weak anchor
  into the larger** — which a lone file does trivially, and a fragment of a
  family does, and a family of ninety photographs of a scene a moment apart
  does not. It is the bridge test carried from one edge to many: near-miss
  families touch through dozens of weak anchors, none of them a bridge, and
  the bridge test kept them all. At the shipped settings IMGS2 merged five
  families, 22,201 false pairs; IMGS3 fifteen, 41,312.

  The weak ones really are weak and not separable one at a time. 443 wrong
  anchors on IMGS + IMGS2 against 351,812 true ones: 12 aligned points at the
  median (true 77), mean correlation 0.82 (0.97), worst block 0.09 (0.89),
  inlier residual 0.27 of the tolerance (0.07) — but the true tail is the
  catalogue's warps (barrel, keystone, perspective, photo of a screen) and
  overlays (watermark, redaction, QR sticker), which is what a photograph
  from a step to the side *is*. And killing wrong anchors one by one does
  not stop merges: at 18 left, two families still merged, because a merge
  needs only a few that are not bridges. The structure is what differs, so
  the rule is about structure.

  Measured, the derived-sample build without the rule against with it, F1
  (cross-family false pairs): IMGS 384 0.9527 -> 0.9481 (0 -> 0); IMGS2 384
  0.9315 -> 0.9709 (22,201 -> 1); IMGS+IMGS2 384 0.9461 -> 0.9589; IMGS 640
  0.9723 -> 0.9706; IMGS2 640 0.9411 -> 0.9835 (23,770 -> 0). **Held out,
  IMGS3: 0.9208 -> 0.9714 at 384 (41,312 -> 222, recall 94.89% -> 94.80%)
  and 0.9222 -> 0.9863 at 640 (52,079 -> 2)**; all three corpora together
  0.9294 -> 0.9619 and 0.9409 -> 0.9799. The 222 are three lone files — a
  micro-crop and two column-rolls — joining the sibling photograph's family,
  which is the lone-file allowance working as designed; `game2`/`game3`,
  which share most of a stadium, stay apart. The cost is IMGS's: about half a
  point of recall and 45 -> 39 perfect transformations at 384. On the found
  corpus it takes 3,249 pairs to 2,438, and every one of sixteen removed pairs
  sampled at random was two different pinned butterflies.

  **0.85 is fitted**, and it is the rule's one number. Replayed on each
  corpus's own anchors (`--dump` now writes `blk_min`), merges climb below
  0.8 and none happen from 0.8 to 0.95, while IMGS's reach falls as it rises.
  Tried and worse: *never* joining two clusters on weak anchors (IMGS recall
  -4, perfect transformations 45 -> 2, because two variants that match each
  other cleanly make a cluster of two and a low-contrast family breaks up);
  requiring twice the aligned points for a clean anchor (a dark or plain
  picture has few features and clean pixels, and `low-light`'s family split
  in three); holding propagated pairs to the clean bar too (recall -7 to -8).

  **Open:** on the found corpus about half of a random sample of the pairs
  still reported are different pinned specimens, most of them propagated at
  correlation 0.60-0.79 inside a cluster. That leak is the propagated tier,
  not the clusters, and the obvious fix above costs too much.
- **The inverted file indexes every word, however common.** Until 0.25 a word
  in more than a fifth of the images (or 32, whichever was more) was left out,
  a cap from the first commit that nothing derived. Measured, it never binds on
  a real corpus: on IMGS and on the found corpus no word reaches a fifth of the
  images, so the candidates (586,329 and 1,030,236), every pair and the clock
  are the same with it and without it. It binds only in a folder that is mostly
  one picture, and there it moved nothing that matters: one family plus two
  hundred other files, identical family recall either way though the anchors
  moved by 1.6%; five hundred re-encoded variants of one photograph, 41,756
  pairs with the cap and 41,318 without, at the same cost (that family is
  starred by the propagation budget either way). Removed; `InvertedFile::build`
  has no cap argument.
- **The vocabulary sample is derived** (`DESC_PER_SAMPLE` in `index.rs` has
  the argument): a tenth of the corpus's distinct descriptors, capped at
  1.28 M. The fixed 160,000 held the *live* words near 150,000 whatever the
  corpus, so descriptors per live word grew with the library, and swept on
  IMGS, IMGS2 and both at two work sizes every curve merges families past
  roughly 70-150. At today's corpus sizes the change is within noise (IMGS
  0.9544 -> 0.9527 at 384, both corpora 6 -> 3 merges at 384 and 6 -> 7 at
  640); it exists for a library of tens of thousands of photographs.

  **What the two cost together**, the derived sample and the clean-anchor
  rule, against `c1bf031`: `bench.py`'s protocol (cold page cache, cooled,
  `--no-cache`, PSS over the process tree), in the order A B B A A B.
  IMGS, CPU-seconds slot-matched: 333.9 -> 336.6 and 341.7 -> 341.1, wall
  52.1 -> 52.7 and 53.0 -> 53.4 s, PSS inside its usual 640-710 MB band both
  ways; the first pair is void, the "after" run having started at 82 C and
  averaged 2,051 MHz against 2,947. Found corpus: CPU 916 / 928 / 926 before
  against 910 / 914 / 910 after, wall 140-142 s both, peak PSS 1,052 MB three
  times before against 1,090 / 1,090 / 1,130 after — the larger vocabulary
  sample, 488 k descriptors against 160 k. All three benchmark corpora
  together, one pair: 284.7 -> 283.3 s, 1,738 -> 1,732 CPU-s, 1,893 MB both.
  So level on time everywhere, and 40-80 MB more peak where a corpus is large
  enough for the sample to grow.
- **"Identical" is a byte comparison, not a hash.** `exact_groups` hashes
  same-size files and then compares each hash group byte for byte. The hash
  alone was FNV-1a over 64-bit words into a 128-bit state, where new bytes
  enter only the low half: a change in bits 40-55 of one word leaves a
  difference the next word can cancel, so collisions are built in two words.
  Two 256x256 PGMs differing in 12,145 bytes were reported `identical`, and the
  window would have offered one for the Trash (`a_hash_collision_is_not_an_
  identical_pair` builds one). The comparison re-reads only files that share a
  hash, which are the duplicates, and `derived/Desktop` is pair-for-pair the
  same either way.

  **Same-size files are told apart by a sample first** (`sample_hash`: 16 KB
  at the start, at a third, at two thirds and at the end), and only files
  agreeing there are read whole; a pair is compared straight away, a larger
  group hashed whole and then compared. Every same-size file used to be read
  whole on every run, cached or not: thirty same-size 9 MB BMPs, all cached and
  nothing changed, read 270 MB, and now 2.5 MB. Four pieces rather than the
  start, because an uncompressed scan's first and last rows are header and
  white margin.

  **Under a wildcard, a same-size file is read only if it may be a picture**
  (`decode::may_be_image`: the 64-byte head test `decode` itself refuses
  `NOT_AN_IMAGE` on, or a name that says image). Such files used to be
  hashed and compared whole and then dropped from the groups once they failed
  to decode: two identical 200 MB `.mkv` files under `-x '*'` cost 400 MB of
  reads and an "identical copy" in the header line, now 128 bytes. The later
  `not_asked` retain stays as the backstop, so the groups are what they were.
- **Byte-identical copies are matched through their original, not beside
  it.** Each exact group elects one member (one the cache already holds, if
  any) and everything from the vocabulary to corroboration runs over the
  originals alone; `with_copies` then states every pair for each copy of
  either file, with its original's verdict. A copy used to be indexed,
  queried and verified like any other file: against its own original, among
  others, and taking a place in every candidate list its original was in. Two
  more places counted copies and no longer do. `InvertedFile::build` measured
  idf over every file rather than the files with words, and the second look's
  degree counted a file's copies as matches, so in a library holding every
  photograph twice over no file was ever re-asked mirrored or inverted.
  Measured: on `derived/Desktop` copied three times over (1,908 files) the
  output is **exactly the single-copy output tripled**, 24,435 pairs with none
  missing and none extra, where `HEAD` was 3,072 short, at **9.6 CPU-s against
  16.3**, peak 87 MB against 110, and a cache a third the size (a copy keeps
  no record of its own). On IMGS, whose 170 copies are 3% of it, the cost is
  level and F1 goes **0.9521 -> 0.9544**, FP 829 -> 762, cross-family 0 -> 0,
  46 -> 50 perfect transformations; both seed halves move less than a tenth
  of a point, one each way. That part is a changed vocabulary sample rather
  than the copies themselves, and so is the found corpus's 3,731 -> 3,450
  pairs: over 150k / 160k / 170k samples it reads 3,604 / 3,731 / 3,851
  before and 3,782 / 3,450 / 3,715 after, a band of about five per cent that
  either build lands anywhere in.

- **Files with one working plane share one analysis** (`lib::Planes`). The
  analysis is a function of the decoded working plane alone, and lossless
  re-saves — the same photograph as PNG, TIFF and lossless WebP, a JPEG with
  its metadata stripped — decode to the same plane bit for bit while their
  bytes differ, so the exact pass cannot see them: **1,113 of 26,886 analysed
  files on IMGS-ALL (3.9-4.4% of each corpus)**. The plane is hashed with a
  SipHash keyed at random per run, plus its size; a twin waits on a `OnceLock`
  for the first one's features and thumbnail, and the word lists are then
  quantised once per analysis. It removes 4% of the extractions; end to end,
  cold A B B A on each corpus, the plain build summed 2,234 -> 2,192
  CPU-seconds (-1.9%), with single pairs spread over ±3-7%, so the measured
  figure is the work removed rather than the clock.

  **And since the audit after 0.30.0 they are matched as one file**
  (`lib::same_analysis`), the way byte-identical copies are. 0.30.0 left
  everything downstream treating them as separate files, and that brought back
  the copy bug word for word: each twin counted the others as matches, so a
  picture held in three lossless formats was never re-asked mirrored or
  inverted. Eight photographs and their mirror images, each saved as PNG, TIFF
  and lossless WebP (48 files): the second look re-asked **0** files, found
  **9 of 72** photograph-mirror pairs and made **15 groups for 8**; now 72 of
  72 and 8 groups. A set is decided from the *analysis* (keypoints and
  thumbnail hashed, then features and thumbnail compared whole) rather than
  from the plane, so a cached run, which decodes nothing, decides it alike; a
  featureless file is never in one, since two blank pictures stretch to the
  same thumbnail. One member of each set is matched, the others take its pairs
  through `with_copies` (now handed `match_of`, a byte original's pixel
  original), and every pair inside a set is stated as `same_pixels`, a
  relation of its own in all three reports and on the window's cards.

### Keep, delete, review

**Every grouped file carries an `action`** (`suggest.rs`), in the text
report's second column, the CSV's `action` and each JSON row. One answer per
file, whatever group it is read in. **The requirement, set by the user, is
that a deletion loses no picture content**; which copy is kept is secondary,
and the original, a lossless copy, a re-encode or a composite that holds the
whole photograph (a collage, a slide, a meme) are all acceptable keeps.
REVIEW is for weak matches close enough to be a copy (a tint, a watermark),
which nothing may act on; the window tints them and marks only DELETE. The user's second round loosened that to the photograph's
level: a thin strip at the edge, or a crop stretched back to the same square,
is not content worth keeping a second file for.

**How it is scored** (`benchmark/suggest_score.py`). A DELETE is a loss unless
a file that is not deleted holds it: for a trap or a composite (whose canvas
is content of its own) only an identical copy does; otherwise a file of the
same seed whose region contains the deleted one's (2% slack), at no less
effective resolution (5%), and unedited if the deleted one is unedited (an
edit can stand in for an edit, never for the original). `blur_fill` is not a
composite (its canvas is the photo blurred) and `video_call_frame` is not
either (a crop recompressed; the manifest's coverage 0.9 is nominal).

**Measured on IMGS-ALL at 512**, plain build: of 27,216 grouped files,
**KEEP 7,183, DELETE 17,877, REVIEW 2,156; three deletions lose content** — a
`browser_window` (earth) deleted by an `embed_small`, a `picture_in_picture`
(red_rocks_sign1) by an `embed_tiny`, and an `embed_small` on IMGS3,
composites the generator lays on canvases of one size with the photograph in
one place, so they line up exactly and differ only in flat furniture — and
**none rests on a pair between two families**. 266 of 304 families keep the
original or an exact or lossless copy. **On the found corpus** (no ground
truth): 1,147 KEEP, 463 DELETE, 195 REVIEW. The first version deleted 60
there and the second 300 (792 KEEP, 713 REVIEW); the user's examples showed
why so few each time (below), and 48 new deletions sampled at random from
each round were each the same photograph as the file kept for it.

**The second round: half the found corpus was REVIEW.** The user's three
largest groups (47, 21 and 19 files) were nearly all REVIEW. Looked at, most
of those files are **different specimens of one species** — the matcher's
known leak on that corpus (*Near-miss families*, "Open") — and REVIEW, not
DELETE, was right for them; what was wrong was calling them anything but
KEEP. Of the found corpus's REVIEWs whose best pair agrees at 0.9 or more,
nearly all were one photograph tinted, saturated or watermarked; between 0.8
and 0.9 about half; below, almost none. And 276 pairs agreeing at 0.9 with a
clean worst block had neither file deleted. A trace of each refusal said
why: of the REVIEWs at 0.9 and over, 116 failed *holds* (each file a crop of
the other by 2-11%, or a crop stretched back to 224x224 at 0.2-0.4 octave),
34 the whole-frame check, 3 the composite rule. Swept on both corpora (the
`out/v26-suggest` scorer plus undeleted 0.9 clean pairs on the found corpus):

| change (on the first table's shipped row) | IMGS-ALL DELETE | content lost | found D / R | undeleted pairs |
|---|---|---|---|---|
| before | 17,872 | 2 composites | 300 / 713 | 276 |
| a deleted file may lie 5% outside its keeper | 17,768 | 2 | 326 / 710 | 251 |
| ... 10% | 17,662 | 7 | 366 / 710 | 216 |
| ... 15% | 17,848 | 17 | 361 / 743 | 219 |
| a keeper may shrink 0.5 octave before detail is asked | 17,874 | 2 | 427 / 689 | 157 |
| weak pairs delete at 0.90 / 0.85 / 0.80 | 18,291 / 18,421 / 18,464 | 3 + 1 trap each | 324-329 / ~660 | 276 |
| whole-frame residual 16, weak 0.85 | 18,859 | 4 + 3 traps | 344 / 631 | |
| two crops each missing <= 10% count as one frame, + shrink 0.5 | 17,769 | 3 | 457 / 680 | 137 |
| ... 15% | 17,691 | 6 | 472 / 696 | 123 |
| ... 10%, + weak 0.85 | 18,318 | 5 + 1 trap | 496 / 604 | 139 |
| **10% (between deleting pairs only), shrink 0.5, REVIEW only at 0.9 (shipped)** | **17,877** | **3** | **463 / 195** | **131** |

The mutual-crop rule first applied to every pair, and a weak pair between two
specimens framed alike then made each "plain", which turned the composite
rule on and kept two settled pairs (330/1693, 1892/1497 in the user's
groups); it is now asked only of pairs that may delete. Lowering the weak
bar was the obvious move for the tinted copies and buys almost nothing: they
fail *holds* or the whole-frame check, not the bar. What is left in the
user's groups: 1643/3685, where 3685 holds 1643 with a strip of background
more and the composite rule keeps both; 2895/5722, each missing 21% of the
other.

**Why the first version deleted so little on a real folder.** The found corpus
resized every photograph to a 224x224 square, so its copies are stretched on
one axis, cropped, and sometimes watermarked; three rules each stopped them:
the clean bar (a watermark or a recoloured background drops the worst block
to 0.76-0.79 at 0.96 overall), the detail test on small resamples (a 9%
squash or a watermark read as "real extra detail"), and the composite rule
(a file deleted one it held whole only if it had a same-frame copy of its own,
which in a group of two files nothing has). Swept on IMGS-ALL and the found
corpus together, each against the ground truth's content loss and against
deletions resting on another family (scaffolding since removed):

| change | IMGS-ALL DELETE | content lost | found DELETE |
|---|---|---|---|
| first version | 17,632 | 0 | 60 |
| clean bar 0.85 -> 0.5 (alone) | 18,502 | 1 composite | 72 |
| + weak pairs delete at >= 0.95, a set with no same-frame copy, `SAME` 0.2 (**first round**) | **17,872** | **2 composites** | **300** |
| weak pairs delete at any correlation | 18,460 | 1 composite, 1 trap | 74 |
| "file with no copy" for any file, not only a set without one | 18,075 | 2 composites, 3 traps | 300 |
| + two crops each with a strip <= 20% the other lacks count as one frame | 17,479 | 5 composites | 362 |
| frame tolerance by area, 10% | 17,551 | 12 composites | 440 |

Of the user's three found-corpus groups, one is now settled
(`Image_6478.jpg` DELETE, its watermarked copy; `Image_868.jpg` KEEP). The
other two hold content each way: `Image_6463.jpg` is `Image_685.jpg`
squashed 16% across with a 16% strip of its own (685 has 9%), which only the
two-crops rule settles, at three more composites lost on IMGS-ALL; and
`Image_632.jpg` is a sharper, enlarged crop of `Image_6497.jpg` (twice its
fine detail), while 6497 shows 38% more of the scene with its background
recoloured.

Tried in the same sweep and not kept: requiring weak deletions to spare any
file holding a plain picture, and judging composites by area rather than by
corners — each cost several hundred good deletions and left the same two
composite losses.

**Two simpler rules, by the user's request** (`--suggest correlation |
representative`, `suggest::by_group`), which read only the groups and so are
also what the window switches between without a scan. `representative`
keeps every representative and deletes the rest. `correlation` keeps every
representative and judges any other file by its best correlation with one:
under `--min-pixel-correlation` REVIEW, from the bar to the cut KEEP, at or
above the cut DELETE. The user proposed the cut halfway, `(min + 1) / 2`;
the sweep below put it three quarters of the way (0.9 at 0.6), and the user
then chose halfway (0.8) after all, so that is `CUT`. Swept offline from
the shipped build's reports (scratch `corr_sweep.py`, the content rule's
report regrouped):

| cut at 0.6 (ratio) | IMGS-ALL DELETE | content lost | originals kept | found DELETE |
|---|---|---|---|---|
| 0.60 (0) | 26,433 | 4,466 | 153 | 1,027 |
| **0.80 (0.5, shipped)** | **24,582** | **3,875** | **153** | **747** |
| 0.84 (0.6) | 23,970 | 3,688 | 154 | 709 |
| 0.88 (0.7) | 23,099 | 3,370 | 157 | 667 |
| 0.90 (0.75) | 22,591 | 3,183 | 158 | 649 |
| 0.94 (0.85) | 20,629 | 2,558 | 166 | 594 |
| 0.98 (0.95) | 14,498 | 987 | 200 | 389 |
| 1.00 | 6,123 | 132 | 254 | 12 |
| representative rule | 26,522 | 4,490 | 153 | 1,034 |
| content rule | 17,877 | 3 | 266 | 466 |

The ground truth has no knee to offer: losses fall steadily to the top, and
nearly all are composites (2,835 at 0.9) — a collage, a slide, a frame agree
with their representative over the photograph they share — plus originals
whose representative is a crop or an edit. So the cut was set on the found
corpus, by eye, sixteen random matches a band against their representative:
0.80-0.90 ten of sixteen another specimen of the species, 0.90-0.95 one,
0.95 and up none. 0.9 is where the found corpus's matches become copies; at
0.8 the rule deletes other specimens of a species.
Neither rule answers to the content requirement; the window says so in its
tooltip and the README says so in words.

**The pieces** (each is in `suggest.rs` with its reason):

- A pair is clean at `Policy::clean`'s 0.85. A pair that is not may delete
  when it agrees at 0.95 overall (`WEAK_DELETES`); below that, or inverted,
  it decides nothing. A file left reached only through such pairs is REVIEW
  when one of them agrees at 0.9 (`REVIEW`), KEEP below, unless a deletion
  rests on it.
- Two crops each missing at most 10% of the other's area (`CROPS`), on a
  pair that may delete, are one frame.
- *Holds* is by the frames' four corners (2% plus 3 of the file's pixels), not
  by `frame_overlap`: its 16x16 grid read a 15% caption strip as 6%.
- *Holds* also needs the keeper not to shrink the picture by more than 0.5
  octave (1.4 times) on either axis (`SHRINK`; 0.2, `SAME`, in the first
  round), unless the extra pixels hold no detail an enlargement would lack:
  the `Traits::detail` figures, measured at full resolution during decode.
  Without them only 6 of 304 families kept their original (the upscale won).
- **A whole-frame check before every deletion** (`whole_worst`): both
  thumbnails Gaussian-blurred to one sample's footprint, a linear tone fit,
  and every 8x8-sample block of 32x32 within 12 grey levels. The pixel check
  lets flat blocks abstain, and a caption bar's white strip "matched" a
  slide's white margin, deleting the caption. A file refused three times is
  not asked again (`WHOLE_TRIES`).
- A file that strictly holds a picture with its own same-frame copies is a
  composite: deleted only by a copy of itself, and it never stands in for the
  plain picture (so the photo and its memes are all kept).
- Of files that hold each other, one is kept: near the group in grey
  (`Traits::grid`), not softer than the rest, then lossless, EXIF, smaller;
  pixel-identical files stand as one; colour over grey, asked lazily —
  a JPEG's colour is not decoded by the analysis, so the few tied JPEG
  front-runners are decoded for it (`decode::colour_of`, 262 on IMGS-ALL).
  Without colour, 5 more families kept the greyscale copy.

**What it costs.** The decision is about **3.5 s of wall on IMGS-ALL** (8
threads, cached run of ~100 s; under 0.1 s on the found corpus), almost all of it
the whole-frame checks' blurs (~20,000, one per file) and the colour decodes.
The decode-time measure costs the reduction **+1.3 ms on a 12-megapixel luma
JPEG** (`reduce_timings` l8 4000x3000: 5.3 -> 6.6 ms; rgb8 9.5 -> 11.5),
measured every eighth row; every fourth, with one float accumulator, it had
tripled the reduction. The cache went to `IMGFPC11` for the traits.

**End to end it is level with 0.33.1** (`out/v26-suggest`, `bench.py`'s
protocol: cold, cooled, evicted, `--no-cache`, plain builds). IMGS-ALL in the
order old new new old: CPU 2,730.8 / 2,522.5 old against 2,645.6 / 2,588.0
new, wall 435.4 / 397.7 against 418.2 / 409.5 s, peak 2,167 / 2,144 against
2,146 / 2,170 MB — **-0.4% of the CPU and -0.6% of the wall** on the means,
the session cooling as it went. The found corpus twice, old new new old then
new old old new: CPU 745.1 / 896.9 / 919.3 / 886.5 old against 829.8 / 877.6
/ 855.3 / 855.3 new, wall 131.4 against 130.0 s on the means, peak 990 MB
both — **-0.9% of the CPU**, the session heating 20% over the first four
runs. What the pieces predict is about +0.5% on IMGS-ALL (the decode measure
some 10 CPU-seconds, the suggestion about 20, the colour decodes 4) and
nothing on the found corpus; neither is resolvable at this machine's spread.

Tried and not kept: a mip pyramid in place of the blur (the 2x2 grid lands
differently on each side: `crop_strip_top` against its original read 16 grey
levels against the blur's 2.7, and some 230 more crops and rotations were kept);
skipping the whole-frame check when the pixel check scored all 36 blocks
(-30% time, and it deleted a `quadrant_rotate` trap the check had caught).

### What still misses

**14,828 pairs at the default, 11,834 at `--work-size 640`** on IMGS (0.30.0,
`out/v23-plain`). The worst rows, out of 62 seeds, with 640 in brackets:
`crop_micro` 37 (40), `embed_tiny` 45 (53), `contact_sheet` 49 (55),
`tiled_watermark` 54 (55), `picture_in_picture` 55, `wave_vertical` 55 (54),
`crop_strip_top` 56 (56), `crop_quarter` and `halftone` 57 (58, 56),
`scale_small` 58 (56).

Two patterns. The old one is very small crops, heavy downscales, and warps
that break a fitted affine model. The other is **containment**: the work size
is the long side of the *canvas*, and the photograph inside it is a fraction
of that, which is what every size below 640 pays for first. This is the cost
worth knowing about before recommending the default to anyone whose corpus is
screenshots, slides or contact sheets.

**Rows where a competitor leads: seven on the four-corpus baseline**, none by
more than three seeds of 304 (0.30.0 in `out/v23-plain` against the field in
`out/v17-all4`): SSCD on `perspective_top` and `keystone_side` (303 against
301), `scale_nearest` (303 against 300), `crop_half` and `scale_mid` (303
against 302); SSCD and czkawka_tuned on `flip_h` (303 and 302 against 300) and
`transpose_diag` (301 and 302 against 300). `photo_of_screen` and
`motion_blur` are level with SSCD at 303, and `halftone` is now img-fp's, 294
against PDQ's 293. The mirrorings are the second look, asked only of files the
first pass anchored fewer than twice. Where img-fp leads it leads by hundreds:
`embed_tiny` 257 against 0, `contact_sheet` 259 against 0, `crop_micro` 222
against SSCD's 0. On IMGS alone it was two rows at the default — `halftone`
(PDQ, 58/62 against 57) and `keystone_side` (SSCD, 61 against 60) — four at
640 and eight at 384.

A note on the traps, since they are now the *whole* FP count and will keep
growing as recall does:
`column_roll_37` slides an image sideways and wraps, leaving 63% of it a rigid
translation of the original, so a crop landing inside that 63% genuinely *is*
present in both files. The corpus calls such pairs DIFFERENT on the scramble
rule and img-fp is caught by it for a defensible reason. Report trap hits and
real errors separately, the way `BASELINE.md` does for SSCD.

### Elongated pictures: measured, not shipped

**`--work-size` limits the long side, so a picture far from 4:3 is analysed
at a fraction of the detail.** A full-page screenshot, a long comic or a
panorama is shrunk by its aspect ratio as well as its size: a 1080x11520 page
of eight photographs is 48x512 at the default, so each photograph in it is
48x64. One of them, saved on its own, was found in a 1:9.3 page at no size up
to 1024 — `--dump` shows the right transform (scale 6.98, correlation 0.85,
overlap 1.0) on 3 aligned points at 512 and 7 at 1024, against a bar of 10 —
and in a 1:10.7 page at 2048 but not at `0`, where 600 features are spread over
the whole page. Up to about 1:6.6 the default found it.

**The rule tried** (`AREA=1` in `out/v19-aspect-texture/img-fp-exp`): a
picture wider than 4:3 gets the pixels a 4:3 picture of the working size gets
— the long-side limit times `sqrt(3/4 * long/short)`, never less than 1 — and
its pixel-check thumbnail the same factor on its 128. A 4:3 or squarer picture
is untouched to the pixel, so the found corpus (224x224) cannot move; a 16:9
one gets 591 on its long side at 512, a 1:10 one 1,402. Every picture still
costs at most what a 4:3 one does.

**On pictures it is for** (`/home/daniel/Documents/IMGS-ELONG`, made by
`out/v19-aspect-texture/elongated/make.py`: 33 pages of 2 to 10 seed
photographs from IMGS2-4, each seed used once, 21 vertical and 12 horizontal,
with one section saved alone and one screen-shaped viewport elsewhere on each;
68 positives, every other pair a negative):

| `--work-size` | found, shipped rule | found, area rule | pages of 5+ sections | false pairs | CPU-s |
|---|---|---|---|---|---|
| 384 | 45 / 68 | **62 / 68** | 12/30 -> 28/30 | 0 -> 1 | 5 -> 7 |
| **512** | 57 / 68 | **65 / 68** | 24/30 -> **30/30** | 0 -> 0 | 8 -> 10 |
| 640 | 57 / 68 | 63 / 68 | 25/30 -> 28/30 | 5 -> 2 | 10 -> 12 |
| 1024 | 60 / 68 | 62 / 68 | 28/30 -> 27/30 | 2 -> 2 | 14 -> 18 |

The area rule at 512 finds more than the shipped rule at 1024.

**On the benchmark corpora**, each run and scored on its own, cold, pooled
over the four (`out/v19-aspect-texture/tables.py`; † would mark a merge, and
there is none):

| `--work-size` | pooled F1, shipped -> area | recall | perfect rows (of 4 x 87) | cross-family pairs | CPU (uncooled) |
|---|---|---|---|---|---|
| 384 | 0.9671 -> **0.9704** | 93.95% -> 94.59% | 249 -> 254 | 6 -> 88 | +4% |
| 512 | 0.9798 -> **0.9813** | 96.45% -> 96.75% | 268 -> 272 | 91 -> 91 | +4% |
| 640 | 0.9828 -> 0.9828 | 97.05% -> 97.07% | 276 -> 271 | 86 -> 170 | +3% |

What moves is containment, because the canvases those rows paste a photograph
into are wider than 4:3: at 384 `contact_sheet` 167 -> 188 of 304,
`picture_in_picture` 247 -> 265, `embed_tiny` 154 -> 170, `magazine_spread`
288 -> 298; at 512 `embed_tiny` 261 -> 274, `crop_micro` 224 -> 232,
`contact_sheet` 265 -> 270. At 640 it is a wash, and `wall_poster` and
`pdf_page` lose two seeds each. IMGS alone is the largest mover: 0.9652 ->
0.9703 at 512, 41 -> 52 perfect rows. The cross-family pairs that arrive are
each one stray file joining its sibling's family — `game2`/`game3` (82) at
384, `docks1`/`docks2` (82) at 640 — the clean-anchor rule's lone-file
allowance, not merges. Both seed halves were recorded per run (`results.jsonl`).

**Why it is not shipped yet:** it changes every published number, it costs
3-4% CPU on the benchmark corpora (uncooled figures; the photographs there are
mostly 4:3, the derived canvases are not), and at 640 it trades five perfect
rows for nothing. The case for it is the IMGS-ELONG table, which the benchmark
cannot see, and the default-size gain, which it can.

### Parameters, and the rule about them

**A number earns its place by being derived from something, not by being the
value that scored best.** A threshold can no longer be parked just past a fixed
transformation parameter, since every amount in the catalogue varies per seed,
but it can still be fitted to the seeds in front of it. `benchmark/VALIDATION.md`
records how the parameter surface was cut and what the first held-out corpus
proved; read it before adding a knob back. Applying the rule took the CLI from
13 result-changing options to 5, the acceptance policy from 9 fitted numbers to
2, and the bridge test from 3 to 0, at equal or better F1 every time. Removing
a parameter is the cheap experiment; run it before adding one.

**The names are load-bearing too.** `--min-frame-overlap` and
`--min-pixel-correlation` were `--min-overlap` and `--min-agreement`, which
read as a loose and a tight version of one bar. The nouns now carry the
difference: one is **frames**, pure geometry with no pixel read, the other the
**pixels** in them. `--min-aligned-points` was `--min-inliers`, RANSAC jargon
for "correspondences that agree on one transform". A name that needs a comment
to stop a misreading is the wrong name.

**How to tell whether a number is fitted.** *Sweep it and look at the shape*:
a value on a plateau carries no corpus-specific information, one balanced on a
peak does. *Score the same runs on two disjoint halves of the seeds*
(`pair_relation` is per-seed, so a half is exactly the corpus those seeds would
have given) and check that the shape reproduces. Every knob reproduces its
shape on both halves under two different splits. What the halves cannot catch
is a value chosen because it topped the F1 column — both halves prefer the
same over-fitted value — and they are **blind to merges**: a merge lands in one
half (at correlation 0.30 on the old build, half A 0.8990 against half B's
0.9785), and a near-miss merge usually lands in *neither*, because the two
sibling photographs are split between the halves and the pairs joining them
belong to no half at all. At bar 7 on IMGS3 the whole corpus scores 0.9747
while both halves score about 0.985. Halves test whether a *shape*
generalises, not whether a value is safe.

**What a threshold sweep hides: the cliff.** Every acceptance threshold is
monotone in F1 over the usable range — looser is always better — right up to
the point where two families merge and thousands of false pairs arrive at
once. So sweep for the *cliff*, not the peak, quote the distance to it, and
split false pairs into rearrangement traps and cross-family errors, because
loosening a bar buys traps long before it buys a merge.

**The sweep of 0.30.0** (`out/v23-plain`, 2026-10-05), on the released plain
x86-64 binary: every option on IMGS, IMGS2, IMGS3 and IMGS4 alone and on
IMGS-ALL, at 384, 512 and 640, everything else at the shipped value
(correlation 0.6), plus correlation by hundredths from 0.56 to 0.75 at 640 on
IMGS3 and IMGS-ALL; about 520 runs, each from a cache at its own work size. The
tables below are 512 and 640; 384 is in `results.jsonl` and in the margins. **†** marks a
family merge, a pair of seeds with 300 or more false pairs between them. (A
single stray file joining its sibling's family is about 80 pairs and is not a
merge; IMGS3 has a few at every setting.) `results.jsonl` has precision,
recall, trap and cross-family counts, the seed pairs, both seed halves, the
perfect rows and the clean-anchor rule's joins and refusals for every run, and
`tables.py thr detail margins` prints them. The 0.20.0 sweep these replace —
IMGS, IMGS2, IMGS3 and the three together, at 384 as well — is
`out/v14-fullsweep` and `out/v15-enlargement`.

| `--min-aligned-points` | 512 IMGS | 512 IMGS2 | 512 IMGS3 | 512 IMGS4 | 512 ALL | 640 IMGS | 640 IMGS2 | 640 IMGS3 | 640 IMGS4 | 640 ALL |
|---|---|---|---|---|---|---|---|---|---|---|
| 3 | 0.9798 | 0.9855 | 0.9887 | 0.9888 | 0.9862 | 0.9816 | **0.9740†** | 0.9891 | 0.9885 | **0.9840†** |
| 5 | 0.9791 | 0.9847 | 0.9884 | 0.9877 | 0.9854 | 0.9799 | **0.9737†** | 0.9890 | 0.9883 | **0.9832†** |
| 6 | 0.9777 | 0.9838 | 0.9882 | 0.9865 | 0.9844 | 0.9789 | **0.9731†** | 0.9886 | 0.9889 | **0.9828†** |
| 7 | 0.9749 | 0.9835 | 0.9874 | 0.9858 | 0.9834 | 0.9781 | **0.9726†** | 0.9885 | 0.9883 | **0.9821†** |
| 8 | 0.9721 | 0.9825 | 0.9863 | 0.9855 | 0.9823 | 0.9768 | 0.9834 | 0.9882 | 0.9880 | **0.9811†** |
| 9 | 0.9698 | 0.9814 | 0.9854 | 0.9856 | 0.9806 | 0.9740 | 0.9830 | 0.9879 | 0.9874 | 0.9828 |
| **10** | **0.9652** | **0.9806** | **0.9849** | **0.9838** | **0.9788** | **0.9719** | **0.9817** | **0.9871** | **0.9871** | **0.9815** |
| 11 | 0.9625 | 0.9798 | 0.9835 | 0.9813 | 0.9770 | 0.9693 | 0.9815 | 0.9869 | 0.9862 | 0.9807 |
| 12 | 0.9597 | 0.9783 | 0.9818 | 0.9795 | 0.9749 | 0.9664 | 0.9806 | 0.9863 | 0.9853 | 0.9796 |
| 14 | 0.9545 | 0.9740 | 0.9776 | 0.9761 | 0.9701 | 0.9591 | 0.9790 | 0.9847 | 0.9835 | 0.9769 |
| 16 | 0.9470 | 0.9702 | 0.9725 | 0.9733 | 0.9645 | 0.9551 | 0.9766 | 0.9821 | 0.9822 | 0.9741 |
| 20 | 0.9307 | 0.9632 | 0.9650 | 0.9669 | 0.9544 | 0.9448 | 0.9724 | 0.9769 | 0.9782 | 0.9679 |

| `--min-frame-overlap` | 512 IMGS | 512 IMGS2 | 512 IMGS3 | 512 IMGS4 | 512 ALL | 640 IMGS | 640 IMGS2 | 640 IMGS3 | 640 IMGS4 | 640 ALL |
|---|---|---|---|---|---|---|---|---|---|---|
| 0.3 | 0.9470 | 0.9631 | **0.9591†** | 0.9659 | **0.9587†** | 0.9557 | 0.9650 | **0.9611†** | 0.9703 | **0.9615†** |
| 0.4 | 0.9468 | 0.9635 | **0.9598†** | 0.9662 | **0.9591†** | 0.9561 | 0.9656 | **0.9617†** | 0.9706 | **0.9620†** |
| 0.5 | 0.9473 | 0.9642 | **0.9606†** | 0.9667 | **0.9598†** | 0.9563 | 0.9663 | **0.9622†** | 0.9708 | **0.9626†** |
| 0.6 | 0.9581 | 0.9750 | 0.9793 | 0.9774 | 0.9732 | 0.9665 | 0.9768 | 0.9815 | 0.9816 | 0.9758 |
| 0.7 | 0.9625 | 0.9793 | 0.9843 | 0.9817 | 0.9775 | 0.9704 | 0.9808 | 0.9863 | 0.9851 | 0.9802 |
| 0.8 | 0.9656 | 0.9814 | 0.9850 | 0.9840 | 0.9791 | 0.9722 | 0.9825 | 0.9874 | 0.9875 | 0.9820 |
| **0.85** | **0.9652** | **0.9806** | **0.9849** | **0.9838** | **0.9788** | **0.9719** | **0.9817** | **0.9871** | **0.9871** | **0.9815** |
| 0.9 | 0.9623 | 0.9771 | 0.9809 | 0.9800 | 0.9749 | 0.9673 | 0.9789 | 0.9838 | 0.9836 | 0.9777 |
| 0.95 | 0.9529 | 0.9656 | 0.9701 | 0.9694 | 0.9642 | 0.9555 | 0.9683 | 0.9718 | 0.9728 | 0.9667 |

| `--min-pixel-correlation` | 512 IMGS | 512 IMGS2 | 512 IMGS3 | 512 IMGS4 | 512 ALL | 640 IMGS | 640 IMGS2 | 640 IMGS3 | 640 IMGS4 | 640 ALL |
|---|---|---|---|---|---|---|---|---|---|---|
| 0.2 | 0.9703 | 0.9858 | 0.9882 | 0.9862 | 0.9820 | 0.9754 | 0.9856 | 0.9900 | 0.9897 | 0.9849 |
| 0.4 | 0.9706 | 0.9852 | 0.9881 | 0.9865 | 0.9824 | 0.9758 | 0.9861 | 0.9899 | 0.9895 | 0.9852 |
| 0.5 | 0.9681 | 0.9843 | 0.9875 | 0.9860 | 0.9815 | 0.9748 | 0.9855 | 0.9894 | 0.9890 | 0.9844 |
| 0.55 | 0.9670 | 0.9830 | 0.9863 | 0.9853 | 0.9805 | 0.9737 | 0.9845 | 0.9884 | 0.9884 | 0.9834 |
| **0.6** | **0.9652** | **0.9806** | **0.9849** | **0.9838** | **0.9788** | **0.9719** | **0.9817** | **0.9871** | **0.9871** | **0.9815** |
| 0.65 | 0.9622 | 0.9754 | 0.9816 | 0.9802 | 0.9751 | 0.9689 | 0.9769 | 0.9841 | 0.9834 | 0.9776 |
| 0.7 | 0.9576 | 0.9687 | 0.9746 | 0.9742 | 0.9688 | 0.9639 | 0.9702 | 0.9770 | 0.9776 | 0.9715 |
| 0.75 | — | — | — | — | — | — | — | 0.9674 | — | 0.9618 |
| 0.8 | 0.9359 | 0.9446 | 0.9497 | 0.9510 | 0.9450 | 0.9437 | 0.9473 | 0.9523 | 0.9543 | 0.9484 |

| `-k` | 512 IMGS | 512 IMGS2 | 512 IMGS3 | 512 IMGS4 | 512 ALL | 640 IMGS | 640 IMGS2 | 640 IMGS3 | 640 IMGS4 | 640 ALL |
|---|---|---|---|---|---|---|---|---|---|---|
| 10 | 0.7555 | 0.7477 | 0.7313 | 0.7305 | 0.7465 | 0.7228 | 0.7480 | 0.7628 | 0.7669 | 0.7441 |
| 25 | 0.9588 | 0.9743 | 0.9786 | 0.9792 | 0.9716 | 0.9664 | 0.9759 | **0.9810†** | 0.9816 | 0.9752 |
| 50 | 0.9620 | 0.9779 | 0.9810 | 0.9816 | 0.9743 | 0.9681 | 0.9773 | **0.9820†** | 0.9832 | 0.9770 |
| 100 | 0.9652 | 0.9806 | 0.9847 | 0.9838 | 0.9788 | 0.9719 | 0.9816 | 0.9867 | 0.9870 | 0.9815 |
| **150** | **0.9652** | **0.9806** | **0.9849** | **0.9838** | **0.9788** | **0.9719** | **0.9817** | **0.9871** | **0.9871** | **0.9815** |
| 300 | 0.9652 | 0.9806 | 0.9850 | 0.9838 | 0.9788 | 0.9719 | 0.9818 | 0.9872 | 0.9872 | 0.9816 |
| 500 | 0.9652 | 0.9806 | 0.9850 | 0.9838 | 0.9788 | 0.9719 | 0.9818 | 0.9872 | 0.9872 | 0.9816 |

**Every merge in those tables is two photographs of one scene, and there are
three left**: `docks1` / `docks2` at overlap 0.5 and below (IMGS3 and IMGS-ALL
at 512 and 640, IMGS-ALL alone at 384; 5,900-6,800 false pairs), `bust1` / `bust2` at aligned points 7 and
below on IMGS2 at 640 and 8 and below on IMGS-ALL (6,500-6,600), and `Segovia1`
/ `Segovia2` at `-k` 25 and 50 on IMGS3 at 640 (319 and 322, on the line; see
`-k` below). None is two unrelated families and none is on IMGS or IMGS4.
0.20.0's sweep had six such pairs, and `Segovia` through aligned points and
correlation, `field2` / `field3`, `Acueducto3` / `Acueducto4` and `Henares1` /
`Henares2` now merge nowhere in the grid: the cycle rule of `admit_anchors`,
and under it luma grey, which moved every anchor. Before the clean-anchor rule
the cliffs were all on IMGS — `--min-aligned-points` merged nine families at 7
(8,148 cross-family pairs), `--min-pixel-correlation` the two Excel
screenshots at 0.40 and twelve families at 0.30 (43,540 pairs), `--work-size
896` `beach` with `panoramic3` — and IMGS still makes no merge at bar 3, at
correlation 0.2 or at any work size. The cliffs moved to the corpora with near
misses, and on 0.30.0 they sit here:

| option | distance from shipped to the nearest merge, 384 | **512 (default)** | 640 | 0.20.0 at 384 / 512 / 640, before the cycle rule |
|---|---|---|---|---|
| `--min-aligned-points` 10 | none down to 3 | **none down to 3**, on any corpus or IMGS-ALL | 2 steps (IMGS-ALL at 8; IMGS2 alone at 7) | 3 steps (IMGS3 at 7) / 1 step (IMGS3 at 9) / 2 steps (IMGS2 at 8) |
| `--min-frame-overlap` 0.85 | 0.35 (IMGS-ALL at 0.5, `docks`; IMGS3 alone never) | **0.35** (IMGS3 and IMGS-ALL at 0.5, `docks`) | 0.35 (the same) | 0.35 / 0.55 / 0.25 |
| `--min-pixel-correlation` 0.6 | none from 0.2 to 0.8 | **none from 0.2 to 0.8** | none from 0.2 to 0.8, the hundredths included | none / none / 0.01 below and 0.09 above |
| `-k` 150 | none from 10 up | **none from 10 up** | 100 (IMGS3 at 50, on the line) | 140 / 140 / none from 10 up |

The combined corpus still does not simply add its parts: IMGS-ALL merges `bust`
at bar 8 at 640 where IMGS2 alone holds at 8 and goes at 7, and does not reach
`Segovia`'s line at `-k` 25 or 50 where IMGS3 alone does. A merge of near-miss
siblings depends on which anchors happen to exist, and the vocabulary a corpus
builds changes that — which is also why the work sizes do not line up. Read the
margins as the distance to *a* merge, not to a fixed edge.

**`--min-aligned-points` at the default was the thin one, and the cycle rule
closed it** (see the end of this section). Measured on 0.30.0 by runs rather
than replays, 512 merges nothing at any bar from 20 down to 3, on any of the
four corpora or on IMGS-ALL; what follows is the 0.20.0 build, where it was one
step from a merge.
Recall is still a ramp and F1 still falls at every step above the cliff, so F1
still prefers the lowest safe bar, and the reason not to follow it is the
margin — which at 512 is a single step. Replayed off one `--dump` per corpus
(`replay_ap.py`, reproducing the runs' merges and stray-file counts exactly),
counting cross-family anchors *before* the clusters are made:

| bar | IMGS3 384: cross anchors | clean | merges | IMGS3 512: cross anchors | clean | merges | IMGS2 640: cross anchors | clean, kept by the bridge test | merges |
|---|---|---|---|---|---|---|---|---|---|
| 6 | 15,492 | 1 | **1** | 17,438 | 0 | **1** | 4,334 | 3 | **1** |
| 7 | 14,499 | 1 | **1** | 17,134 | 0 | **1** | 3,741 | 3 | **1** |
| 8 | 13,410 | 1 | 0 | 16,645 | 0 | **1** | 3,114 | 2 | **1** |
| 9 | 12,027 | 0 | 0 | 16,019 | 0 | **1** | 2,504 | 0 | 0 |
| **10** | **10,797** | **0** | **0** | **15,205** | **0** | **0** | **1,992** | **0** | **0** |
| 12 | 8,873 | 0 | 0 | 13,032 | 0 | 0 | 1,255 | 0 | 0 |

Two different mechanisms. On IMGS2 at 640 two or three *clean* anchors
between the two busts survive the bridge test — the photographs agree block
for block, so nothing about the anchor is weak — and the clusters join on
them. On IMGS3, at 384 and at 512, no clean cross-family anchor survives, so
the merge can only come through the weak-anchor rule: some cluster of one
Segovia family ends up with a weak anchor from every one of its files into the
other. At the shipped bar there is no clean cross-family anchor on any corpus
at any of the three sizes, and on IMGS none at any bar above 3.

So the margin at the default is held by `admit_anchors`, and a threshold is
the wrong tool to widen it: 11 at 512 costs 0.3 points of IMGS F1 for one more
step, 12 costs 0.8 for two, and the merge is not a property of the bar but of
which weak anchors a Segovia fragment happens to have.

**Traced, it was not a fragment.** At 512 bar 9 the join that merges Segovia
is an 80-file Segovia2 cluster, held together by 1,131 clean anchors, joining
the 85-file Segovia1 cluster: every one of the 80 has a weak anchor into
Segovia1, 66 of the 85 point back. Every legitimate multi-file join on IMGS3
has 2-5 files and 0-2 clean anchors inside. Counting does not separate them —
38 weak anchors per file across against 28 clean ones inside — and structure
does: after the bridge test a fragment's clean anchors are a pair, a chain or
a star, a family's close cycles. So two clusters that both hold a cycle no
longer join on weak anchors. That alone cost IMGS2 0.1-0.16 points of F1 and
up to 8 perfect transformations, all from two fragments that *are* cycles —
`fort2`'s greyscale/duotone/halftone triangle and a 3-4 file crop/rotation
cycle of `manhole3` — which the second half of the rule lets back in: one file
of the larger has a weak anchor to every file of the smaller (3 of 3, 4 of 4;
Segovia's best is 73 of 80). Tried and rejected on the way: covering the
larger side too (the legitimate fragments are touched back by 90-96% of the
family, Segovia by 78% — the wrong way round for a rule), and the density of
weak anchors across the cut (0.90-0.95 against 0.45-0.52, separable only by a
fitted cut). 10 stays, five steps from the nearest merge at 512 in the replays,
and with none down to 3 when re-measured on 0.30.0.

**`--min-pixel-correlation` at 640 is not monotone.** Swept by hundredths on
IMGS3: `Segovia` merges at 0.55-0.58 (7,028-7,058 false pairs), is clean from
**0.59 to 0.68**, merges again at 0.69 and 0.70 (6,128 and 5,938), and is clean
at 0.75 and 0.8. At 512 and 384 no corpus merges anywhere from 0.2 to 0.8, the
hundredths around 0.6 included. So *tightening* this bar can merge two
families, which no other curve in this file does. Replayed off one `--dump`
(`replay_corr.py`), the band reproduces exactly, and says why. The clean
anchors are the same 157,258 at every bar, none of them cross-family — a clean
anchor's worst block is 0.85, so no correlation bar below that touches it — and
so the clusters are the same too; the bar moves only *weak* anchors, and every
Segovia merge is made by `admit_anchors` joining clusters on them. Those joins
are not monotone in the weak anchors they are given. One more anchor can
complete the cover of a Segovia fragment into its sibling; one fewer can leave
a fragment unjoined to its own family and small enough that every file of it
has a weak anchor into the other. Which of the two happens depends on which
anchors exist, so a bar that only removes anchors can create a merge. It is the
same weakness as the aligned-points margin above, seen through another bar,
and the cycle rule removes it the same way: replayed, IMGS3 at 640 merges at
no correlation from 0.2 to 0.8, and 0.55 runs at F1 0.9878 with 2
cross-family pairs where 0.21 ran at 0.9787 with 7,058. **Re-measured on 0.30.0
by runs** (`out/v23-plain`): F1 at 640 falls monotonically from 0.2 to 0.8 on
every corpus and on IMGS-ALL, every hundredth from 0.56 to 0.75 included on
IMGS3 and IMGS-ALL, and nothing merges. The band is gone.

**`--min-frame-overlap` has a cliff on IMGS3**, and it is the near-miss one:
`docks1` / `docks2`, one quay framed slightly differently. On 0.30.0 it joins
at 0.5 and below at both 512 and 640, on IMGS3 and on IMGS-ALL (5,900-6,800
false pairs); on 0.20.0 it was 0.6 and below at 640 and 0.5 and below at 384,
with `Acueducto3` / `Acueducto4` at 0.3 at 512, which no longer joins. Replayed
on the older build, the cycle rule changed neither, so these are not
weak-anchor joins. On IMGS, IMGS2 and IMGS4 the shape is exactly the
documented one — no cliff anywhere, loosening buys trap pairs by the thousand
(889 at 0.85 to 10,884 at 0.3 on IMGS at 512), and F1 is flat to a few
thousandths between 0.80 and 0.85 (0.8 is a hair *higher* on every corpus at
both sizes), so the step loose would buy nothing but the distance to a cliff.
It does not move.

**`-k` above 100 is unchanged, and below 25 it now costs far more**: identical
to within a handful of pairs from 100 to 500 on every corpus at both sizes, the
knee between 50 and 100, and the shipped 150 half the range clear of it. What
is new is the bottom of the range. At 10, 0.20.0 kept 86-92% recall; 0.30.0
keeps **58-61%** (F1 0.73-0.77), because the propagation budget is
`prop_budget` — as many composed pairs a round as the direct pass verified
candidates, which is files times `-k` — and at `-k 10` it no longer fits the
corpus's families: on IMGS 45,324 a round against 586,715 at the default, and
24 clusters (2,049 files) are **starred**, compared with their root alone;
IMGS-ALL stars 126 clusters, 10,723 files. From 25 up nothing is starred and
the curve is the old one. So the budget couples propagation to `-k`, which was
designed in and is harmless at any setting a user would choose; a `-k` sweep
below 25 now measures the budget, not the candidate list. At 640 IMGS3 puts
319 and 322 false pairs between `Segovia1` and `Segovia2` at 25 and 50 — on the
line, about four stray files' worth, and not traced — where at 10, starred, it
puts none past 300. It is not connected to a result above 100; do not reach for it
to fix anything.

**`--min-frame-overlap` and `--min-pixel-correlation` are not two strengths of
one bar.** They sit one line apart in `Verdict::accepted` and each is the only
defence against a failure mode the other cannot see: the two Excel screenshots
match at median overlap **1.000** and median agreement **0.000**, a
`column_roll` against a crop of its own original at median overlap **0.562**
and median agreement **0.971**. On IMGS, loosening overlap adds traps and
loosening correlation (on the builds before the clean-anchor rule) merged
families. Near misses are the case both can see: a photograph from a step to
the side overlaps partially *and* agrees partially, which is why each has a
cliff on IMGS3.

**Neither of them is a cost knob, and the one cost effect runs backwards.**
Overlap gates `pixel_check`, but the quantity it gates is bimodal — 90% of
direct verdicts eligible on inliers already overlap 0.9 or more — so direct
`pixel_check` is flat at 27-32 CPU-seconds across every setting of either knob
(measured with `--features prof`, propagation off). What moves is propagation,
backwards: a round proposes every **unmatched** pair inside a component, so
*tightening* a bar makes more work. Composed hypotheses go 70,887 -> 75,986 ->
147,876 as correlation goes 0.35 -> 0.50 -> 0.70, and 57,562 -> 75,986 ->
131,989 as overlap goes 0.70 -> 0.85 -> 0.95. Set both for what the tool
should claim, never for what it costs. (Until 0.19.x propagation also dropped
every composed pair at overlap 0.5 or less whatever the flag said; it now
keeps them against the tier's own floor, pair-for-pair identical at 0.85 and
+891 propagated pairs, every one a trap, at 0.3 — `out/v13-propagation-floor`.)

**`--min-aligned-points` is the only bar propagation can overrule.**
`Policy::new` gives the propagated tier `min_aligned_points: 0` — no features
vouch for a composed transform — while overlap and correlation are inherited
by all three tiers, so a pair the inlier bar rejects can come back through
propagation and a pair the other two reject is gone. Its cost is the merge
tax, not the gate: `n_in` is skewed (156,016 of 236,549 direct verdicts carry
50 inliers or more), so the bar turns away a band of ±6% of checks from 6 to
20, but a merged component is enormous and a propagation round proposes every
unmatched pair inside one. On the old build at 640, bar 6 proposed **340,008**
composed hypotheses against the shipped 75,986 and took twice the wall clock.
A setting that costs real time is telling you it has merged something.

**Making it self-adjusting: derivable, measured, and worse.** The bar is a
*count* with no natural scale, the obvious candidate for deriving from the
corpus. `from_single` builds a 4-DoF similarity from one correspondence, so
under a null model of randomly placed correspondences an observed count is
`1 + Binomial(n_match - 1, p)`, with `p` the chance a random correspondence
lands within tolerance:

```
tau = max(0.015 * diag(B), 3)   p = pi*tau^2 / area(B) = pi * 2.25e-4 * (r + 1/r)
```

Every correspondence is tried as a hypothesis, so the run's expected number
of coincidental anchors is `V * min(n_match,600) * P(X >= k-1)`; set that
below alpha and solve for k. It reproduces both numbers it should — **10** at
a typical rich pair and IMGS's 236,549 verified pairs, 8-9 at alpha=1 — and
1000x in alpha or in corpus size moves it two steps. **And it is worse**:
replayed over the old build's verdicts it merged `13.webp` with
`low-light.avif` at every alpha, including one stricter in aggregate than the
flat bar (flat 8-12: no merge; derived alpha 1 / 0.01 / 0.001: one merge
each). Every cross-family anchor it admits and the flat bar rejects is sparse
— `n_match` 8 to 42, `n_in` 5 to 9. The null model bounds **coincidence**,
and coincidence was never the binding constraint: correspondences between two
different photographs exist *because* they share real structure, so
conditioned on there being few of them they are *more* likely to be
consistent, not less. Any rule that hands an individual pair a discount walks
into it. A *per-corpus* scaling never discounts a single pair and is
untested, not refuted.

**How all of that was measured, because it is reusable.** `--dump` sets the
pixel-check gate to `(3, 0.2)` (or looser, if the run's own bars are), so
`blk` and `blk_min` are real measurements for every verdict with three inliers
or more, and *any* bar at or above 3 can be replayed offline against one run:
the anchor tier, `drop_weak_bridges` (a sixty-line Tarjan) and the clean-anchor
rule's joins. `out/v14-fullsweep/replay_ap.py` does it for every
`--min-aligned-points` bar. Sweeping a CLI flag costs a run per value; this
costs one.

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

  **Swept after 0.32.0, and it is a ramp, not a plateau** (`out/v25-slack-
  budget`, IMGS-ALL at 512 from v24's cache, one value at a time, the other at
  its shipped value; `pts2` is the shipped row):

  | slack | F1 | recall | perfect | traps | cross-family | halves alt / hash |
  |---|---|---|---|---|---|---|
  | correlation 0 | 0.9743 | 95.37% | 45 | 4,570 | 3 | 0.9714 / 0.9740 |
  | 0.05 | 0.9765 | 95.80% | 45 | 4,638 | 3 | 0.9738 / 0.9767 |
  | **0.1 (shipped)** | **0.9781** | **96.11%** | **45** | **4,674** | **3** | **0.9756 / 0.9786** |
  | 0.15 | 0.9787 | 96.22% | 46 | 4,688 | 3 | 0.9763 / 0.9792 |
  | 0.2 | 0.9789 | 96.27% | 47 | 4,715 | 3 | 0.9766 / 0.9795 |
  | 0.3 | 0.9790 | 96.29% | 47 | 4,732 | 3 | 0.9767 / 0.9796 |
  | 0.6 (no bar at all) | 0.9790 | 96.29% | 47 | 4,733 | 3 | 0.9767 / 0.9796 |
  | points 0 | 0.9779 | 96.07% | 45 | 4,651 | 3 | 0.9754 / 0.9783 |
  | 1 | 0.9780 | 96.09% | 45 | 4,664 | 3 | 0.9755 / 0.9784 |
  | **2 (shipped)** | **0.9781** | **96.11%** | **45** | **4,674** | **3** | **0.9756 / 0.9786** |
  | 3 | 0.9782 | 96.13% | 45 | 4,694 | 3 | 0.9758 / 0.9787 |
  | 4 | 0.9783 | 96.16% | 45 | 4,704 | 3 | 0.9759 / 0.9788 |
  | 6 | 0.9785 | 96.20% | 45 | 4,747 | 3 | 0.9761 / 0.9790 |

  Both are monotone and neither has a cliff, which is what the tier's
  argument predicts: a corroborated pair joins two files already in one
  cluster, so it cannot merge anything, and the cross-family count is 3 at
  every setting. Looser buys traps a few dozen at a time and recall to a
  ceiling at a correlation slack of 0.3, where the bar inside a cluster is gone
  (0.6 - 0.3 is below every corroborated pair the corpus has). Both halves
  follow the whole. So **neither 0.1 nor 2 is derived**: each is a point on a
  ramp, and the benchmark prefers no correlation bar inside a cluster at all.
  What argues against following it is the thing the corpus cannot see — the
  0.6 default exists so that merely similar photographs are not grouped, and
  every point of slack lets pairs below it into the groups a person deletes
  from (IMGS: 1,620 reported pairs below 0.6, 98% of them true there).
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

**Two numbers in `index::shared` that nothing derived, swept and found
inert; the cut is removed (0.29.1), the cap kept.** A word shared by more than 64 keypoint pairs between two images was
dropped as "repeated texture" (`(i - i0) * (j - j0) > 64`), and the list
stops at `60_000` pairs. Swept with `TEXTURE=N` in
`out/v19-aspect-texture/img-fp-exp` over 8, 16, 32, 64, 128, 256, 1024 and no
cut at all, on IMGS, IMGS2, IMGS3 and IMGS4 each at 384, 512 and 640 (84
cached runs): **pooled F1 is the same to ±0.0001 from 32 to no cut at every
size**, the per-corpus rows move by a handful of pairs, no setting merges
anything, and the CPU column does not move (cached runs, uncooled). The cut
does drop real correspondences — on IMGS at 512 it discards 15,067 words and
3.5 M candidate pairs, and on `derived/Desktop` removing it raised 142 pairs'
aligned points by 20 on average — but none of those pairs was near a bar. At
8 it starts to cost (0.9798 -> 0.9796 at 512). The `60_000` cap binds only
with no cut at all, 8 and 12 times on IMGS at 512 and 640: the two guard the
same pathological case of one word on hundreds of keypoints on both sides.
So the cut went, and the cap stays as the one guard: it is a bound on work
rather than a filter on evidence. Pair-for-pair, the build without the cut is
the sweep's `TEXTURE=1000000000` rows.

**The pixel check's four constants, swept after the 0.30.0 audit**
(`out/v24-constants`, `tables.py`). `THUMB_LONG` 128, `GRID` 48, `BLOCK` 8 and
the flat-block bar of 4 grey levels were in the first commit and had never been
measured. Each was built in at a value (`build.sh`, an `option_env!` read since
removed) and run on IMGS-ALL at 512, cached, everything else shipped; the
thumbnail needs a cold run per size. † is a merge (a seed pair with 300 or more
false pairs).

| run | F1 | recall | perfect | cross-family | halves alt / hash | peak MB |
|---|---|---|---|---|---|---|
| **shipped** | **0.9781** | **96.11%** | **45** | **3** | 0.9756 / 0.9786 | 2,137 |
| thumb 64 | 0.9740 | 95.30% | 43 | 89 | 0.9711 / 0.9744 | 1,923 |
| thumb 96 | 0.9771 | 95.94% | 43 | 355 (`sea_photo` 255) | 0.9745 / 0.9777 | 2,028 |
| thumb 192 | 0.9782 | 96.12% | 45 | 4 | 0.9753 / 0.9789 | 2,518 |
| thumb 256 | 0.9790 | 96.29% | 44 | 91 | 0.9768 / 0.9798 | 2,988 |
| grid 32 | 0.9680 | 96.33% | 45 | **26,247†** | 0.9765 / 0.9639 | |
| grid 40 | 0.9730 | 96.23% | 46 | **13,243†** | 0.9759 / 0.9739 | |
| grid 56 | 0.9761 | 95.73% | 44 | 7 | 0.9728 / 0.9767 | |
| grid 64 | 0.9751 | 95.53% | 45 | 9 | 0.9722 / 0.9755 | |
| grid 80 | 0.9720 | 94.93% | 44 | 7 | 0.9692 / 0.9720 | |
| block 4 | 0.9775 | 96.02% | 45 | 252 (`Henares` 248) | 0.9752 / 0.9775 | |
| block 6 | 0.9779 | 96.06% | 45 | 8 | 0.9756 / 0.9783 | |
| block 12 | 0.9703 | 96.26% | 46 | **20,024†** | 0.9762 / 0.9683 | |
| block 16 | 0.9514 | 96.21% | 46 | **64,571†** | 0.9759 / 0.9582 | |
| flat 0 | 0.9613 | 92.71% | **2** | 7 | 0.9577 / 0.9570 | |
| flat 1 | 0.9775 | 95.98% | 45 | 4 | 0.9749 / 0.9781 | |
| flat 2 | 0.9776 | 96.01% | 45 | 4 | 0.9750 / 0.9780 | |
| flat 3 | 0.9780 | 96.08% | 45 | 4 | 0.9754 / 0.9783 | |
| flat 6 | 0.9788 | 96.26% | 46 | 3 | 0.9762 / 0.9798 | |
| flat 8 | 0.9761 | 96.35% | 45 | **7,298†** | 0.9767 / 0.9745 | |
| flat 12 | 0.9739 | 96.53% | 46 | **14,303†** | 0.9771 / 0.9753 | |

What they say, one at a time:

- **The thumbnail is on a plateau, and 128 is its low edge.** 192 is level
  (0.9782) for 380 MB more peak; 256 is +0.0009 for 850 MB more and puts a
  stray `Segovia` file back; 96 and 64 lose recall. No size merges. It stays.
- **`GRID` and `BLOCK` are one number, and it is the thin one.** What decides
  is the blocks a side, `GRID / BLOCK`: 4 and 5 merge (grid 32 and 40, block
  12; `Segovia`, `Henares`, `bust`, `game`, `sea_photo` — the near-miss
  siblings), 3 merges worst (block 16), and 6 (shipped), 7 and 8 are clean.
  Fewer, larger blocks average a near miss's disagreeing patch away, and the
  clean-anchor rule reads its worst block (`blk_min`), which a coarse grid
  hides. So **the shipped value is one step from a cliff on both axes**, the
  only constant here that is. More samples at the same blocks a side cost
  recall (grid 56, 64, 80 at block 8 are 7, 8, 10 blocks a side, and fall
  monotonically); block 6 (8 a side at 48 samples) is level, 0.9779, clean,
  and two steps from the cliff in the block direction. Not changed: it moves
  every published figure for a margin that has not yet been needed, and it
  should be re-run on the single corpora and at 640 before anyone does.
  Block 4 (12 a side, 16 samples a block) lets a stray `Henares` file in:
  blocks that small are noisy.
- **The flat bar's plateau runs from 1 to 6, and 8 merges.** At 0 no block
  abstains, and dim or plain pictures fail the check: 45 perfect rows to 2,
  which is the defect the bar exists for. 6 tops the F1 column (0.9788) one
  step from `Segovia`'s merge at 8; 4 is two steps from it and stays.
  Loosening it buys traps and then a merge, the documented shape.

The seed halves do see these merges, unlike the thresholds' near-miss ones:
the hash half falls with every one (0.9582-0.9753 against the shipped
0.9786) while the alt half rises, so a split of the siblings across halves
cannot be counted on in either direction.

### Speed and memory, and what has already been tried

**A build that cannot assume AVX2 now picks its kernels when it starts**
(`src/simd.rs`, `build.rs`). `cargo install` reads no `.cargo/config.toml` from
the package, so a crates.io install was built for plain x86-64: the hand-written
AVX2 kernels were compiled out (they were chosen with
`cfg(target_feature = "avx2")`) and every loop the compiler vectorises ran four
lanes wide. On this machine that cost **+52% CPU in matching** (cached IMGS:
102-104 CPU-s against 67-69) and **+24% cold** on `derived/Desktop` (60-62
against 49-50, extraction 34-35 s against 24-27), same pairs. Now:

- `v3()` is `const true` in an AVX2 build, so the release and native builds
  compile to what they did; in a `cfg(dispatch)` build it asks the CPU once.
- The six hand-written kernels (`Query`'s dot products, `dist2`,
  `blocks_meet`, the pixel check's `wide`, `halve_row`, `grey_row_avx2`) are
  compiled on every x86-64 build and taken when `v3()` says so.
- `dispatched!` gives `sift::extract`, `lib::quantise`, `InvertedFile::query`,
  `index::shared`, `verify::verify` and `verify::verify_transform` a second
  copy compiled for x86-64-v3, with what they call marked
  `cfg_attr(dispatch, inline(always))` so that it is compiled inside the copy.
  **Watch for anything that keeps code out of line**, because it then runs in
  the plain copy and nothing says so: a closure handed to `LocalKey::with`, an
  `Option::map` or a `collect` wrapping a hot function, a generic `extend`.
  The first version wrapped the extractor in a closure and recovered nothing;
  `objdump` on the `x86-64` binary, counting `ymm` per symbol, is how each
  leak was found (`blur_*` through `BLUR_SCRATCH.with`, `Grad::of` through
  `Option::map`, `pixel_check` through `PYRAMIDS.with`).

Measured, three rotated rounds, 20 s apart, all four builds pair-for-pair
identical (222,879 on IMGS, 2,631 on Desktop): cached IMGS **67-70 CPU-s plain
against 67-68 native** (HEAD native 67-69); cold Desktop **50-52 against
48-50** (HEAD native 49-50), extraction 25-27 s against 24-25. `extract_threads`
under dispatch: 23.1 / 16.1 ms single-threaded against native's 25.3 / 16.3, and
the documented checksums `b8e5341a1d7b5cad` / `5c2fd85ed71708a8`.
`the_plain_and_the_v3_copies_agree` holds the two copies to each other bit for
bit (features, words, query scores, intersection, verdicts); it exists only in
a `cfg(dispatch)` build, so run `RUSTFLAGS="-C target-cpu=x86-64" cargo test
--release --lib` to see it, as `release.yml` does. Rustdoc compiles without
`.cargo/config.toml`'s flags while `build.rs` sees them, so the cfgs are
written to cover that disagreement too (it is merely slow).

**So should the release be the portable build? Measured, and no**
(`out/v20-portable`, on the build after the 0.28.0 audit fixes). The case was
that the dispatch build had measured level with native, and that a release
for plain x86-64 would run on the CPUs without AVX2 the v3 release refuses.
Two things stand against it, and neither was visible before:

- **It is not the same code on JPEG XL.** `jxl-grid` chooses fused
  multiply-add with `cfg(target_feature = "fma")`, at compile time, with no
  run-time path, so a build without FMA decodes a JPEG XL a rounding apart.
  Every other format is bit-identical across x86-64, x86-64-v2 and x86-64-v3
  (checked per format on `derived/Desktop`), and so is the extractor
  (`extract_threads` gives the documented checksums under all three) — but
  `derived/Desktop`'s 29 JPEG XL files moved 60 dump rows. On the corpora it
  is a vocabulary-sample ripple, not a loss in kind: IMGS, IMGS2, IMGS4 and the
  found corpus came out pair-for-pair identical, and IMGS3 lost 364 pairs
  (1,431 out, 1,067 in, 13 and 7 of them touching a JPEG XL file), F1 0.98534
  -> 0.98486, perfect rows 79 -> 74. The portable binary would not be the
  benchmarked one.
- **It is slower everywhere, by 2-5%.** Cold, cooled, cache-evicted,
  `--no-cache`, in the order v3, plain, plain, v3 (IMGS: three rounds of v3,
  plain, v2 in rotation), CPU-seconds: IMGS 349.7 -> 356.9 (+2.1%; v2 375.4,
  +7.3%), IMGS2 446.7 -> 462.5 (+3.5%), IMGS3 623.3 -> 649.6 (+4.2%), IMGS4
  392.6 -> 401.6 (+2.3%), found 754.6 -> 790.6 (+4.8%); wall the same within a
  point. The dispatched functions are the hot loops the profiler names, but
  everything else — the decoders above all — compiles four lanes wide: 6,824
  `ymm` instructions in the plain binary against 37,519 in v3's. The earlier
  level reading was a cached run, which skips the decoders.

The release stayed x86-64-v3 until 0.30.0, which ships the plain build: it
runs on any x86-64 CPU, the pairs it loses on IMGS3 are within reach of the
settings (below), and the profile below puts it level with v3 stage for stage.
The published figures were then a build away from the shipped binary on JPEG
XL, by `jxl-grid`'s FMA rounding, IMGS3's row in particular (F1 0.98486
against 0.98534 at the default); **since `out/v23-plain` they are the shipped
binary's**, and `bench.py` measures `target/plain` unless told otherwise.

**Re-checked on 0.30.0** (`out/v23-plain`, `compare.py`). The downloaded
release and a local `RUSTFLAGS="-C target-cpu=x86-64"` build give
byte-identical reports on IMGS, IMGS2, IMGS3, IMGS4, the found corpus and
`derived/Desktop`. Against the native build, IMGS and the found corpus are
byte-identical; IMGS3 moves exactly v21's set again (1,302 true pairs out, 935
in, 129 and 132 false); IMGS2 and IMGS4 have the same pairs and groups but 22
and 4 pairs whose `aligned_points` differ by one (and one `scale` in the third
decimal); `derived/Desktop` moves 13 pairs out and 18 in. The two IMGS2 JPEGs
behind one of those pairs analyse byte for byte the same under both builds
(their cache records are identical), so the difference is the vocabulary,
which is trained on a sample that holds the corpus's JPEG XL files. **With
every `.jxl` left out (`-x '!jxl'`), plain and native are byte-identical on
IMGS2, IMGS3 and IMGS4** — so JPEG XL is the whole of the difference, IMGS3's
2,500-pair ripple included.

**And the cost, A/B'd rather than profiled**: cold, cooled to idle + 3 C,
evicted, `--no-cache`, A B B A, plain against native. IMGS-ALL 2,330 / 2,284
CPU-seconds against 2,266 / 2,290, wall 332 / 325 s against 324 / 326; the
found corpus 764 / 771 against 748 / 748, wall 105 / 106 s against 104 / 103.
That is **+1.3% of the CPU on IMGS-ALL and +2.6% on the found corpus**,
averaged over both orders, with wall within 1-2% and the same peak (2,167-2,184
MB on IMGS-ALL, no swap). It sits between `out/v20-portable`'s +2-5% (0.28,
each corpus alone) and the profile's "level"; why it is smaller than v20's is
not traced, and it is not zero.

**Profiled, the plain build is level with v3 stage for stage**
(`out/v22-plain-cpu`, IMGS-ALL, cold, `--features prof`): 2,967 against 2,977
CPU-seconds, every hot stage within a few per cent. The two that are not:
`sift:base` 15-18% heavier against `sift:blur` in the same run (about 12
thread-seconds; the base blur's plain copy, not found in the listing), and
`decode:png` about 10% heavier against `decode:jpeg` (the `png` crate, which
no copy here can reach). JPEG's IDCT already picks AVX2 at run time inside
`zune-jpeg`. So what is left to take out of the plain build is work, not width.

**And the pairs the portable build loses are not lost for good**
(`out/v21-plain-recovery`, 0.29.1). Run as v3 and as plain x86-64 at the
defaults, IMGS, IMGS2 and IMGS4 are pair-for-pair identical; IMGS3 moves 1,302
true pairs out and 935 in. Of the 1,302, v3 had found 1,046 by propagation,
193 by corroboration and 63 directly — the ripple runs through the clusters,
not through the JPEG XL files. Plain's own dump says why it missed them: 407
never reached a verdict with three points; of the direct verdicts, 508
failed the aligned-points bar (303 on it alone), 130 correlation alone and 95
overlap with or without correlation; 125 were propagated and failed on
correlation, 26 were mirrored or inverted, and 11 cleared all three bars and
lost to the cluster rules. Loosening plain's settings brings back
**every one of the 1,302** under some setting: `--min-aligned-points` alone
recovers up to 77% (9: 30%, 7: 74%), `--min-pixel-correlation` alone 48%,
`--min-frame-overlap` alone 23%, `-k` 3%; points 7, overlap 0.7 and
correlation 0.5 together recover 97% with no merge (F1 0.98853 against v3's
0.98534 at the defaults), and looser combinations reach 99% only by merging
families. Plain at `--min-aligned-points 9` alone already scores above v3 at
the defaults (0.98544).

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

**Those three shares are at `--work-size 640`, and the default has been
below it since.** `--work-size` scales the first four fifths of the pipeline and the
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
changes above leave it at `48a0f69e907049e6` / `6efa669a9cb66fb9`. (Since
`response` left `Keypoint` in the second memory pass the checksum covers four
fields, and reads `b8e5341a1d7b5cad` / `5c2fd85ed71708a8` — on the build
before that change as well, summed over the same four.)

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

**A memory pass, and where each corpus's peak really was.** Every change is
byte-identical: the same 216,009 pairs and 140 groups here and the same 3,778
and 794 on the found corpus, every verdict field compared — and, more strictly,
**all 5,637 per-image cache records byte-identical** between the two builds on
this corpus, which is the grey plane, the keypoints, the descriptors and the
thumbnail of every file. Measured cold, cooled and cache-evicted, with
`bench.py`'s own PSS sampler:

| | peak PSS before | after | CPU-seconds, slot-matched pairs |
|---|---|---|---|
| benchmark corpus | 812 / 776 / 775 / 759 / 751 MB | 709 / 716 / 738 / 695 / 689 MB | 327 → 312, 318 ← 368, 318 → 307, 329 ← 314, 335 → 317 |
| found corpus | 1,140 / 1,140 MB | 1,052 / 1,052 MB | 784 → 823, 823 ← 824 |
| benchmark, cached, one machine cache holding both | 1,233 MB | **488 MB** | 12.9 s → 10.1 s wall |

About **-90 MB (-12%) cold here, -88 MB (-8%) on the found corpus**, and -60%
for a cached run whose cache also holds another corpus — which, since the cache
is one file per machine, is the ordinary way to run this. CPU is level to
better: four of the five pairs here are the new build faster by 3-6%, the fifth
the other way by 16% on a run 170 MHz slower; the found corpus's matched pair is
level. The pass is not aimed at the clock; where it is faster it is because
less memory was moved.

The finding that decided what was worth doing: **the two corpora peak in
different places, and neither was where the paragraph on peak memory above
said.** Measured with a sampler reading `mallinfo2` every quarter-second:

- **This corpus peaked inside a single JXL decode.** jxl-oxide's render holds
  six float planes at its widest — 306 MB for a 13.5-megapixel file — and
  `decode_jxl` then streamed the result into a second whole-picture float
  buffer (154 MB) and packed a byte copy of that (38 MB). At ~500 MB it was
  larger than the decode budget it had claimed 364 of, and it ran alone on top
  of four hundred megabytes of analysis. The render now streams a row at a time
  straight into the reduction (`reduce_rows`), and the claim is the render's
  own 8 bytes a sample. HEIF lost its packed copy the same way.
- **PNGs are decoded a row at a time** (`decode_png_rows`), straight from the
  `png` crate into the box reduction. The corpus has 114 PNGs past four
  megapixels and a 44-megapixel one is 177 MB of RGBA; now it is a row. The
  reduction became a row-fed `Reducer` that the whole-buffer path uses too, so
  the two are one piece of arithmetic, and anything the stream is not sure it
  reads identically — interlaced, animated, sixteen-bit, an EXIF chunk, an
  error of any kind — takes the old path, error message and all.
  `png_rows_decode_exactly_as_the_whole_picture_does` holds them together.
  JPEG has no equivalent: zune-jpeg has no row API.
  **Since 0.33.0 that includes a frame larger than `max_alloc`**, which the
  row path used to send to the general path so that it would be refused the
  same way. The general path then held the whole frame: a 32000x32000 RGB PNG
  peaked at 3.0 GB of RSS, in swap, for 20 s on this 6 GB machine; through the
  rows it is 31 MB and 3.0 s (a 12000x12000 one was already 18 MB). The limit
  is there to turn an allocation larger than memory into an error, and the
  row path makes none. Sixteen-bit PNGs still take the general path.
- **The found corpus peaked twice, at the same height**: during the vocabulary
  build and during the second look. The build widened its 160,000-descriptor
  sample to floats (82 MB) and carried each level's k-means centres back as
  floats before rounding them to bytes (~70 MB at the deepest level). The
  sample stays bytes now and each member is widened as k-means reads it —
  exact, so every distance and sum is the same float — and the centres are
  rounded inside the parallel map. The spike is gone.
- **The second look's peak** lost the word lists' keypoint index (`u32` to
  `u16`, 25 MB), the million candidate pairs held through it for nothing, and
  the thumbnails' mip pyramids, which are now built on the first pixel check
  that needs one (`Thumb::mips`) — a third of every thumbnail, most of which a
  found corpus never reads. (Since the second memory pass, below, they are not
  kept at all.)
- **And a cached run unpacked the whole machine's cache.** `cache::open`
  inflated every record in the file, including every record for a path this
  run was not walking, and held them through the analysis so that their spans
  could be carried over — which needs a key and a span and nothing else. With
  the found corpus in the same cache that was 900 MB of someone else's
  analysis; that record is now framed and skipped. What is given up is
  noticing a damaged compressed body before a run that walks its path.

What is left, so that nobody goes looking for it: on the found corpus the peak
is **875 MB of analysis** — descriptors 625, thumbnails 152, keypoints 98 — plus
the index, and none of that can shrink without being lossy. Packing a word and
its keypoint into one `u32` would take another 25 MB off the word lists, at the
price of shifts inside `shared`'s block filter, the matcher's hottest loop; not
tried. (Tried since, and shipped: see the second memory pass.) Here the peak is now the JXL render itself on top of the analysis, and
past that the decode budget, which is `MemAvailable / 8` and so is a choice
about the machine rather than a property of the build. (The 8 is measured
since; see *The decode budget's divisor* below.)

**A second memory pass, for the four-corpus baseline.** `out/v17-all4` put
img-fp's peak at **3,051 MB PSS**, third highest of the field, and the first
thing worth knowing is that **IMGS-ALL does not peak where IMGS does**. IMGS
alone peaks during the analysis, on a decode transient; 27,659 files peak at
the **end of verification**, where everything the analysis made is held
beside everything matching has built on it. Every change below is
byte-identical on all four corpora together, on IMGS and on the found corpus —
the same 1,118,629 pairs and 702 groups, 222,879 and 140, 2,438 and 770 — and
the `--dump` CSV's direct and variant rows are byte-identical too. (Its
propagated rows come out in a different order on every run of any build, the
baseline included, because propagation walks a `HashMap`; the set is the same.)

The cold end-to-end figure first, one session, `bench.py`'s protocol (cooled to
idle + 3 C, the corpus evicted from the page cache, `--no-cache`), in the
order final, base, base, final, with **swap added to PSS**, because this
machine runs at swappiness 100 with gigabytes already swapped out and the
baseline build's peak was being paged out under it:

| IMGS-ALL, cold | peak PSS + swap | of it swapped | wall | CPU-seconds |
|---|---|---|---|---|
| **before** | 3,115 / 3,117 MB | 311 / 399 MB | 474.5 / 470.5 s | 2,835 / 2,853 |
| **after** | **2,282 / 2,256 MB** | 0 / 0 | 458.9 / 457.0 s | 2,888 / 2,860 |

**-27% of the footprint and -3% of the wall**, the wall being the swapping
that stopped. CPU is level: +1.8% in the first pair and +0.2% in the second,
with every "after" run at a clock 2-3% lower than its partner. PSS alone — all
`bench.py` records — reads 2,804 / 2,718 MB before, so a `bench.py` row would
show -18%; the larger figure is the honest one. On the found corpus, in the
same order: peak **989 / 988 MB against 1,108 / 1,126** (-11%), CPU 882 / 953
against 921 / 918 (one pair each way, following the clock). On IMGS alone,
three pairs: CPU level (497 / 492 against 493 / 498 in the matched pairs) and
the peak unmoved at 680-810 MB, because IMGS's peak is the analysis and none of
this touches it.

How it was found, which is reusable: a sampler of `/proc/self/smaps_rollup`
against the run's `-v` stage lines says *when*, and `mallinfo2` at each stage
boundary says *what* — heap in use, heap free, and mmapped. At the baseline's
peak the itemised data was 2,470 MB of 2,776 (cached, no swap), and
**`mallinfo2` reported 451 MB of the heap free**. The changes, in the order
they were made, with cached IMGS-ALL figures (each with no swap in its run):

- **Verification is collected a slice at a time** (`VERIFY_SLICE`, 65,536
  candidates). A rayon `collect` of a filtered stream gathers each worker's
  results in vectors of their own, grown by doubling, and copies them into one
  at the end: 923,601 verdicts at 112 bytes were some 300 MB at that moment.
  The holes it left were the 451 MB, and trimming them (`malloc_trim` after
  verification was tried) gives back 400 MB that the second look dirties again
  within seconds. Slices appended in order are the same list: **2,776 ->
  2,547 MB**, heap free at the peak 451 -> 25.
- **The anchors are read out of `all_direct` where they lie** instead of being
  cloned into a second list: 876,049 of its 923,601 verdicts, with growth
  slack.
- **The vocabulary's sample was copied twice**: the pool, then a shuffled copy
  of the pool inside `Vocabulary::build`. Shuffling in place is the same draw.
  The vocabulary stage, cold: 2,592 -> 2,478 MB.
- **And k-means' results were the level twice over**: every parent's centres
  collected, then copied into the level, 133 MB each way at the deepest level.
  The parents now go a slice at a time (`BUILD_SLICE`), each parent's seed its
  own index as before. Vocabulary stage 2,411 -> 2,259.
- **The postings are dropped after the candidates and built again for the
  second look**, which is the only thing that reads them after retrieval: 94
  MB off the end of verification for 1.8 s on one thread. And where the
  re-asked files are few, their mirrored and inverted word lists are quantised
  first and the vocabulary dropped before the postings are rebuilt, so the two
  never coexist. That is decided by size — the lists against
  `Vocabulary::heap_bytes` — because on the found corpus nearly every file is
  re-asked and the lists would outweigh the vocabulary several times over;
  there each file's are made and spent in turn, as before.
- **A word-list entry is one `u32`**, the word above `KP_BITS` and the keypoint
  below, which the entry above this one called not tried: 130 -> 87 MB. The
  block filter compares `entry | KP_MASK` on both sides, one `or` per word.
  `shared_timings` says it costs 16% of the function on synthetic lists and
  nearly double on lists sharing nothing, but in place on IMGS-ALL, profiled,
  `shared` is 48.7-48.9 CPU-seconds against 49.0-49.3: on real candidates the
  merge is emission and sorting, not filtering. A variant that broadcast the
  masked words with `vpermd` instead was worse everywhere.
- **A leaf of the vocabulary is a pointer to the descriptor it copies**
  (`Vocabulary::leaves`). The tree is sized so that its leaves outnumber its
  sample, so a parent at the level above holds about three members, and k-means
  hands a parent with no more members than children each member as a centre
  of its own — byte for byte, which the debug build asserts. Those members are
  descriptors the analysis holds until after the vocabulary is dropped, so the
  leaf level, 1.04 M leaves and 133 MB of copies on IMGS-ALL, is now 8 MB of
  pointers and the few real means in `own`. The vocabulary goes from 181 MB to
  81. The descent's last level reads its children through the pointers
  (`dists_at`, the same integer kernel): profiled, `quantise` read 175 against
  162-166 CPU-seconds, in a session too short of memory to trust, and end to
  end it does not show — the found corpus, where quantisation is a quarter of
  the run, is level above. Holding the *sample* as references too — 9 MB
  rather than a 145 MB copy — was measured and rejected: k-means reading its
  members from all over 1.4 GB built the tree 30% slower (13 s -> 17-18 s).
- **A thumbnail's pyramid is built for each comparison and not kept**
  (`Pyramid`, two per worker). It is a 2x2 mean, sixteen outputs to a few
  vector instructions (`halve_row`, held to the old scalar arithmetic by
  `the_pyramid_is_built_exactly_as_it_was`), and almost every IMGS-ALL
  thumbnail is read above level zero, so keeping it was a third again of every
  thumbnail: **103 MB**. `pixel_check` profiled at 129.5 / 130 CPU-seconds
  against 123-135.
- **A direct verdict is held as `(u32, u32, Verdict)`**, 76 bytes, and becomes
  an edge where it is read (`direct_edge`): an edge carried its transform
  beside the verdict that holds it and its inversion beside the verdict's own
  variant. 99 -> 66 MB.
- **`response` left `Keypoint`**: 216 -> 173 MB. See the cache section; the
  cache is `IMGFPC06` for it.

Together, cached: **2,776 -> 2,228 MB**, and what is at the peak now is the
analysis — **descriptors 1,386 MB, thumbnails 303, keypoints 173** — with 87 of
word lists, 81 of vocabulary, 66 of verdicts and 22 of candidate pairs on top.
The phases that can peak are within 170 MB of each other, measured cold: the
end of analysis ~2,120, the vocabulary build ~2,260, and the end of
verification and the second look ~2,290.

What is left is the analysis, and it is lossless only at a price. The
descriptors carry **5.85 bits of order-0 entropy a byte** (17% zeros, 57% under
16, 97% under 128), so no fixed-width packing gains anything, and an entropy
coder with random access — a code per byte, an offset per descriptor — would
save some 22-26% of 1,386 MB in exchange for a decode inside `correspond` and
the descent, the two hottest loops in the run, hundreds of millions of times.
Not tried. Thumbnails and keypoints are read by the pixel check and the
geometry at full precision. The decode budget, `MemAvailable / 8` taken once
at the start, sets the analysis-phase transient, and that phase is not the
peak here.

**A pass over the four-corpus baseline's CPU, and the one change in it that is
not exact.** Profiled cold on IMGS-ALL, the starting point was 2,946
user-seconds, of which decoding was 1,109 thread-seconds (JPEG 568), the
extractor 1,508 and everything after it about 450. Six of the seven changes
below are exact — pair-for-pair and field-for-field on all of IMGS-ALL, and the
JSON report byte-for-byte — and one changes what grey *is*, so its accuracy is
measured rather than asserted.

- **Grey is BT.601 luma, and a JPEG's is its Y plane** (`decode::luma`,
  `decode_jpeg_luma`). It was the mean of R, G and B. A JPEG stores luma and
  chroma, and asking `zune-jpeg` for `ColorSpace::Luma` skips the chroma's
  IDCT, the upsampling and the colour conversion, and hands back one byte a
  pixel rather than three: 400 of IMGS2's JPEGs decode in **2.6 s against 3.8**
  on one core, and in place `decode:jpeg` against the untouched `sift:ori` went
  from 5.59 to 4.03 of it (**-28%**). Every other format computes the same Y
  from its RGB (integer weights 299/587/114, divided once), so a JPEG and a PNG
  of one picture still give the same grey, and PIL's own `convert("L")` — which
  is what the corpus's greyscale transforms are — is now the identity. A
  colour space with no Y plane (CMYK, YCCK, RGB stored as such), a picture past
  `max_alloc` and any error take the general path, so a broken file fails with
  the message it always did. Measured on IMGS-ALL at the default, against the
  build before it:

  | build | F1 | precision | recall | perfect | cross-family | halves (alt / hash) |
  |---|---|---|---|---|---|---|
  | mean of RGB (`out/v17-all4`'s build) | 0.9781 | 99.58% | 96.11% | 43 | 74 | 0.9759 / 0.9787 |
  | luma | 0.9789 | 99.57% | 96.27% | 49 | 93 | 0.9768 / 0.9799 |
  | luma and the streamed resample below | **0.9788** | **99.57%** | **96.24%** | **46** | **89** | **0.9762 / 0.9793** |

  Both seed halves move up with the whole. The cross-family pairs are still no
  merge: the stray `Acueducto3` file (68 pairs) is gone and a stray `Segovia`
  one (84) has taken its place, which is the clean-anchor rule's lone-file
  allowance on the other sibling pair, and the rest are single pairs. The
  cache went to `IMGFPC08` with it.
- **The area resample runs vertical first, and is fed the reduction's rows as
  they come** (`decode::Fit`). The full-size grey plane a picture already near
  the working size used to be written into, and read back by `fit_to`, no
  longer exists: each reduced row is added into the one or two output rows its
  area covers while it is still in cache, and the horizontal pass then runs
  over `th` rows rather than all of them, every output over the same number of
  taps (`horizontal::<T>`, zero-padded) so that its inner loop has a constant
  trip count. Not exact against the old order — the two passes are summed the
  other way round — so it is in the accuracy row above; `streamed_fit_is_fit_to`
  holds the streamed form to `fit_to` bit for bit. `reduce_timings`: l8
  1200x900 **2.75 -> 0.83 ms**, rgb8 2.9 -> 1.36, `resize_area` 1000x750 ->
  512x384 **1.48 -> 0.61**; in place `decode:fit` **133 -> 30** thread-seconds
  and `decode:reduce` 130 -> 58. (A gather version of the horizontal pass was
  no faster: Zen 2's gathers cost what the scalar loads did.)
- **The cache deflates at level 1** (`cache::pack`). The descriptors are nine
  tenths of a record and hold nothing for an LZ to find, so level 6's longer
  match search only spent time: 60 records of IMGS2 packed to **3,095,465 bytes
  in 222 ms at level 6 and 3,078,257 in 42 at level 1** — smaller too, since
  the Huffman stage sees longer runs of literals. Every record is packed by the
  worker that made it, so with the cache on, which is the default, this was
  some 3 ms an image of a cold run. The reader is unchanged; a cache written
  at level 6 reads as it did.
- **An octave is made a row at a time** (`sift::Octave`). The five blurs are a
  chain and nothing reads a Gaussian whole, so each blur is a stage that makes
  its next row on demand from the rows below it, each Gaussian lives in a ring
  of `r + 1` rows (three at least, for the gradient), and what is written whole
  is what is read whole later: the differences, the gradients and the halved
  base of the next octave. Exact — the same `blur_row`, the same vertical sums
  in the same order (`vertical_row`), the same gradient expressions — and
  `extract_threads` still reads `b8e5341a1d7b5cad` / `5c2fd85ed71708a8`.
  `octave_threads` (new) at 512x384: planes 11.1 ms on one core and 34.7 each
  on eight, streamed 10.8 and 29.9, so the octave now scales like pure
  arithmetic (x2.78, where the planes were x3.13); in place, blur, gradients
  and halving together went **717 -> 640** thread-seconds. (So `sift:blur` in a
profile is now the whole octave, gradients and halving included, and
`sift:grad` and `sift:halve` stay empty.) The driver is a loop
  and not a recursion, on purpose: a recursive function cannot be inlined into
  the extractor's x86-64-v3 copy, and `objdump` on a `target-cpu=x86-64` build
  shows no `Octave` symbol outside `extract::v3`. In a harness without
  `few_arenas` the rings' small allocations interleaved with the planes made
  glibc trim and refault 1,700 pages an image; the real binary does not, and
  measures fewer minor faults than before.
- **`best_transform` counts a rival against the best hypothesis's outliers
  first.** A count does not depend on the order and a rival is only asked
  whether it beats the best, so the order decides nothing but how soon a loser
  is known — and most rivals on a real duplicate are the best's own inliers,
  which explain none of its outliers and are now stopped at the end of them.
  Point tests on IMGS **5.11 G -> 2.72 G**, output identical. The stage's time
  barely moved, which is the finding: `best_transform` is bound by gathering
  the two images' keypoints, not by the counting.
- **The decode budget claims what a decode now holds.** A JPEG's claim is one
  byte a pixel, for the luma plane, where it was three (a JPEG with no Y plane
  takes the rest through `cover`, as any decoder does), and the working plane
  is what `Fit` holds rather than a full-size float plane. Workers waiting on
  the budget (`decode:permit`) went **19.6 -> 0.5** thread-seconds on IMGS-ALL.
- **The report's tail runs on every thread.** The 1.12 M output pairs were
  built, sorted by path and serialised on one thread after everything else had
  finished: they are now made in parallel, sorted with rayon's stable sort
  under the same comparator, and the `pairs` list is serialised in pieces
  written in order (`list_par`, held to `list` byte for byte by
  `the_pairs_are_written_on_every_thread_byte_for_byte`). The last stretch of
  an IMGS-ALL run went **7.6 -> 4.9 s** of wall, the report byte-identical.

Tried in this pass and not kept, each measured:

- *Two descriptor histograms taken in turn by consecutive samples*, so that a
  sample's eight read-modify-writes do not wait on the previous sample's
  stores. **-14% on one core and level at eight** — the stall it removes is one
  the sibling hyperthread was already filling — and not exact.
- *The k-means centre update summed on every thread*, exactly (the `f64` sums
  of bytes are integers). Bit-identical and no faster: the update was never
  where the vocabulary's ten seconds go.
- *Hamerly bounds in the vocabulary's k-means*, with margins wide enough to
  keep every assignment exact. In 128 dimensions the nearest and second-nearest
  centres are too close for a triangle-inequality bound to separate: **15%** of
  measurements skipped, **22%** with the own-centre re-measure. Not worth the
  code. Padding 13-wide centre tables to 16 lanes measured no different either
  (170 against 177 ns a call).
- *Reading the next file ahead* with `posix_fadvise(WILLNEED)` while a worker
  analyses the current one. Level on a cold IMGS run (wall +0.5 and +1.0 s in
  the clock-matched pairs): on this SSD the reads are not on the critical path.

One thing outside the code: this machine's libheif decodes AVIF through
`libheif-plugin-aomdec`, because `libheif-plugin-dav1d` is not installed, and
dav1d is the faster AV1 decoder. `decode:heif` is ~90 thread-seconds of an
IMGS-ALL run.

**What the pass is worth end to end**, on IMGS-ALL under `bench.py`'s
protocol (cold page cache, cooled, `--no-cache`, PSS and swap sampled), in the
order before, after, after, before:

| IMGS-ALL, cold | CPU-seconds | wall | peak PSS + swap | clock |
|---|---|---|---|---|
| **before** | 3,047.7 / 2,878.7 | 486.1 / 454.2 s | 2,257 / 2,252 MB | 1,797 / 1,905 MHz |
| **after** | 2,690.9 / 2,564.4 | 421.5 / 407.8 s | 2,267 / 2,262 MB | 1,799 / 1,889 MHz |

Slot-matched, **-11.7% and -10.9% of the CPU and -13.3% and -10.2% of the
wall**, at matched clocks. The peak does not move, and should not: on this
corpus it is the end of verification, where the analysis is held whole, and
nothing here changed what the analysis holds. What the pass took out of the
analysis phase's transient — a JPEG's decode buffer a third the size, no
full-size grey plane before the resample, no Gaussian planes in an octave — is
below that peak here, and is what a corpus that peaks in its analysis gets.

**An EXIF orientation is applied to the working plane, not to the picture**
(`decode::orient`, since the audit after 0.30.0). The decoded picture used to
be turned at full size before it was reduced: a second full-size buffer for the
turn, walked column-wise, a cache miss a pixel — and a phone stores most of its
portraits that way. Twenty-four 4032x3024 JPEGs at `-t 1`, orientation 6
against the same pictures stored upright: decode 3 s against 2, total 3.83
against 2.98 CPU-s and 46 MB against 34 MB peak before; **3.36 against 3.24
CPU-s and level on memory after**. The mapping is `apply_orientation`'s pixel
for pixel (`orienting_the_plane_is_orienting_the_picture`); what moves is which
edge the box reduction trims when the size does not divide, so the cache went
to `IMGFPC10`. The PNG row path now reads an `eXIf` chunk and orients the same
way instead of falling back to the whole-picture path.

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
- **The claim is made before the file is read** (`decode::Claim`), from the
  header probe the progress stage takes (`decode_estimate`), and a decoder
  takes whatever it finds it needs beyond that through `cover`, without
  queueing. The claim used to be made by the decoder, after the read, so every
  worker waiting for room already held its whole file: six 75 MB BMPs peaked at
  176 MB on one thread and 535 MB on six, and 184 MB on six after. A file the
  head says is no picture is turned away before it claims anything.
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

### The decode budget's divisor, measured

**8 is the knee, and it stays.** The budget is `MemAvailable / 8` (at least 64
MB), and nothing derived the 8. Swept on IMGS — the corpus whose peak is in
the analysis, with its 44-megapixel PNGs and JXL renders — cold, evicted,
`--no-cache`, `-t 8`, cooled to idle + 3 C, `--features prof` for the time
workers spent waiting on the budget (`decode:permit`, thread-seconds), with
2.9-3.0 GB available (`out/v25-slack-budget/budget.py`):

| divisor | budget | waiting on it | peak PSS + swap | CPU-s | wall |
|---|---|---|---|---|---|
| 1 | 2,997 MB | 0.0 s | 701 MB | 494 | 75 s |
| 2 | 1,493 MB | 0.0 s | 713 MB | 485 | 74 s |
| 4 | 746 MB | 0.0 / 0.0 s | 800 / 844 MB | 466 / 479 | 71 / 73 s |
| **8** | **372 MB** | **5.7 / 9.8 s** | **751 / 739 MB** | **378 / 479** | **57 / 74 s** |
| 16 | 185 MB | 32 s | 648 MB | 464 | 74 s |
| 32 | 92 MB | 73 s | 564 MB | 474 | 81 s |

Pairs identical at every divisor (223,557). From 1 to 4 the budget never
binds, and the peak is the analysis plus whichever large decodes happen to
coincide (701-844 MB, no order in it). At 8 it starts to: under ten
thread-seconds of waiting in some 480, and the slot-matched pair against 4
(479 / 74 s against 479 / 73 s) is level on the clock while 50-100 MB lower.
Past 8 the waiting grows fast — 32 s at 16, 73 s at 32 — for 90 and 180 MB
off the peak. So the shipped value is the first one that does anything, and
the step past it starts to cost. The CPU and wall columns otherwise follow
this machine's temperature, not the budget (the first run, cooler, is 378
against 479 for the same setting); the waiting and the peak are what the
budget owns. On a machine with more free memory the budget is larger and
binds less, which is the point of taking it from `MemAvailable`.

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
thresholds against that offline instead of re-running the tool per guess. Its
paths are the files' own bytes, quoted as CSV quotes them (they were written
with Rust's `{:?}` until 0.19.x, which split a name holding a comma or a quote
into two columns); its `variant` rows are every mirrored or inverted verdict
with three aligned points, as the `direct` rows are, where they used to be the
accepted ones only; and a propagated pair is one row, holding the last round's
verdict, where every round used to add one — 57 of 349 rows on
`derived/Desktop` were repeats. A `variant` pair can still appear twice, once
from each end, because those are two verdicts. The
cache is on by default and is what makes tuning the matching stages practical:
on the 5,638-image corpus a cold run at 384 is ~57 s and a cached one ~12 s
(at 640, ~82 s and ~17 s; the default, 512, sits between), and each record
carries the extraction settings that made it, so a run reads only the records
made at its own `--work-size` — which also means one cached extraction serves a
whole threshold sweep at a given work size, and that is how
`out/v14-fullsweep` was taken. A sweep's runs after the first also write
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
knee, not a peak**, and the shipped default, 512, sits one step below it.
0.30.0, the released plain build (`out/v23-plain`): accuracy on all four
corpora and on IMGS-ALL, each run once at its size; pairs on the found corpus,
which has no ground truth; cost on IMGS and the found corpus from one cold,
cooled, cache-evicted session on `bench.py`'s protocol (idle 70-75 C, every run
at 2.6-3.1 GHz, which this machine reaches in a cool morning; the 0.20.0 table
this replaces ran at 1.5-2.8 GHz, so compare the ratios, not the seconds). The
0.20.0 rows are in `out/v14-fullsweep` and `out/v15-enlargement`.

| `--work-size` | IMGS F1 | IMGS recall | IMGS2 F1 | IMGS3 F1 | IMGS4 F1 | ALL F1 | ALL recall | IMGS CPU-s | IMGS wall | found pairs | found CPU-s |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 64 | 0.0493 | 2.53% | 0.1048 | 0.0873 | 0.0979 | 0.0828 | 4.32% |  |  | 76 |  |
| 96 | 0.5141 | 34.60% | 0.5625 | 0.5868 | 0.5775 | 0.5420 | 37.18% |  |  | 452 |  |
| 128 | 0.6838 | 51.96% | 0.7507 | 0.7626 | 0.7717 | 0.7342 | 58.01% | 128 | 22 s | 808 | 69 |
| 192 | 0.8459 | 73.38% | 0.8763 | 0.8794 | 0.8848 | 0.8635 | 76.03% |  |  | 1,705 |  |
| 256 | 0.9028 | 82.45% | 0.9335 | 0.9342 | 0.9381 | 0.9223 | 85.75% | 174 | 26 s | 2,028 | 195 |
| 320 | 0.9288 | 86.94% | 0.9554 | 0.9571 | 0.9596 | 0.9487 | 90.49% |  |  | 2,028 |  |
| 384 | 0.9507 | 90.90% | 0.9704 | 0.9723 | 0.9711 | 0.9640 | 93.37% | 250 | 35 s | 2,028 | 195 |
| 448 | 0.9606 | 92.76% | 0.9764 | 0.9806 | 0.9794 | 0.9740 | 95.30% | 300 | 42 s | 2,472 | 703 |
| **512 (default)** | **0.9652** | **93.63%** | **0.9806** | **0.9849** | **0.9838** | **0.9788** | **96.25%** | **342** | **47 s** | **2,472** | **758** |
| **640 (the knee)** | **0.9719** | **94.92%** | **0.9817** | **0.9871** | **0.9871** | **0.9815** | **96.80%** | **416** | **57 s** | **2,472** | **768** |
| 768 | 0.9774 | 96.02% | 0.9832 | 0.9879 | 0.9868 | 0.9827 | 97.05% |  |  |  |  |
| 896 | 0.9768 | 95.86% | 0.9844 | 0.9875 | 0.9865 | 0.9838 | 97.27% |  |  |  |  |
| 1024 | 0.9758 | 95.70% | 0.9845 | 0.9869 | 0.9870 | 0.9832 | 97.16% |  |  |  |  |
| 1280 | 0.9764 | 95.86% | 0.9845 | 0.9880 | 0.9874 | 0.9832 | 97.17% |  |  |  |  |
| 0 (full size, `-t 2`) | 0.9749 | 95.57% | 0.9825 | 0.9870 | 0.9865 | – | – | | 437 s | | |

Precision is not in the table because it does not move: 99.4-100% at every
size on every corpus, a little *higher* at the small sizes, which find fewer
pairs and so fewer traps. What the work size trades is recall.

**Every column is monotone up to 768, and up to 0.20.0 none was below 512.** A small
picture is enlarged before it is described (`upsample_below`, below). Until
0.20.0 that was decided from the picture `extract` was handed and up to 512
whatever the working size, so two things went wrong. A picture the working size
had shrunk was enlarged again: at 256 or 128 every photograph was analysed at
512, at 512's cost, from a quarter or a sixteenth of the detail, so 256 cost
43% more CPU than 384 and 128 cost what 448 costs and found less than 384
does. And a picture already small was enlarged *past* the working size: a
120-pixel thumbnail was analysed at 480 at `--work-size 140`, so on a library
of small images the option did nothing at all — the found corpus gave the same
pairs at the same cost at every size from 224 up. Now a picture is enlarged
only up to the working size (`lib::enlarge_below`: `min(work, 512)`, and the
whole 512 at `0`), which also means a picture the working size shrank is never
enlarged, since one doubling would pass it. **Nothing is analysed above
`--work-size`**, so the option bounds what every picture costs, and the found
corpus now answers to it: on 0.30.0, 808 pairs at 128, 2,028 at 256-384,
2,472 from 448, where its 224-pixel photographs start being doubled — at 3.6
times the CPU (195 CPU-seconds at 384, 703 at 448; on 0.20.0, 830, 1,706 and
2,438 pairs, at five times).
At 512 and above nothing changes, pair for pair and group for group against
0.20.0 (222,879 pairs at 512, 224,488 at 640); below 512 every row is new. The
cache format went to `IMGFPC05` with it. The old rows are in
`out/v14-fullsweep`; their two small-size merges (256 joined `Segovia1` /
`Segovia2` on IMGS3, 64 joined `game2` / `game3` on all three) were products of
the enlargement and are gone — no size merges a family now.

What the small sizes are worth is what they cost, and it is not much: the floor
is decoding, about 145 CPU-seconds on IMGS whatever the size on 0.20.0 (the
whole run at 128 is 128 on 0.30.0, after the luma decode), and below 96
there is too little picture left to describe. **There is still no floor to
set**: nothing breaks, nothing merges, and the curve says plainly what each
step buys. (`sift::extract` returns no features under 8 pixels a side, so
sizes 1 to 4 exit 2 listing every file as featureless. A floor of 128 shipped
briefly on a justification — the pixel check's 128-pixel thumbnail — that
`Thumb::build` does not bear out; the comment above the argument parsers in
`lib.rs` says so.)

Above 512 recall saturates rather than growing in proportion, because what
resolution buys is concentrated in one kind of row. On 0.30.0, from 640 to 768
IMGS's misses fall from 11,834 to 9,261 and then stop (9,651 at 1280), and the
rows that move are containment ones — `contact_sheet` 55 -> 58 -> 61,
`embed_tiny` 53 -> 55 -> 57, `crop_micro` 40 -> 43 -> 46, `picture_in_picture`
60 -> 62 — while `thumbnail` and `scale_small` lose a seed or two by 1280. A photograph that fills its frame is already described well at 640;
one that is a small part of its canvas is not.

**The default is 512 and the knee is 640, which is a choice rather than a
measurement.** 512 is where the enlargement limit stops binding, so a small
picture is analysed exactly as it is at 640 and only large ones lose detail;
on 0.30.0, 640 buys 1.3 points of IMGS recall and 0.55 of IMGS-ALL's, nearly
all of it in the containment rows, for **21% more CPU on IMGS** (416 against
342 CPU-seconds) and 1% on the found corpus — where 0.20.0 charged about 40%.
384 costs 27% less on photographs and 74% less on a library of small ones, and
gives up 2.7 points of IMGS recall and 18% of the found corpus's pairs. Above
640, IMGS2, IMGS3 and IMGS4 are flat to 0.003 and IMGS gains 0.005 at 768 and
nothing more by 1280; nothing there is worth asking for.

**The cliff at the top is gone.** On the builds before the clean-anchor rule
896 merged `beach` with `panoramic3.jpg` (2,207 cross-family pairs) and full
size made 14 cross-family pairs; now no size merges a family on any corpus
(on 0.30.0, every size from 64 to 1280 on all four corpora and on IMGS-ALL).
Full size costs two to three gigabytes for a 44-megapixel scale space, which
the decode budget does not cover — run it at `-t 2` (IMGS: 541 s, 2.9 GB
peak on 0.20.0; on 0.30.0 437 s and 827 CPU-seconds, peak 3.5 GB PSS plus 1.0
GB swapped, uncooled; IMGS2 3.0 GB, IMGS3 2.3, IMGS4 1.7). IMGS-ALL was not run
at full size. It scores below 768 on IMGS and level with 640 elsewhere.

**How much a small picture is enlarged (`upsample_below`), swept.** `extract`
doubles a picture while twice its long side still fits in 512 — and, since
the change above, in the working size — so at the default a 224-pixel file is
analysed at 448 and a 28-pixel crop at 448 too, sixteen times over. The 512 was
in the first commit and had never been measured. Why
enlarge at all, when it adds no detail: the detector's smallest scale is fixed
in pixels — the pyramid starts at a blur of 1.6 — so structure at the pixel
scale of a small picture is smoothed away before the search begins, and a
picture of a few dozen pixels has almost no octaves to search. Enlarging moves
that structure to where it can be found. Swept at 384 and 640 on the build
that had stopped enlarging pictures the working size shrank but still enlarged
small ones up to the target whatever the working size (`out/v15-enlargement`; the found corpus has no ground truth, so its column
is pairs, and its CPU is uncooled and swings ±20% between identical runs —
512 and 768 do the same work on it):

| rule | 384 IMGS | IMGS2 | IMGS3 | all 3 | 640 IMGS | IMGS2 | IMGS3 | all 3 | found pairs | found CPU-s |
|---|---|---|---|---|---|---|---|---|---|---|
| no enlargement | 0.9389 | 0.9640 | 0.9694 | 0.9566 | 0.9598 | 0.9766 | 0.9825 | **0.9693†** | 1,706 | 405 |
| up to 256 | 0.9431 | 0.9664 | 0.9717 | 0.9602 | 0.9654 | 0.9819 | 0.9863 | 0.9775 | 1,706 | 406 |
| up to 384 | 0.9466 | 0.9696 | 0.9700 | 0.9601 | 0.9659 | 0.9819 | 0.9858 | 0.9788 | 1,706 | 407 |
| **up to 512 (shipped)** | **0.9481** | **0.9709** | **0.9714** | **0.9619** | **0.9706** | **0.9835** | **0.9863** | **0.9799** | **2,438** | **1,137** |
| up to 768 | 0.9508 | 0.9682 | 0.9723 | 0.9635 | 0.9749 | 0.9826 | 0.9874 | 0.9812 | 2,438 | 1,387 |
| up to 1024 | 0.9502 | 0.9707 | 0.9717 | 0.9634 | 0.9767 | 0.9802 | 0.9883 | 0.9811 | 2,783 | 3,044 |
| 512, at most 2x | 0.9475 | 0.9694 | 0.9707 | 0.9622 | 0.9683 | 0.9809 | 0.9858 | 0.9792 | 2,438 | 1,388 |
| 512, at most 4x | 0.9490 | 0.9701 | 0.9714 | 0.9633 | 0.9717 | 0.9821 | 0.9863 | 0.9796 | 2,438 | 1,389 |
| 1024, at most 2x | 0.9480 | 0.9685 | 0.9728 | 0.9624 | 0.9714 | 0.9796 | 0.9875 | 0.9804 | 2,438 | 1,370 |
| 1024, at most 4x | 0.9499 | 0.9688 | 0.9706 | 0.9638 | 0.9735 | 0.9788 | 0.9876 | 0.9805 | 2,783 | 3,167 |
| up to the work size | 0.9466 | 0.9696 | 0.9700 | 0.9601 | 0.9708 | 0.9827 | 0.9870 | 0.9808 | 1,706 | 434 |

**It stays at 512, with no cap on the factor.** Enlargement is worth having on every corpus —
none at all costs IMGS 1.7 points of recall at 384 (`thumbnail` 60 -> 45,
`scale_small` 59 -> 47, `crop_micro` 38 -> 26) and the found corpus 30% of its
pairs — and F1 climbs through 256 and 384 to 512 on IMGS and IMGS2 (IMGS3 is
flat from 256). From 512 to 1024 it is a **plateau whose sign depends on the
corpus**: IMGS gains up to 0.6 points of F1, mostly `crop_micro` (38 -> 48 at
384), IMGS2 loses up to 0.3 and IMGS3 gains up to 0.2, while 1024
quadruples every picture under 256 pixels and costs the found corpus 2.7 times
the CPU for 14% more pairs. A plateau is where a constant belongs, and 512 is
its low edge. The **cap** settles the other half: at most 2x, the standard
SIFT doubling and the one factor with a clean derivation, costs up to a
quarter of a point everywhere, and at most 4x is within a seventh of a point
of no cap either way, so factors past 2 earn something on the smallest crops
and nothing past 4 needs ruling out. No rule here merges a family except none
at all, on the three corpora together at 640. What ships is 512 *and* the
working size, whichever is smaller — the "up to the work size" row is that
rule at 384, and from 512 up it is the 512 row — chosen not for F1 but so that
`--work-size` bounds the cost of every picture; see *Measured trade-offs*.

`--features` is gone: measured over 300 to 900 at a fixed vocabulary it moves
F1 by 0.007 and 900 costs 11% of the run for nothing, so 600 is a constant in
`main.rs`. It had looked load-bearing, and that was the vocabulary step, not the
detector.

## The window

`img-fp-gui`, behind the `gui` feature so that `cargo install img-fp` needs no
GTK. The CLI binary is unchanged by it: output, `--help` and pairs are
byte-identical to the build before the split, and its size moved 15 KB (the
colour `decode::preview`, which only the window calls).

- **The scan is a child process, not a thread**, so that Cancel is exactly
  Ctrl-C: the window re-runs its own binary as `img-fp-gui --worker RESULT
  <img-fp argv>`, which is `execute` with the progress line spoken as JSON on
  stdout (`progress::speak_json`) and the report also written as JSON to
  `RESULT`. That copy has the groups and no `pairs`, which the window never
  read and which were 289 MB of IMGS-ALL's 303 MB report, written to
  `$XDG_RUNTIME_DIR` (a tmpfs, 581 MB here); it is parsed with
  `gio::spawn_blocking`, off the main thread. Cancel sends SIGINT; the worker's existing handler answers it in
  **30-60 ms** measured, keeping the cache. A thread could not be stopped from
  inside libheif or a large decode. Exit hands back all the scan's memory.
  `PR_SET_PDEATHSIG` makes a dying window take the worker with it (checked
  with SIGKILL). It fires on the death of the *thread* that spawned, which
  is the GTK main thread; keep spawning there.
  **It runs `/proc/self/exe`, not `current_exe()`.** A package upgrade
  renames a new binary over the open one, and `current_exe()` then reads
  `…/img-fp-gui (deleted)`: every scan failed with "No such file or
  directory" until the window was restarted (reproduced on Xvfb). A worker
  ended by SIGKILL is "did not answer the cancel" only when Cancel was
  pressed; otherwise it is named as killed, since the OOM killer sends the
  same signal and was being reported as the user's own cancel.
  **And it is started by a fork and `execv` of its own** (`scan::spawn`),
  not `std::process::Command`, since 0.33.1. `Command` references
  `pidfd_spawnp` and `pidfd_getpid` weakly at `GLIBC_2.39`, and the loader of
  every older glibc printed "weak version `GLIBC_2.39' not found" on each
  start (RHEL 9, Leap 15.6), though nothing failed. With a `pre_exec`,
  `Command` forked anyway, and `spawn` is that fork step for step: three
  `dup2`s, SIGPIPE to its default, the death signal, exec. GLib's spawn takes
  UTF-8 arguments, and a path need not be. `release.yml` now holds the
  window to glibc 2.34 as it does the CLI; checked in Docker on Rocky 9.8 and
  Leap 15.6 (scan, Cancel, Trash, the window killed mid-scan).
- **The GTK base follows the desktop's text colour** (`follow_dark_text`).
  A plain GTK 4 app gets GTK's light default plus the user's
  `~/.config/gtk-4.0/gtk.css`, and desktops that theme libadwaita put a whole
  dark theme there, which leaves buttons light under light text: unreadable,
  as it was on the author's Mint-L-Dark. The window asks for the dark variant
  when its styled text is light or the theme name says dark. The same file's
  `--accent-bg-color` warnings are GTK 4.14 not knowing CSS variables, and
  harmless. The window's own CSS goes in at USER priority so the marked-card
  highlight survives such a stylesheet.
- **Mnemonics are always underlined** (`keep_mnemonics_visible`). Cinnamon
  holds a passive X grab on the left Alt key (found with `xdotool key
  XF86LogGrabInfo` and `/var/log/Xorg.0.log`), so a bare Alt press never
  reaches any app there and GTK's show-while-Alt-is-held cannot work. GTK
  applies the flag only to labels that exist when it is set, so it is set
  after the pages are built, and on each secondary window after its child.
- **The picked card is not the focused card.** `Results::picked` is the
  last card clicked or reached with the keyboard, outlined by a `picked`
  class; the image menu acts on it. Using focus instead broke every button
  for the mouse, since clicking a button moves focus to the button. (The user's
  stylesheet also left GTK's own focus ring invisible.)
- **The scan log is not modal**, and there is one of it (`App::show_log`).
  It fills in while a scan runs, so a modal log cost Cancel and the cards
  its lines are about; the button raises the open one instead of stacking a
  copy.
- **Animations are off** (`gtk-enable-animations`), at the user's request.
- **F1 opens the list of keys** (`show_shortcuts`), which is the only place
  the non-mnemonic keys are written down; the pages carry a one-line
  "F1 keyboard shortcuts" and nothing more.
- **"Theme parser error" warnings from GTK are dropped** (`quiet_theme_errors`):
  they are about the user's own `gtk.css`, printed on every start, and
  nothing here can act on them. Every other log message passes through.
- **The window's copy of the report is written before `-o`'s**, and the
  per-file facts both state are read once. A `-o` that failed used to end the
  run first, so a finished scan came back as a failed one with nothing to
  show; reading the facts once takes `main:output` from 0.30 to 0.27 s on a
  cached IMGS scan with `-o`, identical reports either way. **And the window
  reads it on exit 1 too** (`setup.rs`, when the file is there), showing the
  results and an alert naming what failed: until 0.25 it read the report only
  on exit 0 or 2, so the fix above never reached anyone (checked on Xvfb with
  `-o /dev/full`).
- **Released beside the CLI** by `release.yml`: the CLI is built first and
  without the feature, the window after it, and the DT_NEEDED check holds the
  CLI to no GTK. The worker is smoke-tested headless on an empty folder.
- **Rendered with cairo by default** (`GSK_RENDERER` still overrides). The GL
  renderer loads Mesa's libLLVM: idle PSS 100 MB against 73 MB.
- **Thumbnails are decoded again, in colour**, on two threads at most, only
  for the group on screen (a group change drops the queue) plus the next
  group's first 16; textures are kept to a 96 MB budget (`KEEP_BYTES`), by
  bytes rather than by count, since one large view outweighs sixty cards.
  A card is drawn from the desktop's freedesktop thumbnail when one is
  current (`Thumb::MTime` equal to the file's) and at least the card's size
  (`thumbs::from_cache`: `large`, then `x-large`, `xx-large`), so a folder a
  file manager has shown costs a few KB of PNG a card rather than a full
  decode; the large view always decodes the file. A picture already being
  decoded is not queued again (`Queue::in_flight`).
- **The pictures take the page and the words wait to be asked for**, by the
  user's choice among four mocked layouts. A group is a mosaic
  (`gui/mosaic.rs`): every picture at its own shape, in rows sized so the
  whole group fits the visible height, and only a group whose rows would fall
  under 150 pixels scrolls, at 200 a row. Never cropped to a uniform tile,
  because a crop and its original would then look alike. Nothing is written
  on a picture but state: a `Reference` pill, a yellow corner on a weak match,
  a red border and a red tick on a marked one (the picture dimmed, not
  covered, so you still see what you marked). Pointing at an image, or
  reaching it with the keyboard, puts its details in the bottom bar
  (`refresh_bar`): name and folder, size against the reference's, overlap and
  correlation, the file that holds it when the content rule deletes it, and
  the suggestion. Otherwise the bar says what is marked. The tick circle,
  shown on hover, marks; a double click or Enter opens the large view.
- **There is one selected image** (`Results::selected`, outlined), set by a
  click or by the arrow keys, and everything that acts on "this image" acts on
  it. The bar shows whichever moved last, the pointer or the selection
  (`follow_pointer`), so an arrow key shows its image even with the pointer
  resting on another. Nothing is selected when a group opens; the first
  arrow selects the first image.
- **The page's keys are the window's** (capture phase, only while the results
  page shows). Attached to the page they were never seen while nothing on it
  had the keyboard, since GTK then hands a key to the window alone. They leave
  Up, Down, Home, End, Space and Enter to the strip when it has the keyboard,
  and Space and Enter to a focused button. The strip activates on Enter or a
  double click only (`activate_on_single_click(false)`): activating takes the
  keyboard to the images, and on every single click that sent Up and Down to
  the images instead of the groups.
- **Measured on IMGS-ALL (703 groups of up to 91), it was laggy, and three
  things were.** Found with a 10 ms main-loop tick that reports when it runs
  late, on Xvfb, against a saved report. (1) `gtk::Picture` asks for its
  texture's size, so every texture arriving re-laid out the whole page, strip
  of 703 rows included: 40-120 ms stalls many times a second until the last
  picture came. `still.rs` is a picture whose size never depends on what it
  shows; a texture only redraws it. (2) A group change built ninety tiles
  afresh, 80-100 ms. Tiles are now made once and reused (`Results::tiles`,
  `Mosaic::show` hides the spare ones), tile `i` always being image `i`, so
  its handlers never change. (3) What was left of a group change, ~45 ms, was
  `set_tooltip_text`: about a quarter of a millisecond a call. Tooltips and
  style classes are set only when they change (`set_tooltip`, `set_class`),
  and the strip's rows have none (their badges say the same). A group change
  is now ~5 ms.
- **Marking in bulk is one pass over numbered files** (`State`, `mark_ids`).
  Mark suggested deletions and Unmark all groups on IMGS-ALL (17,850 files)
  held the window for 0.9 s each: every file was marked on its own, recounting
  its group and rewriting its strip row, with paths hashed at every step. Now
  each file has a number given when the results arrive (`Member::id`), marks,
  sizes and suggestions are vectors by number with the marked count and bytes
  kept as they change, and each group's row is redrawn once, only when its
  count moved (`Row::shown`). Measured, function then frame: 890 ms -> 12 ms
  (Mark suggested), 860 ms -> 10 ms (Unmark all); what remains is the one
  repaint every image needs, 50-80 ms on Xvfb with cairo. Two smaller things
  went with it: the tick's tooltip no longer changes with the mark (90 changes
  were 20 ms), and a marked picture is dimmed by a veil drawn over it rather
  than by CSS opacity.
- **The mosaic is fitted to `page_size`, told after the allocation that
  changed it** (`idle_add_local_once` on the vertical adjustment's notify):
  queueing a resize inside the viewport's own allocation is what that avoids.
  Its tiles are laid out with `PAD` around them, since the picked outline is
  drawn outside a tile and the viewport clips it otherwise.
- **Arrow keys move by the layout** (`Mosaic::vertical_neighbour`): left and
  right through the group's order, up and down to the nearest tile of the row
  above or below.
- **Groups are a strip of pictures**, each a plain copy of the group's
  photograph (`face`: of the size most members share, since copies and edits
  keep a photograph's size and every canvas it was pasted into has its own;
  neither inverted nor mirrored; suggested KEEP first), with a count and,
  once any is marked, a red count of marks. The reference image was used
  first, and on IMGS-ALL it was as often a small photo on a black canvas; the
  file most often named as `kept_copy` was tried next and is no better, since
  a composite holds the photograph whole and is named for it. Its pictures come from
  a loader of their own with one decoder (`Thumbs::with_workers(1)`), so a
  group change neither drops them nor waits for them; the rows near the shown
  group are asked for first. Its scroller has `propagate_natural_width` off
  and a minimum content width: a picture's natural width is the size it was
  decoded at, twice what it is shown at, and propagated it doubled the strip.
- **What applies to one image or one group is a menu**: the ☰ button in the
  header, and the same `gio::Menu` on a right click, the Menu key or
  Shift+F10 (*View large*, *Open*, *Show in folder*, *Mark all except this*,
  *Unmark group*, *Unmark all groups*). Each is also Alt and its letter from
  anywhere on the page, which the menu shows beside it (`MENU`; the labels
  test checks them with the page's own letters). The image items are off
  while no image is selected. This replaced a
  wrapping row of six buttons above the cards, which had needed two
  load-bearing `FlowBox` settings on GTK 4.14 to stay out of them.
- **Mnemonics live in `gui/labels.rs`** and a test checks each set of
  controls visible together for clashes. Add a label there, not inline.
- **The Trash dialog names the groups in which every image is marked**
  ("In groups 4, 9 and 12 every image is marked, so no copy of those pictures
  would be left"; past twenty, the first twenty and how many more). That is
  the whole of the warning, by the user's decision: a group with one file
  left unmarked is not flagged, although its other members were compared
  with the representative and not with that file.
- **Hidden folders are a checkbox** (*Include hidden folders*, `--hidden`),
  off like the CLI's; the window's scans are recursive, which is why the walk
  skips them by default.
- **Nothing is pre-marked**, by the user's decision; marks are per *file*,
  since groups overlap. **The suggestion is offered, not applied**: the bar
  says "suggested: keep", "suggested: delete" or "weak match" for the image
  pointed at (`suggestion`; the user's wording), a REVIEW image has a yellow
  corner, and *Mark suggested deletions* (Alt+D) makes the
  marks exactly the rule's DELETEs in every group, unmarking everything else,
  hand-made marks included (the user's decision: running a second rule
  replaces the first), and is off while the marks already are that set.
  *Suggestion rule* (Alt+R) picks `--suggest`'s rule without a scan: the
  content rule's actions come with the report, the other two are worked out
  from the groups by `img_fp::by_group`, and all three are computed when the
  results arrive, because once a representative is in the Trash its group no
  longer says what was kept for its files. The choice lasts for the session.
  The report `-o` writes is the command line's, the content rule unless the
  scan was given another. *Scan settings* (Alt+S; it was *New scan*, which
  read as starting one) goes back to the setup page and keeps the results. No group of IMGS-ALL's 694 or
  the found corpus's 771 is DELETE throughout, so the Trash dialog's "every
  image is marked" warning is not set off by the suggestion alone. Trash is `gio::File::trash`, never a delete.
- **Only paths persist** in `$XDG_CONFIG_HOME/img-fp/gui.json`, written when
  a scan starts: folders, excludes, and the report, log and cache file names.
  Every option opens at the command line's default. It used to save every
  option on every scan, touched or not, which froze each default at whatever
  it was on the user's first scan: a window from 0.17 kept asking for `.dds`
  after 0.19 dropped it, and exited 2 on every one. The one deliberate
  difference from the CLI is `recursive`, on by default in the window.
  Folders given on the window's command line (`%F` from the desktop entry,
  which declares `inode/directory` for "Open With") **replace** the
  remembered ones for that session; they used to be appended, so opening the
  window on one folder scanned every folder it had ever been pointed at.
- **Testing on the real display:** Cinnamon's focus-stealing prevention
  ignores `xdotool windowactivate`, and keys then go to whatever window has
  focus. Activate with `wmctrl -i -a`, and check `xdotool getactivewindow`
  before every key sent.
- To see it without a desktop: `Xvfb :99`, run with `DISPLAY=:99`, drive it
  with `xdotool` (after `windowfocus --sync`) and capture with `import -window
  root`. Point `XDG_DATA_HOME` into a scratch folder on the same filesystem
  before testing the Trash.
- **Other distros, in Docker.** Build with `RUSTFLAGS="-C
  target-cpu=x86-64"` as the release does, mount the binary into
  `ubuntu`, `debian`, `fedora`, `archlinux`, `opensuse/*`, `almalinux`
  images, install that distro's GTK 4 and libheif, and drive it on an Xvfb
  inside the container. Use `--network host`: the bridge has no IPv6 route
  and the mirrors answer DNS with IPv6 first, so `apt-get update` hangs. With
  the host's network the container shares X's abstract socket, so give each
  one its own display number. `--no-install-recommends` leaves out the SVG
  loader, which makes Breeze print `Gtk-CRITICAL` and lose its tick marks;
  install `librsvg2-common` before blaming the app. Almalinux 10 has no Xvfb;
  point its container at an Xvfb on the host instead.
- **Wayland, in Docker.** sway: `WLR_BACKENDS=headless WLR_RENDERER=pixman`,
  `setcap -r /usr/bin/sway` first (Docker will not exec a binary with file
  capabilities), `grim` to capture, `wtype -s 400` to type (without the
  pause the first key of each call is lost). GNOME: `gnome-shell --headless
  --wayland --no-x11 --unsafe-mode --virtual-monitor WxH` inside
  `dbus-run-session`, after starting a system `dbus-daemon`, removing
  `/run/systemd` (or it asks logind) and turning hot corners off (the
  virtual pointer starts at 0,0). Capture with `org.gnome.Shell.Screenshot`;
  input through `org.gnome.Shell.Eval` with a virtual device from
  `global.stage.context.get_backend().get_default_seat()`, created once and
  kept on `globalThis`. It starts in the overview: hide it with
  `import("resource:///org/gnome/shell/ui/main.js")`, then `activate` the
  window. In the run that worked, a pointer click on Scan came before the
  first key; whether keys alone are enough was not tested.

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

Every report gives one overlap per pair, the larger of the two ways round,
which is what `--min-frame-overlap` is compared with — so a crop and a
photograph pasted into a poster read 1.00, as a copy does. Reporting both
directions was built and taken back, by the user's decision: the larger
figure reads better. `--dump`'s `ov_a`/`ov_b` still have both.

`--dump -` and `--log-file -` are stdout, as `-o -` is, and a run where two of
the three would share it is refused before it starts (`stdout_has_one_reader`).

img-fp's own `groups` are **not** a closure — each is a representative plus the
files that matched it, and it names the representative (see `src/group.rs`):

```json
"groups": [{"group": "group_1", "representative": "/path/a",
            "files": [{"path": "/path/a", "role": "representative", ...},
                      {"path": "/path/b", "role": "match", "relation": "direct", ...}]}]
```

Each entry of `files` is the CSV report's row for that file, keyed by its
columns — the evidence is the member's pair with *this* group's
representative. Earlier builds wrote bare paths there, which is what every
file under `out/v*` holds; `score.py` reads both (a dict gives its `path`).

The `pairs`-wins rule still holds for img-fp too, for a different reason than for the
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
are not a pair). "No picture" includes text that merely starts like one:
`BM`, `P1`-`P7`, `00 00 01 00` and `GIF8` are two to four bytes, so
"BMW service record" was a broken BMP, a problem and exit 2, and two copies
of it a byte-identical *image*. `decode::plausible` now asks the bytes after
such a signature the first question that format's own decoder asks (BMP's DIB
header size, PNM's digit after whitespace and comments, PAM's field name,
ICO's entry count and planes, GIF's version), so nothing a decoder can read is
turned away, and `unmarked_format` no longer lets `guess_format` take those
bytes back;
a hidden folder met during a walk without `--hidden` (`walk::is_hidden`: any
name starting with a dot; one named as a root is scanned) — a home folder's
dot-folders held some 19,000 images on the author's machine (`.steam`, icon
themes under `.local/share`, `.themes`, `.vscode`), every copy of a theme's
icons a group the window offered for the Trash, and its scans are recursive;
a Trash or a thumbnail cache met during a walk (`walk::SetAside`: `.Trash`,
`.Trash-UID`, `.Trashes`, `$RECYCLE.BIN`, `RECYCLER`, `.thumbnails` by name,
and the XDG home Trash and thumbnail cache by place; one named as a root is
scanned) — the window trashes duplicates there, and a rescan found each of them
again, as the group's *reference*, because `.Trash-1000` sorts first and a
tie goes to the lowest index;
a symlink met during a walk without `--follow-symlinks` (a path *named* on the
command line is still followed), a followed link looping back into a folder
already being walked, a root `--exclude` covers, and a second name for a file
already listed — named twice, overlapping roots, a symlink, a hard link. The
walk keys files on (device, inode), not on the path: two names for one file
are byte-identical and would otherwise be an exact pair of one file. A real
name beats a symlink whatever order they arrive in, then the smaller path wins
(`walk::settle`); `vid-fp` let readdir order pick, and a link stood in for the
file it pointed at. `--exclude` compares canonical paths and, when links are
followed, asks where a path *leads* — `vid-fp`'s `2fba2f7` is the bug that
prevents, and `walk.rs`'s tests are ported from its.

Problems: an image that would not decode, a path the walk could not read (a
mistyped root arrives as one `ENOENT`, which is what stops a two-root run
silently scanning one of them; a followed link to nothing is one too, since it
is the shape of a link into an unmounted drive, and it stops `--prune-cache`),
an `--exclude` path that does not resolve and so excluded nothing, an image that described to **no
features at all**, a JPEG that is **cut off**, and a cache that could not be
read, written or created.

**A cut-off JPEG is analysed and called damaged** (`decode::jpeg_cut_off`).
`zune-jpeg` decodes leniently, as browsers do, so a JPEG cut off at a third of
its length was a picture two-thirds grey, analysed and matched as if whole —
once chosen as its group's representative over the file it was cut from —
while a cut-off PNG or WebP is refused (`Foto 38622.png`). It is still
analysed, since what is there may be worth matching, but it is a problem, its
rows say `DAMAGED` (`damaged: true` in the JSON, a `damaged` CSV column, a
line on the window's card), and it is never cached, so every run says so. The
test is structural, not the decoder's strict mode: segments by their lengths,
then image data to the first marker that is not stuffing, a restart or fill;
a file that ends before the end-of-image marker was cut off, and trailers after
it are not read. Strict mode was tried first and rejected: it called one of
IMGS's ffmpeg-written JPEGs, which libjpeg and PIL read, "Bad Huffman Code".
The structural test flags **0 of 28,640** JPEGs across the four corpora, the
found corpus and `~/Pictures`. Corruption inside data that does reach its end
is not looked for.

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

**None of `-o`, `--dump` and `--log-file` may name a picture**
(`outputs_are_not_pictures`, through `decode::is_a_picture`: an existing file
whose head the decoder would take). `img-fp photos -o photos/a.jpg` wrote the
report over `a.jpg`, exit 0, and named `a.jpg` as its group's representative;
`img-fp -o *.jpg` is the same thing by a glob, and the log was truncated
before anything had run. Asked before the log is opened, and by the window's
`check_args` too. A file that is not a picture — last run's report — is
written over as it always was.

**Two of `-o`, `--dump` and `--log-file` on one file are refused**
(`outputs_are_distinct`, through `report::same_destination`: one inode, or
one place a write would land), before the log is opened. They used to
overwrite each other with exit 0 — the report replaced the dump, and over the
log it left 1,321 NUL bytes and the log's tail.

**`--cache` with `--no-cache` is allowed only beside `--clear-cache`** (`validate`), to name the file to
delete: refusing it outright left no way to delete a named cache without using
it, and the window, dropping `--cache` when the cache was off, deleted the
default one instead.

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
`$XDG_CACHE_HOME/img-fp/analysis-IMGFPC11.bin` (the name carries the format;
see below), `~/.cache/img-fp/` without
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
- **Settings are a record's, not the file's** (`IMGFPC09`). They were in the
  header, and a run at any other `--work-size` replaced the whole file: one
  scan at 640 in the window cost the analysis of every folder the machine had
  scanned at 512, silently, and going back cost it again. Now a run reads the
  records made at its own settings and carries the rest unread, like a record
  for a file it did not walk (kept while the file exists). The price is that
  a `--work-size` sweep leaves a copy of the corpus's analysis per size in the
  file, which is one more reason a sweep should hand itself a `--cache` of its
  own. **`--prune-cache` drops those copies** as well as the records for files
  the scan did not find: it keeps only what the scan used. It used to keep
  them for the files the scan found, so one run at 640 doubled the cache for
  good — 30.5 MB to 64.4 MB on `derived/Desktop`, which a prune left at 64.4
  and now takes back to 30.5 — and only `--clear-cache`, which takes every
  record with it, could give the copy back. A cache written by a build with a different *format* is
  left as it is, and the run keeps nothing in it — the magic carries a
  version, and a file with the right prefix and another version is another
  img-fp's rather than damaged (see *Damage costs what it damaged*, below).
  **And the default file is named for its format** (`cache::file_name`,
  after 0.34.0). With one name for every format, that rule made an upgrade
  cache nothing at all: the new build met the old file, left it alone, and
  kept nothing on any run until `--clear-cache`. Now each format has a
  default of its own, so an upgrade starts a new file and two versions side
  by side never meet; only a `--cache` naming another version's file hits the
  rule. Old formats' files stay in the directory, by the user's decision
  (deleting those unused for a month was offered and declined).

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
when what else it holds — records superseded by a later one for the same path,
for files that have gone, at other settings for files that have gone — is **a
quarter of its records' bytes** (`Store::worth_compacting`), or when a
`--prune-cache` asks or a damaged record would otherwise be reported on every
run. Then the rewrite is a **copy** of the records worth keeping, byte for
byte and sorted, with nothing unpacked or deflated again. It used to be any
such record at all: one deleted photograph made the next run copy every other
folder's analysis to drop 9 KB of it (263 MB of IMGS's), and the window's
first scan after a trip to the Trash always paid it. The quarter is the
log-structured store's trade rather than a fitted figure: the file is never
more than a third larger than what it holds, and a rewrite copies at most three
bytes worth keeping per byte it drops. A dead record costs nothing meanwhile —
a later record for its path replaces it as the file is read, and one for a
file that has gone is passed over. So a sweep's cached runs write
nothing at all, and a cold run no longer has a save phase: the deflate that
used to run over the whole corpus after the analysis runs on each worker as it
finishes an image. Measured on `derived/Desktop`, interrupted twice and then
finished, the pairs and groups are identical to a run that was never
interrupted, and the resumed file is the same size to the byte.

**`image` is built with its decoders only** (`Cargo.toml`). Its default set
includes `avif`, which is the ravif/rav1e AV1 *encoder*; nothing here encodes,
AVIF is read by libheif, and fat LTO was already dropping it from the binary,
so the binary is the same size either way. What it cost was the build: two
cooled clean `--release` builds of each, alternated, 519 / 522 CPU-s and 141 /
144 s with it against 425 / 416 CPU-s and 132 / 128 s without, and 161 crates
compiled against 124. Every format IMGS holds, plus BMP, GIF, PNM, TGA, TIFF,
WebP and QOI, decodes to the same `--dump` bytes. **Nor `rayon`**, since the
audit after 0.30.0: all it did was let the `exr` crate build a thread pool of
its own, one thread per core, for every compressed EXR it decoded — `-t 1`
over eleven EXR files created 68 threads, 4 without it — and its blocks
decompress to the same pixels in order.

**libheif is loaded at run time, not linked** (`src/heif.rs`, through
`libloading`), since the same audit. Linked, `libheif.so.1` was in the
binary's `DT_NEEDED`, so the loader refused to start img-fp at all on a
machine without it, a folder of JPEGs included, and `libheif-rs` 3.0's floor
(1.17) was every user's. Now a missing library makes each HEIF file a problem
naming it ("HEIF and AVIF need libheif, which is not installed") and nothing
else changes; any libheif from 1.12 serves (`heif_init` is called when it
exists, which is how 1.14+ finds its decoder plugins). Checked in Docker: the
plain release binary runs on Debian 12 (libheif 1.15) and Ubuntu 22.04
(1.12), with and without libheif, decoding HEIC and AVIF where it is there;
and every HEIC and AVIF record of `derived/Desktop` is byte-identical to the
`libheif-rs` build's. The CLI's real glibc floor was never 2.39, which the
README said: it is 2.34, what Rust's std asks for, and `release.yml` now
fails if that moves. The window still wants GTK 4.10, which no distribution
with an older glibc ships.

**And it did move, through libm.** `f32::hypot` is glibc's `hypotf`, which
glibc 2.35 versioned anew, so the suggestion code's two calls made a binary
built on this machine (or the runner) ask for `hypotf@GLIBC_2.35`, and that
check refused 0.34.0 and 0.35.0: neither was published, on GitHub or on
crates.io. `suggest::hypot` takes the squares in f64 instead. Before using a
float function std hands to libm (`hypot`, `cbrt`, `exp2`, the
trigonometric ones), run `objdump -T target/plain/release/img-fp | grep -o
'GLIBC_[0-9.]*' | sort -Vu | tail -1` on a plain build: it must say 2.34.

**DDS is not in the default extensions.** `image` 0.25 has no DDS decoder
behind its `dds` feature ("The image format `DDS` is not supported"), so every
`.dds` a walk took was a problem and exit 2; it is a skip now, and the feature
is gone from `Cargo.toml` with it. Under `-x '*'` the same held for any format
`guess_format` names with no decoder built (DDS, PCX): `unmarked_format` now
asks `reading_enabled`, so such a file is not an image, a skip. `.hdr`
(Radiance) is decoded and is in the default extensions, and so, since 0.29,
are `.pam` (PNM's `P7`), `.apng` (a PNG, read for its first frame) and `.dib`
(a BMP): all three decoded under `-x '*'` and a default walk passed them over
as "not searched". The same audit added `.avifs` and `.heics` (whose first
picture libheif reads), `.jif`, `.jfi`, `.pjpeg`, `.pjp`, and Twitter's
`.jpg_large`, `.jpg_orig` and `.png_large`. **A file with no extension is
taken only by `-x '*'` (and the `!` forms), as in `vid-fp`.** 0.31.0 read the
first 64 bytes of every such file and let a list take it when they were a
format the list names, and that reached browser caches: Firefox keeps
`~/.cache/mozilla/.../cache2/entries` as extensionless files, 9,055 of them
pictures on this machine, and a recursive scan of a home folder — the
window's default — took every one. A list walk now neither takes nor opens a
file with no extension; IMGS's `beach` needs `-x '*'` again, which
`bench.py` always passed. **An ICO holding an RGB PNG is read as that
PNG** (`decode::ico_png`). The crate refuses one, as the format says it should,
and Pillow writes one whenever a picture with no alpha channel is saved as
`.ico` — `Image.open("logo.jpg").save("favicon.ico")` — under a directory entry
claiming 32 bits a pixel; browsers show them. The fallback runs only after the
crate has refused, on the entry the crate itself would have chosen, and only
when that entry is a PNG. ImageMagick's icons hold bitmaps and were never
affected.

**A decoder may allocate what the machine has available** (`decode::max_alloc`,
`MemAvailable` at start, at least 512 MiB), not the `image` crate's own 512 MiB,
which refused a 15000x12000 TIFF (540 MB of RGB) with "Memory limit exceeded"
while the same picture as JPEG or PNG decoded. The decode budget decides how
much decodes at once; this only turns a picture larger than memory into an
error rather than an abort. The probe and the window's preview use the same
limits. **JPEG XL is held to it too, since 0.29** (`decode::jxl_image`):
`jxl-oxide` sizes its frame buffers from the header and allocates them whole,
so a header claiming sixty thousand pixels a side was an allocation failure,
which ends a Rust process, rather than one file that would not decode. The
header is refused first against one float plane a channel, and the decoder's
own `AllocTracker` then counts every buffer against the same limit. HEIF is
refused from its header the same way; libheif's own limits are fixed and not
the machine's.

**A HEIF is recognised by any of its `ftyp` brands** (`heif_brands`),
not only the major one: a `mif2` file listing `mif1, heic` was "not an image"
here and read by libheif everywhere else.

**`--min-aligned-points` below 3 is run as 3, and the run says so.** Three is
the fewest a transform is fitted through, so 1 and 2 already behaved as 3, and
0 let a pair with no geometry at all through once overlap and correlation were
0 too. It is not refused; the header prints a note and the report's `config`
records the 3 that was applied.

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
| **cache load** (660 MB, inflate + mip pyramids, parallel; the pyramids are now built per comparison and never kept) | **2.0 s** |
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
(Every stream is deflated at level 1 since the CPU pass under *Speed and
memory*, which is smaller as well as five times faster to pack: the
descriptors' 0.758 above was level 6, and level 1 gives 0.752.)

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
  trip over. **Done since, for memory rather than for the file**: it was 4 of a
  keypoint's 20 bytes, 43 MB of a four-corpus run, and with the field gone
  from `Keypoint` itself a cached keypoint and a computed one differ in
  nothing. The extractor keeps the response beside its candidates for as
  long as it ranks them. The cache went to `IMGFPC06` with it.

**A record is kept under the file's canonical path, and its key includes
the change time** (`IMGFPC07`). Both were found by running, not by reading:

- Keyed on the path as the walk spelled it, one folder scanned as `photos`,
  `./photos` and its absolute path was analysed three times and held a record
  per spelling, and a relative record was carried over or dropped according to
  the directory the next run started in, since `carry_over` asks whether the
  path exists. The window names folders absolutely, so it never shared a
  record with `img-fp .`. `lib::cache_names` canonicalizes each walked file
  for the cache alone; the report still spells paths as the run was given them.
  **The walk states the canonical path itself** where it knows it, which is
  everywhere but under `--follow-symlinks`: a root is canonicalized anyway, and
  below it a walk that follows no links descends only real directories.
  `canonicalize` is a `readlink` per path component, and on a cached run over
  `derived/Desktop` those were 4,468 of 8,370 filesystem calls, all failing;
  now 16 of 2,629. And the report no longer opens each grouped file for its
  size (649 opens to 24): it takes the analysis's own `dims`, asking the
  header only of a file that would not decode. Reports on all four corpora are
  byte-identical either way.
- **And the walk's own `stat` is the only one** (`walk::Stat`). It already
  read every file's metadata for its identity; the exact pass then asked
  again for the size, the cache for the key, the header probe and the report
  for the size once more — 2,529 `statx` for 636 files on a cached run over
  `derived/Desktop`, three a file on one thread. They take the walk's size
  and key now, and the existence checks for other folders' records run on
  every thread. The key is therefore taken at the walk, which is still before
  the file is read.
- Keyed on size and mtime, a file rewritten with a different picture of the
  same size and its mtime put back (`cp -p`, `rsync -a`, `touch -r`,
  `exiftool -P`) kept its old record, and the cached run reported a pair at
  correlation 1.00 that `--no-cache` did not. ctime cannot be set back from
  userspace. The price is a re-analysis after a chmod, rename or new hard
  link, and of every file after a backup restore.

Concurrent runs are last-writer-wins: the loser's records are lost and nothing
is corrupted, because the temporary file a save renames into place carries the
process id. Two runs sharing one cache is not a case worth locking for — the
cost of losing is one re-analysis — but two runs sharing one *temporary file*
would be a damaged cache, which is a case worth a suffix.

**A cache another format version wrote is left as it is** (`Reject::
OtherVersion`). It used to be replaced by an empty file at once, silently: a
window from the release and a CLI from `cargo install`, one format apart,
sharing the default cache, emptied each other's on every run (30.6 MB to 145
KB, exit 0). Now the run reads nothing from it, appends nothing, compacts
nothing, and says so in a note (not a problem: nothing failed, and the other
version's records are intact). `--clear-cache` still deletes it, since that
is asked for by name.

**A record this run will not unpack is skipped, not copied** (`read_record`'s
`body`). Every record of every other folder, and of other work sizes, used to
be read into a buffer of its own and dropped. The body is now passed over
inside the reader's megabyte buffer: a scan of two files against IMGS-ALL's
1.4 GB cache, 1.35-1.61 s against 1.65-1.71 with the page cache cold and
0.40-0.51 against 0.70-0.75 warm. The bytes are still read. A page-sized
buffer reads a sixth of them, by seeking past the bodies, and is the fastest
warm (0.25 s) and twice the slowest cold (3.4 s): a chain of small dependent
reads that defeats readahead. Reading only the heads would need an index the
format does not have.

**Damage costs what it damaged.** A record whose body will not unpack, or
whose shape these settings cannot produce, is left out and its image analysed
again; the rest of the file is kept, and the next compaction drops it. A
length that runs past the end of the file is a torn tail and is cut there, and
damaged framing part-way through cuts the file at the last whole record and
says so. Both used to cost more: one bad record rejected the whole file, every
other folder's records with it, and a length damaged into a petabyte was
handed to the allocator, which aborted the process on every run until the
file was deleted by hand.

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
