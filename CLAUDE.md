# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from — read it before
changing how anything is scored.

**Status: the tool exists, beats every measured competitor, and has been
checked against a corpus it was never tuned on.** F1 0.980 at 100.0% precision
and 96.1% recall, against SSCD's 0.890 / 99.3% / 80.6%, with 102 of 121
transformations handled perfectly against SSCD's 34.

On 72 held-out transformations it has never seen, F1 0.963 at 99.6% precision
and 93.2% recall, and **every false positive there is a rearrangement trap** —
not one pair of different photographs was claimed. `benchmark/BASELINE.md`
holds the competition's numbers, `benchmark/VALIDATION.md` the held-out ones,
`README.md` the design.

What is left is not accuracy-critical: decode is 35% of the runtime and has no
downscaled JPEG path (the vendored `zune-jpeg` exposes none), the mirrored and
inverted query is asked only where the first pass came up short rather than
being folded into the vocabulary, and nothing has been tested above a few
thousand images.

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
    transforms.py     CATALOGUE (70, catalogue A) + CATALOGUE_B (54)
    transforms_c.py   CATALOGUE_C (72) -- the held-out validation transforms
    make_variants.py  the generator
  out/v4/             the baseline run: metrics.json, baseline.json, *.json
vendor/               third-party tools and venvs, gitignored
```

Corpora live outside the repo: `/home/daniel/Documents/IMGS` is the one
everything was tuned against, `/home/daniel/Documents/IMGS-VAL` is the held-out
one that nothing may ever be tuned against. Seeds for the second came from
`/home/daniel/Documents/temp-imgs`.

## The corpus

3,137 files from 46 seeds through 124 transforms.

- 38 seeds in the corpus root get **catalogue A** (70 transforms): re-encode,
  resize, crop, colour, rotate — the axes a perceptual hash is usually judged on.
- 8 seeds in `archive/` get **catalogue B** (54 transforms): containment,
  occlusion, non-affine warps, rearrangement, alpha — what A leaves untested.

Regenerate with `cd benchmark/corpus && ../../vendor/venv/bin/python
make_variants.py --clean` (~5.5 min, deterministic: same bytes, same names).
Add an image to the root to extend A, to `archive/` to extend B.

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

- **positives** — same seed, regions say SAME or CROP (100,642)
- **negatives** — different seeds, so they were never the same photograph
  (4,813,486); plus same-seed pairs with disjoint regions or a different
  scramble (910)
- **excluded** — PARTIAL overlaps and `exif_rot90` (4,688)

So there is **one F1**, not one per corpus half.

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

On the tuning corpus, 3,888 pairs: very small crops (`crop_center_25`,
`crop_tiny_detail`, `quadrant_tl`), heavy downscales (`scale_12`, `thumb_128`),
and partial colour inversions (`solarize`). 15 false positives remain, 14 of
them the `tile_shuffle` trap, where a crop legitimately lies inside one
shuffled tile. The fifteenth — a 25% centre crop of one seed against a square
crop of another — is the only real error in 96,769 claims.

On the held-out corpus the same kinds fail harder: `crop_micro_15` (a 15%
window) at 8/16, `fax_bilevel` at 12/16, `tiled_watermark` at 11/16, the mirror
variants at 14-15/16. All 142 false positives there are rearrangement traps;
none is a pair of different photographs.

A note on the traps, since they dominate both FP counts and will keep doing so:
`column_roll_37` slides an image sideways and wraps, leaving 63% of it a rigid
translation of the original, so a crop landing inside that 63% genuinely *is*
present in both files. The corpus calls such pairs DIFFERENT on the scramble
rule and img-fp is caught by it for a defensible reason. Report trap hits and
real errors separately, the way `BASELINE.md` does for SSCD.

### Parameters, and the rule about them

**Tune on `/home/daniel/Documents/IMGS`. Report on
`/home/daniel/Documents/IMGS-VAL`.** Never choose a value by looking at the
validation column: the moment a decision is made against it, it stops being a
held-out corpus and becomes a second tuning corpus, and there is no third.
`benchmark/VALIDATION.md` has the full argument and the numbers.

A number earns its place by being derived from something, or by measurably
costing performance on *both* corpora when removed. One that barely moves the
tuning corpus and improves the held-out one was never doing its job. Applying
that rule took the CLI from 13 result-changing options to 8, the acceptance
policy from 9 fitted numbers to 2, and the bridge test from 3 to 0, at equal
tuning-corpus F1 and better held-out F1.

Two things that did *not* survive deletion, and why they stay:

- **Three acceptance tiers.** Folding corroboration in with propagation looks
  right and is wrong: a corroborated pair has features vouching for it, a
  propagated one does not. Two tiers cost 4.2 points of held-out recall.
- **The cluster margin** (`CLUSTER_SLACK_*`). Without it, corroborated pairs
  face the anchor bar and held-out recall falls from 93.2% to 90.7%.

Things that generalise badly and were fixed rather than tuned: the vocabulary
was a fixed 65,536 words at any corpus size, which made img-fp nearly useless
on a small folder (one pair in twenty-eight, on eight images). Depth now
follows the descriptor count.

### Tuning discipline

`--dump` writes every verdict considered, accepted or not, as CSV. Fit
thresholds against that offline instead of re-running the tool per guess. Use
`--cache` while tuning the matching stages: a cold run is ~86 s, a cached one
~30 s, and the cache is keyed on the extraction settings so changing
`--work-size` or `--features` invalidates it correctly.

Note that timings taken this way are warm-cache and run about 40% faster than
`bench.py`'s cold-cache figures. Compare tuning runs with each other, never
with `BASELINE.md`.

Measured trade-offs, so they need not be rediscovered: `--work-size` 448 gives
F1 0.975 at 80 s, 640 gives 0.980 at 86 s, 768 gives 0.976 at 147 s.
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
