# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from: read it before
changing how anything is scored.

The full measurement history (every table, every pass, every rejected idea
with its numbers) was in this file until 0.44.0. It is in git:
`git show 5de81bc:CLAUDE.md`. Look there before re-trying anything listed
under *Tried and rejected* below, or before quoting a figure not given here.

## Status (0.50.0)

- Baseline corpus **IMGS-ALL** (IMGS + IMGS2 + IMGS3 + IMGS4: 27,659 files,
  304 seeds, 90 transforms). At the default `--work-size 512`: **F1 0.9821**,
  99.57% precision, 96.90% recall, 47 perfect transforms; 7 cross-family
  pairs, no family merge (`out/v33-fit-pyramid/RESULTS.md`; 0.45.0 was
  0.9805 / 95 cross-family). 640 and the per-corpus figures below are
  0.44.0's, not re-measured. At 640: F1 0.9826. SSCD, the best competitor:
  F1 0.748, and every other tool has zero perfect transforms
  (`benchmark/BASELINE.md`, field in `out/v17-all4`).
- 0.47.0 (audit fixes, `out/v35-audit`): F1 **0.9823**, recall 96.93%,
  same 7 cross-family pairs and trap FPs, 48 perfect transforms; keep rule
  17,299 DELETE, none loses content, 276 / 304.
- 0.48.0 (`out/` not written; measured in a session scratch dir): keep rule
  **297 / 304** originals (was 276) from `EXTRA_PER_OCTAVE` 0.1 -> 0.2,
  17,300 DELETE, none loses content, 304 / 304 faithful; pairs and groups
  byte-identical. Also: signals started ignored stay ignored (`nohup`),
  "changed since the scan" counts from its start, the window demotes
  suggestions resting on a changed kept copy as `--last` does, and a JPEG's
  measured colour is kept in its cache record.
- 0.48.1: a same-size copy softer by more than
  `SOFT_GAP` (0.15) may not delete a file (`suggest::Ctx::softer`); a file
  held back only by that is KEEP. Before, an original among several soft
  same-size copies was outvoted and deleted (177 / 178 synthetic families,
  now 6). IMGS-ALL: 17,259 DELETE, none lost, 297 / 304, REVIEW unchanged.
- 0.49.0 (audit fixes, no change to pairs or suggestions): a header
  claiming more pixels than memory is a per-file problem, not an abort of
  the run (`fits_in_memory`), and decoder panics are caught; reports and the
  dump are written whole (`report::write_whole`); `lost+found` set aside; the
  window names a kept copy's folder and zooms its images (`[-][+]`).
- 0.50.0: the keep rule on two-file families (`benchmark/pairs.py`:
  every seed's original + one variant, each transform alone). 0.49.0 lost
  content in 2,241 of 27,355 pairs (an original deleted for its brightened,
  recoloured, sharpened, watermarked copy, or for a composite holding it);
  now 24, all where `extra` takes a reduction past `SHRINK` for an
  enlargement, as before. Two rules: a frame of two different pictures
  that nothing measured separates is undecided, both REVIEW; and a lone
  larger frame whose extra area reads as canvas (`on_canvas`) does not delete
  what it holds. REVIEW in pairs 8,114 -> 29,368 of 54,710 files (every lone
  re-encode and format conversion). IMGS-ALL: 16,873 DELETE, none lost,
  297 / 304, 304 / 304, REVIEW 3,004. Still open: which of several lossy
  copies is the original (IMGS-PROVENANCE keeps it in 34 / 266, a faithful
  copy in all; size picks a q97 re-save instead, so it is no answer), and
  colour edits in families of three or more.
- Per-corpus at 512: IMGS 0.9711, IMGS2 0.9808, IMGS3 0.9861, IMGS4 0.9854
  (`out/v31-cost-cliff`).
- Cost: ~2,500 CPU-s / ~6 min cold on IMGS-ALL; SSCD ~55,000 CPU-s.
- Released binary is the **plain x86-64 build** (`target/plain`), which
  `bench.py` measures by default. It differs from a native build only on
  JPEG XL (jxl-grid's FMA rounding). The repo no longer ships a
  `.cargo/config.toml` with `target-cpu=native`, so a plain `cargo build` is
  the release build too (a native build on an AVX-512 host SIGILLed on AVX2
  machines). `check_cpu` reads `cpuid` itself (`lib::cpu`):
  `is_x86_feature_detected!` is constant `true` for features the build
  assumes, so the old check could never fire; it refuses with raw `write` +
  `_exit`, since formatting an error ran AVX-512 code.
- Grey is BT.601 luma; a JPEG's grey is its Y plane.
- Shipped `--min-pixel-correlation` is 0.6, chosen for what users want
  grouped (0.5 grouped merely similar photos), not for F1.
- Default `--work-size` 512, one step below the knee at 640. 640 buys ~0.5
  points of IMGS-ALL recall (mostly containment: `embed_tiny`,
  `contact_sheet`) for ~22% more CPU on photographs. `--work-size` is the
  only knob tied to the clock; nothing is analysed above it.
- Precision: on IMGS every false pair is a rearrangement trap. On the
  near-miss corpora stray single files join a sibling photograph's family
  (the clean-anchor rule's lone-file allowance). **Watch merges, not pairs.**
- Open: no downscaled JPEG decode path (worth ~4%); the second look is
  re-asked only where the first pass anchored <2 times; nothing tested above
  ~30k images; on the found corpus about half of sampled reported pairs are
  different specimens of one species, leaking through the propagated tier;
  5 of the 7 families not keeping their original are grey or near-grey
  originals beaten by a tinted copy (`temperature`) on "colour over grey".

Docs split: `benchmark/BASELINE.md` (competition), `benchmark/VALIDATION.md`
(held-out experiment), `README.md` is **users only**: install and usage, no
benchmark figures, comparisons or local measurements; same for `--help` and
the man page generated from `Args`. Hidden flags (`hide = true`, re-added in
`print_man` since `clap_mangen` drops hidden args): `--dump`, `-k`,
`--no-last`, `--prune-cache`.

## Layout

```
src/
  lib.rs        pipeline, propagation, bridge pruning; cli_main, worker_main
  main.rs       img-fp: calls cli_main
  gui/          img-fp-gui (feature `gui`, GTK 4.10+)
  decode.rs     format sniffing, decode to one grey plane, decode budget
  heif.rs       libheif loaded at run time (libloading), not linked
  sift.rs       local features
  index.rs      vocabulary tree, inverted file, containment scoring
  verify.rs     correspondence, geometry, pixel check, acceptance Policy
  group.rs      pairs -> groups around a representative
  walk.rs       roots -> files (lists, -x, --exclude, symlinks, dedupe by inode)
  cache.rs      on-disk analysis cache
  report.rs     text / CSV / JSON reports
  suggest.rs    KEEP / DELETE / REVIEW per file
  against.rs    --against
  last.rs       --last (last scan kept beside the cache)
  problems.rs   skips, problems, exit codes, --log-file
  progress.rs   progress bar
  simd.rs       x86-64-v3 vs portable kernels (`dispatched!`, `v3()`)
  spill.rs      analysis arrays in an unlinked scratch file, read via mmap
build.rs        sets cfg(dispatch) for x86-64 builds without AVX2
benchmark/
  bench.py      measurement harness (cold, sequential, instrumented)
  score.py      F1 vs generated ground truth
  analyse.py    + traps vs cross-family, merges, two seed halves
  suggest_score.py / against_score.py   score `action` / --against reports
  pairs.py      the keep rule on two-file families (each transform alone)
  replay.py     replay anchor tier + bridge test off one --dump
  runners/      one wrapper per competitor -> canonical JSON
  corpus/       transforms.py (CATALOGUE), make_variants.py, make_combined.py,
                make_against.py
  out/vNN/      measurement sessions; v17-all4 is the field's baseline
vendor/         third-party tools and venvs (gitignored)
```

## Corpora

- **IMGS-ALL** `/home/daniel/Documents/IMGS-ALL`: hard links to the four
  corpora, ground truths merged, built by `corpus/make_combined.py`. Default
  for `bench.py`, `score.py`, `analyse.py`. Re-run with `--clean` after
  `make_variants.py` (links go stale).
- **IMGS** `/home/daniel/Documents/IMGS`: 62 seeds. Seed `beach` has no
  extension, so comparable runs need `-x '*'` (`bench.py` passes it).
- **IMGS2** (design), **IMGS3**, **IMGS4**: photographs chosen in near-miss
  sets (same scene a moment / step apart), so each set is a family of hard
  negatives. IMGS3 and IMGS4 were held out and are now **spent**; the next
  rule to defend needs fresh seeds.
- **IMGS-ELONG**: 33 elongated pages of 2-10 photos (68 positives), for the
  area rule.
- **IMGS-AGAINST**: IMGS-ALL split into scan + library as symlinks; run with
  `--follow-symlinks -r -x '*'`.
- **Found corpus** `/home/daniel/Downloads/archive` (exactly that path:
  `~/Downloads` adds a stray file that changes pair counts). 9,285 224x224
  photos, unlabelled: **cost and byte-identity only, never accuracy.**

Regenerate: `cd benchmark/corpus && ../../vendor/venv/bin/python
make_variants.py --clean` (deterministic). Every transform amount is drawn
per seed from a hash of the seed's thumbnail (`seed_key_of`); transform and
its recorded region/coverage come from one factory (`_crop_jit`,
`_rotate_jit`, `_scales`) so the ground truth cannot drift from the pixels.
Known failure: `strip_metadata` on AVIF is deleted by the generator.

## Ground truth: generated, never labelled

The most important decision in the project. Hand-labelling found photos
measured the labeller's bias and was deleted. Generated truth: positives are
same-seed pairs whose regions say SAME or CROP; negatives are different seeds,
or same seed with disjoint regions or different scramble; PARTIAL is excluded.
One F1.

`pair_relation`: (1) different scramble keys -> DIFFERENT, always;
(2) region IoU >= 0.95 SAME, one contains >= 95% of the other CROP, disjoint
DIFFERENT, else PARTIAL; (3) coverage < 90% of the other's downgrades SAME to
CROP.

## How it works: the load-bearing pieces

Each was measured; changing one needs a re-measure.

- **Local features, not a global descriptor.** That is why containment
  works. Check `collage_2x2`, `embed_quarter` after any global shortcut.
- **Retrieval scores containment** (normalised by the query's idf mass), not
  cosine (-5 recall).
- **A claim is a transform plus pixel agreement**, never a score over a
  threshold (rejects `tile_shuffle`).
- **Pixel check**: flat blocks abstain; flatness is relative to the
  thumbnail's own contrast (`Thumb::build` stretches each thumbnail); blocks
  are averaged (|r|), not counted; both sides are read through a mip pyramid
  (`Thumb::lod`) at the right footprint. **Do not reintroduce a plain tap.**
  A level is the one below filtered [1 3 3 1]/8 each way and halved (0.46),
  not a 2x2 mean: a box's levels depend on where its grid falls, so a crop or
  a reflection of an odd-sided thumbnail compared out of phase (exact copies
  under the clean bar through the *exact* transform: 12/288 -> 2/288).
  Levels are kept for a run of comparisons on one thumbnail (`Thumb::id`).
- **The fitted transform is the least-squares fit**, kept even when it moves
  an inlier out of tolerance (0.46). It used to be dropped then, which was
  nearly always, leaving the one-keypoint similarity hypothesis: exact copies
  were 1-9 working px out at the corners and 110/288 fell under the clean bar.
  Probe: 48 seeds x 6 exact transforms, fitted vs exact transform's
  `blk_min` (`out/v33-fit-pyramid`).
- **`encloses_centre`**: an anchor's inliers must bracket the middle of what
  it claims (over the keypoints in the claimed region, either frame).
  Without it: 36 merges.
- **Propagation** composes transforms and re-checks pixels; worth ~7 points
  of F1 for ~6% of the clock, unconditional. BFS tree from the best-connected
  member. Later rounds only in components that gained a pair. Work per round
  is a budget (`prop_budget` = direct candidates verified); a component that
  does not fit is **starred** (each file vs. the root). Spread by row.
- **Three acceptance tiers** (`verify::Policy`): `anchor`, `propagated`
  (no inlier count, no enclosure), `corroborated` (anchor minus
  `CLUSTER_SLACK_*`). Collapsing them costs recall or precision.
- **Whole-frame matches** (`Rules::framed`, `verify::frame_onto_frame`):
  a verdict whose transform puts each of A's corners on B's same corner
  (inlier tolerance; resize only: a half-turn let a `quadrant_rotate` and a
  `tile_shuffle` of one dark seed in on 3 points and delete the trap) clears
  anchor/corroborated with >= 3 aligned
  points if `blk_min >= CLEAN_WORST_BLOCK`; never clean, so it adds a file
  and joins no clusters. For icons/logos: a 512 px icon vs its 128 px copy
  had 7 points at 0.997. Featureless files (no keypoints) still match only
  identical / same-pixel copies.
- **Weak bridges are dropped** (`drop_weak_bridges`): any bridge whose far
  side is more than one file, including mirrored/inverted anchors. Since
  0.44.0 a dropped bridge is kept as a weak anchor (may add one file, counts
  in the cover test). Without the bridge test: 4 merges.
- **Clean-anchor clusters** (`admit_anchors`): an anchor is clean when no
  detailed block correlates below 0.9 (`blk_min`, `Policy::clean`, the
  rule's one fitted number; 0.85 until the fit and pyramid fixes raised near
  misses ~0.04 and Segovia1/2 merged at 0.85. Swept 0.875-0.95: no merge,
  F1 0.9823/0.9821/0.9816/0.9806; 0.9 is the plateau's lower middle). Weak anchors may join two clusters only when
  every file of the smaller has a weak anchor into the larger, and **not**
  when both close a cycle of clean anchors unless one file of the larger has
  a weak anchor to every file of the smaller. This is what stops near-miss
  families merging (IMGS2/3 at 87-92% precision without it).
- **Groups are a representative plus everything that matched it**
  (`group.rs`), not components (fragile, invent false pairs) and not cliques
  (thousands of groups). Groups overlap; members were not compared with each
  other. The representative is not the file to keep.
- **Vocabulary sized from the corpus** (`VocabParams::for_corpus`): occupancy
  held at ~3 descriptors/word by depth *and* narrowed branching (16^depth
  steps alone swung occupancy 16x and merged families at ordinary sizes).
  A word is a live leaf's centre slot. Centres are bytes (the one lossy step,
  measured neutral). Sample = a tenth of distinct descriptors, capped 1.28 M
  (`DESC_PER_SAMPLE`). Do not fix the vocabulary size.
- **Inverted file indexes every word**; no frequency cap.
- **"Identical" is a byte comparison**, after a 4x16 KB sample hash; under a
  wildcard only possible pictures are read (`decode::may_be_image`).
- **Byte-identical copies and same-analysis twins** (`lib::Planes`,
  `lib::same_analysis`) are matched through one original and get its pairs
  via `with_copies`; otherwise they count each other as matches and suppress
  the second look. Twins are reported as `same_pixels`. A twin must also
  match in colour (`lib::same_colour`: measured colour equal to the bit, or
  two JPEGs differing only in APPn/COM, else `decode::chroma_of` grids within
  `COLOURFUL`): grey is luma only, so a lossless-greyscale JPEG was a twin of
  its colour original, stood as one with it, and won as the smaller file.
- **Area rule** (`decode::area_factor`, 0.44.0): a picture wider than 4:3
  gets the pixels a 4:3 one gets (factor `sqrt(3/4 * long/short)`, thumbnail
  too, capped by `THUMB_MAX_SIDES`). 4:3 or squarer is untouched.
- **Enlargement** (`upsample_below`, `lib::enlarge_below`): small pictures
  doubled up to `min(work, 512)`, never past the working size. 512 is the low
  edge of a plateau; no cap on the factor.
- **EXIF orientation** is applied to the working plane, not the decoded
  picture (`decode::orient`).

### Keep, delete, review (`suggest.rs`)

User requirement: **a deletion never loses picture content.** Keeps may be
the original, a lossless copy, a re-encode, or a composite holding the whole
photo. REVIEW is for weak matches close to a copy; nothing acts on it.
On IMGS-ALL at 512: 16,873 DELETE, none loses content, 297/304 families
keep the original or a lossless copy, 304/304 a faithful full copy (0.49.0:
17,259; 0.48.0: 17,300; 0.47.0:
17,299, 0, 276, 304; 0.45.0: 17,986, 1 lost, 272, 299). Scored by `suggest_score.py`
(`benchmark/suggest_score.py REPORT.json`, ~100 s a cached run), and on
two-file families by `pairs.py` (`run BINARY OUT --cache CACHE`, a cache
an IMGS-ALL run made, ~11 min; `score`, `compare`): **IMGS-ALL cannot see
the pair regime**, where every majority the rule chooses by is missing.
0.49.0: 2,241 of 27,355 pairs lost content; now 24.

Rules (constants in `suggest.rs` with reasons): clean pair at 0.85 (its own
bar now, not `Policy::clean`'s; 0.85/0.9/0.95 lose the same, delete ~the
same); a mirrored pair, like a negative, deletes nothing either way (a flip at
q95 was kept over its q92 original 14/19 times); weak pair
deletes at >= 0.95 (`WEAK_DELETES`), REVIEW at >= 0.9 (`REVIEW`); two crops
each missing <= 10% count as one frame on deleting pairs (`CROPS`); *holds*
by frame corners, keeper may shrink <= 0.5 octave (`SHRINK`) unless the extra
pixels hold no detail (`Traits::detail`, `extra` above 0.2 an octave + 0.03,
`EXTRA_PER_OCTAVE`: enlargements read <= 0.3/octave at p99, real originals
>= 0.44 at p01; at 0.1 a 3x Lanczos upscale beat its original; originals
kept at 0.1/0.15/0.2/0.25/0.3/0.4: 276/296/297/297/298/298, none lost;
above 0.25 real originals start to read as enlargements); a whole-frame blurred check before
every deletion (`whole_worst`, 12 grey levels on a 64-sample grid,
`WHOLE_GRID`: at 32 thin lines blurred to haze and, once transforms fitted,
composites of one photograph deleted each other; losses at 32/48/64/96:
3/1/0/0), every holder asked in turn
(a cap of three tries, removed: it cost 108 safe deletions and no time);
composites are deleted only by their own copies; where no picture of a set
is copied at its own frame, a larger frame deletes what it holds only if its
8-px detail reads nearer "the rest is more picture" than "the rest is flat"
(`on_canvas`, split at sqrt of the share: at 1 px a third of slight crops
read as canvas; composites deleting their original in pairs 971 -> 40, losing
content 133 -> 2); a class of two different pictures of one frame (twins
count once) that neither `more` nor a size gap past `SHRINK` separates
deletes nothing and is REVIEW, both files, however they agree (not against a
library: a pinned file decides); no file is deleted by one
of its size softer by more than `SOFT_GAP` = 0.15 (the p95 softening of a
faithful lossy re-encode; the keeper is chosen by majority, and nine soft
enlargements outvoted their original; a file held back only by this is
KEEP, the test asked last so it holds back nothing else);
tie-break by grey closeness
(`TAU` = 3, the p99 grey-grid distance between two files holding the same
pixels), sharpness (`SOFT_TIE` = 0.05, the p99 softening of a q84-96 JPEG
re-encode), lossless, EXIF, size, colour (`COLOURFUL` = 1.5, above the 1.31
the noisiest grey copy reads; `decode::colour_of`, lazily, and kept in the
JPEG's cache record, `cache::Store::keep_colour`: it was decoded again on
every rescan, 4.5x a cached rescan's CPU on grey JPEGs). Each derivation
and its sweep is on the constant. Colour is the colour of a 4x4 grid's cell
means (`decode::CellColour`): pixel by pixel, noise on a grey photo read as
colour and no bar separated it from a faint tint. A palette (indexed PNG,
<= 8-bit BMP, colour-mapped TGA) is not lossless (`decode::indexed`): a
256-colour copy used to be kept over its PNG original. Nor is a
JPEG-compressed TIFF; a WebP is lossless when its picture chunk is `VP8L`
wherever it lies (EXIF or ICC make the first chunk `VP8X`), a JPEG XL when
its first frame is Modular without XYB (a recompressed JPEG is VarDCT).

A DELETE row carries `kept_copy` (JSON only), which the window shows: the
end of its chain of holders (`suggest::kept_at_end`), which stays. Deletion
spreads one holder at a time, and the nearest holder was itself a DELETE for
15% of deletions. A deletion whose kept copy is gone (`--last`, or the
window after the Trash or on restore) becomes REVIEW: carried out, it would
take the last copy. The window labels it "review: its kept copy is gone", not
a weak match. The two group rules judge only by representatives still there
(`--last` and the window): a group whose representative has gone suggests
nothing for its files.

`--suggest correlation | representative` (`suggest::by_group`): simple
group-only rules, by user request; `correlation` deletes at 0.9 at the
default bar (`CUT` = 0.75 of the way from the bar to 1), the user's choice. Neither answers to the content requirement.

### `--against LIBRARY` (`against.rs`)

Verdicts `repeat` / `better` / `review` / `new` per scanned file; `--list`
prints paths. It is the content rule with the library pinned
(`suggest::suggest_pinned`). One index over both sides; the scan queries,
then library files it reached query in turn until none is new; candidates
ranked by `both_ways`. The second look also re-asks the best-connected member
of each clean-anchor component holding a scanned file. A scanned original
whose library copy is a same-frame re-encode is `repeat` (user's decision).
Copy election: the first member of an exact group is the original, so cached
and cold runs agree.

### `--last` (`last.rs`)

Every finished scan is kept in `last-scan/` beside the cache file (shared
with the window); `--last` re-writes text/CSV byte-for-byte, JSON without
`pairs`. Not kept under `--no-cache`, `--no-last`, `--against`, or a walk
that found no picture; the window keeps by the same rule. Paths are kept absolute (`keep_scan`): read back from another folder,
or by the window from `$HOME`, relative ones named other files. The window's
Trash leaves a file whose length differs from the scan's, or whose ctime is
from the second the scan *started* on (`Notes::changed_after`; it was the
end, so an edit saved mid-scan passed); `--last` and the window
(`State::note_changed`, on showing, *Mark suggested deletions* and the
Trash) suggest deleting no such file and nothing resting on one. The Trash dialog also warns when a
marked DELETE's kept copy is marked (groups overlap, so "every image of a
group marked" missed an original + re-encode marked with a crop left).

### `--skip-mirrored`

Skips the second look: ~60% off a cached found-corpus rescan, but -6.6 to
-9.6 points of recall on the benchmark corpora and family fragments that join
near-miss siblings. A real trade, which is why it exists (and
`--no-propagate`, a dominated one, does not).

## Parameters, and the rule about them

**A number earns its place by being derived, not by scoring best.** Removing
a parameter is the cheap experiment; run it before adding one.

- **Sweep for the cliff, not the peak.** Acceptance thresholds are monotone
  in F1 up to a family merge. Quote the distance to the nearest merge (†: a
  seed pair with >= 300 false pairs), and always split false pairs into
  rearrangement traps vs. cross-family errors.
- **Score on two disjoint seed halves** (`analyse.py`) to check a shape
  generalises. Halves are blind to near-miss merges.
- **Re-test survivors after a removal, stacked**: `max_scale` is inert alone
  and load-bearing without the ratio test.
- **Names are load-bearing**: `--min-frame-overlap` (geometry only),
  `--min-pixel-correlation` (pixels), `--min-aligned-points` (inliers).
  Overlap and correlation each catch a failure the other cannot see; neither
  is a cost knob (tightening them makes propagation do *more* work).
- `--min-aligned-points` is the only bar propagation can overrule; below 3 is
  run as 3, with a note.
- Self-deriving the aligned-points bar from a null model was built and is
  worse (coincidence was never the binding constraint).

Cliffs at 0.44.0, distance from shipped to nearest merge
(`out/v31-cost-cliff/cliffs.txt`). **Not re-measured since 0.46's fit and
pyramid changes, which move every pixel figure; only the clean bar was.**

| option | 512 (default) | 640 |
|---|---|---|
| `--min-aligned-points` 10 | none down to 3 | 3 steps (`bust` at 7) |
| `--min-frame-overlap` 0.85 | 0.35 (`docks`, `Henares` at 0.3) | 0.25 (`docks` at 0.6) |
| `--min-pixel-correlation` 0.6 | none 0.2-0.8 | 0.2 below (`game` at 0.4) |
| `-k` 150 | none from 10 up | none |

`-k` below 25 measures the propagation budget (clusters get starred), not the
candidate list. Every remaining merge is two photographs of one scene.

Internal constants, measured (`out/v24-constants`, `out/v25-slack-budget`):
thumbnail 128 is the low edge of a plateau; **`GRID / BLOCK` (48/8 = 6 blocks
a side) is one step from a merge cliff** (4-5 blocks merge); flat-block bar 4
sits on a 1-6 plateau (8 merges); `CLUSTER_SLACK_*` are points on a ramp, no
cliff, kept for the 0.6 default's sake; decode budget `MemAvailable / 8` is
the knee; inlier tolerance 0.015 of the diagonal is on a plateau (0.03 is
unsafe); `index::shared`'s texture cut removed, its 60,000 cap kept as a work
bound. Lowe ratio test, `PROP_NCC`, `PROP_GAP`, per-block cut and coverage
fraction were removed as inert or redundant.

**How to measure without a run per value**: `--dump` writes every verdict
(gate loosened to 3 points / 0.2) with `blk`/`blk_min`, so any bar >= 3 and
the cluster rules can be replayed offline (`replay.py`,
`out/v14-fullsweep/replay_*.py`). To sweep an internal constant, read it
from an env var temporarily, hard-code the result, delete the scaffolding,
and re-measure the final build.

## Speed and memory

Shares (at 640, IMGS): decode ~35%, extraction ~45%, rest ~20%; at 384
decode ~48%. On the found corpus the second look is ~41% (nearly every file
has no duplicate) and enlargement 224 -> 448 is the biggest knob.

Rules that decide what is worth trying on this machine (Ryzen 7 3700U, 4
cores / 8 threads, power-capped):
- At 8 threads it charges for **µops, not waiting**: removing a stall shows
  at `-t 1` and vanishes at `-t 8`; removing work or bytes moved shows at
  both. `prof` stage tables are thread-time; only an end-to-end A/B is a
  pass's worth.
- `-t 4` costs ~35% fewer CPU-s for ~15% more wall.
- Every optimisation pass is **byte-identical** (same pairs, groups,
  fields) on the **whole** corpus, not a subset; check with `-o a.json` /
  `-o b.json`. The exceptions (byte centres, luma, streamed resample, area
  rule) were measured for accuracy instead.
- Peak memory depends on `MemAvailable` (decode budget); use `-t 1` for a
  deterministic reading. **The analysis is not in the heap** (`spill.rs`):
  each record's descriptors, keypoints and thumbnail pixels are written as
  made (cold, and on cache unpack) to an unlinked file beside the cache and
  read back through a read-only `MAP_SHARED` mapping; the same bytes, so
  output is byte-identical. Their pages are the kernel's to drop and re-read.
  So the real figure is **RssAnon + swap**, not PSS, which counts the mapped
  file pages. IMGS (5,641 files): heap held after analysis 426 -> 56 MB, at
  the end of verification 480 -> 118 MB; a cached run under a 380 MB cgroup
  cap with no swap finished (old build OOM-killed), at 300 MB it thrashed.
  What is left peaks inside decodes (a per-thread transient) and, cached, at
  assembly (the pair records, which scale with pairs). Writes go through
  `pwrite`, never the mapping (ENOSPC is an error, not a SIGBUS); any failure
  leaves that record in the heap. IMGS-ALL (`out/v34-spill`): heap peak
  2,330 -> ~1,040 MB, PSS unchanged at ~2,340, output identical, CPU within
  the base runs' own spread; then 776 MB once assembly stopped holding a
  second copy of the pairs, dump-only verdicts and a path string per pair
  end (`OutPair` names are shared `Arc<str>`). The peak is still assembly
  (graph, suggestion, `OutPair`s and the stable sort's buffer); matching
  ~390 MB, analysis transients ~300 MB.

Portability: `cargo install` builds plain x86-64; `simd.rs` dispatches the
hand kernels and `dispatched!` gives hot functions a v3 copy. **Anything that
keeps code out of line** (closures to `LocalKey::with`, `Option::map`,
generic `extend`) silently runs the plain copy; check with `objdump`, counting
`ymm` per symbol. Test both copies agree:
`RUSTFLAGS="-C target-cpu=x86-64" cargo test --release --lib`.

glibc floor is **2.34**; `release.yml` fails if it moves. f32 libm functions
(`hypotf` is 2.35) can raise it: check
`objdump -T target/plain/release/img-fp | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1`.

Decoder hazards:
- **Any decoder running work on the global rayon pool can deadlock the
  decode budget** (a thread steals an outer task and claims twice). JXL gets
  `JxlThreadPool::none()`, and a thread already holding a claim never queues.
- `image` is built with decoders only, no `rayon` (exr spawned a pool per
  file). libheif is dlopened; missing libheif makes HEIF files a problem.
- `max_alloc` = `MemAvailable` (min 512 MiB), applied to JXL headers and
  HEIF headers too. PNGs (8-bit) and JXL renders stream a row at a time.
  `into_decoder` + `from_decoder` skip the crate's own `limits.reserve`, so
  `decode::fits_in_memory` asks first: without it an 18 KB BMP claiming
  96 x 788M px allocated 227 GB and aborted the whole run (3 of 3,000
  fuzzed files). Decoder panics are a per-file problem (`decode_guarded`,
  `probe`), never the end of the run or of a window thumbnail worker.

Measurement tools:
- `--features prof`: per-stage table, timed with `rdtsc` (this machine's
  clocksource is HPET, where clock reads cost ~1.5 µs). Stages nest.
- `cargo test --release -- --ignored --nocapture <name>`: `kernel_timings`,
  `reduce_timings`, `quantise_*` (incl. `quantise_threads`), `shared_timings`,
  `shared_pool`, `shared_overlap`, `match_timings`, `pixel_timings`,
  `extract_threads` (checksums `b8e5341a1d7b5cad` / `5c2fd85ed71708a8`; run it
  on any `sift.rs` change), `octave_threads`, `blur_threads`. Bench the
  regime (threads, cache-busting sizes), not just the kernel.
- `objdump -d` is the only instruction-level evidence (`perf` and `ptrace`
  are blocked here).

### Tried and rejected (numbers in `git show 5de81bc:CLAUDE.md`)

Do not retry without new evidence:
- Second look: narrowing its descent, dropping any variant, cutting its
  candidate list, inlier or correspondence ceilings, dropping unseen words,
  relabelling words instead of descending (-21% of its pairs), an
  equivariant vocabulary (symmetrised pool merges families).
- Descent: partial-distance pruning, batching level by level, measuring
  several parents together, float centres in dimension-major blocks.
- Extractor: branchless extremum test, whole-row or strip blur passes,
  wider horizontal blur blocks, folding taps, row-by-row gradient fill, lazy
  gradients, descriptor histogram tricks (paired corners, register-held
  bins, two alternating histograms, vectorised trilinear corners).
- Memory/alloc: per-thread plane pool, `malloc_trim` at phase boundary,
  pinned mmap threshold, glibc tunables, holding the vocabulary sample as
  references.
- Other: DCT-scaled JPEG decode (~4% ceiling, second decoder),
  `posix_fadvise(WILLNEED)` read-ahead, Hamerly k-means bounds, parallel
  k-means update, mip pyramid instead of blur in `whole_worst`, one-file
  bridge test (`IMGFP_CUT`), cosine retrieval, best-edge-first propagation
  tree, k-truss / biconnected grouping, plain-tap pixel sampling.

## The window (`img-fp-gui`, feature `gui`)

- The scan is a **child process** (`img-fp-gui --worker RESULT <argv>`),
  so Cancel is SIGINT. Spawned from the GTK main thread (PDEATHSIG is
  per-thread) by our own fork + `execv` of `/proc/self/exe` (`scan::spawn`;
  `std::process::Command` pulls in GLIBC_2.39 weak symbols, `current_exe()`
  breaks after a package upgrade).
- Report copy for the window has no `pairs`, written to `$XDG_RUNTIME_DIR`,
  parsed off the main thread; read on exit 1 too.
- The Trash leaves a file whose length differs from the scan's, or whose
  ctime is from the scan's start on (`Notes::changed_after`): written,
  replaced, renamed or chmod'ed since. Suggestions resting on such a file
  turn REVIEW, as under `--last`.
- Rendered with cairo; textures premultiplied BGRA at drawn size
  (`thumbs::Size`); JPEG thumbnails via libjpeg scaled decode; freedesktop
  thumbnail cache used when current (`Thumb::MTime`, and `Thumb::Size` when
  written: mtime alone survives `cp -p`/`rsync -a`); HEIF/AVIF/JXL decode one
  at a time;
  texture budget by bytes (`KEEP_BYTES`, `STRIP_KEEP`).
- Mosaic (`gui/mosaic.rs`) is virtual and its tiles pooled; strip is a
  `ListView`; `still.rs` keeps texture arrival from relaying out the page;
  tooltips/classes set only on change. Marks are vectors by file number
  (`Member::id`).
- UI decisions by the user: pictures take the page, details in a bottom bar
  of fixed columns; one selected image; Shift marks ranges; nothing
  pre-marked, suggestions offered (*Mark suggested deletions*, Alt+D),
  rule switchable without rescanning; Trash dialog names groups with every
  image marked; animations off; mnemonics always underlined (Cinnamon grabs
  Alt); F1 lists keys; dark theme followed from the desktop's text colour;
  tree view by folder (`gui/folders.rs`).
- Mnemonics live in `gui/labels.rs` with a clash test; add labels there.
- Zoom (`Mosaic::zoom`, header `[-][+]`, Ctrl+Plus/Minus/0): a chosen row
  height, steps of √2 (twice/half the area); a group that fits the page at
  rows at least that tall is fitted as before. Limits are the page's: a row
  no taller than the page shown, none shorter than the tick plus a badge line
  (`Results::least_row`; an overlay measures as its main child only), no
  step down while all is on screen. `flow` ends each row where its height is
  nearest the target (stretch or shrink), not always after the overflow.
- Only paths persist (`$XDG_CONFIG_HOME/img-fp/gui.json`); options open at
  CLI defaults; `recursive` on. Folders on the command line replace the
  remembered ones. Last scan and its marks (`marks.json`, keyed by scan id)
  kept in `$XDG_CACHE_HOME/img-fp/last-scan/`.
- Testing: Xvfb + `xdotool windowfocus --sync` + `import -window root`;
  on the real Cinnamon display activate with `wmctrl -i -a`. Point
  `XDG_DATA_HOME` at scratch before testing Trash. Other distros in Docker
  with `--network host` and a distinct display number per container; install
  `librsvg2-common`. Wayland: sway headless (`setcap -r`, `wtype -s 400`,
  `grim`) or headless gnome-shell driven via `org.gnome.Shell.Eval`.

## Conventions

- **Canonical runner output**: `{"tool", "config", "runtime_seconds",
  "groups", "pairs"}`. **`pairs` wins over `groups`** everywhere (expanding
  a closure invents matches). img-fp's groups are representative + matches,
  each file entry the CSV row; `score.py` reads both bare paths and dicts.
- Reports give one overlap per pair, the larger direction (user's choice);
  `--dump` has both.
- **Exit codes** (vid-fp's): 0 clean, 1 fatal, 2 finished with problems,
  130 interrupt (instant; records are appended as made). A signal the run
  was started ignoring (`nohup`'s SIGHUP, a background job's SIGINT) stays
  ignored; the window's worker resets all three, since Cancel is SIGINT. **Every benchmark
  run exits 2** (`Foto 38622.png` is truncated); `bench.py`'s `OK_EXIT`
  accepts it.
- **Skips vs problems** (vid-fp's summary format). Skips: extension not
  taken by `-x` (extensionless files only under `-x '*'`), non-pictures under
  a wildcard (`decode::plausible` checks short signatures), hidden folders
  without `--hidden`, Trash/thumbnail/NAS recycle and snapshot folders
  (`walk::SetAside`: `#recycle`, `@eaDir`, `#snapshot`, `@Recycle`,
  `@__thumb`, `@Recently-Snapshot` too; and `lost+found`, root's 0700 on
  every ext4 drive, which made a drive scan exit 2 and refuse a prune), symlinks
  without `--follow-symlinks`, second names for one (device, inode),
  macOS `._` metadata files (AppleDouble magic, any name; were a problem and
  a byte-identical "image" pair), featureless images (were a problem: a
  folder with one plain backdrop exited 2 every run).
  Problems: undecodable, unreadable path, unresolved `--exclude`,
  cut-off JPEG (`decode::jpeg_cut_off`; analysed, marked
  `DAMAGED`, never cached), cache I/O failure.
- `-o`, `--dump`, `--log-file` may not name a picture or each other; at most
  one may be stdout. Reports and the dump are written whole or not at all
  (`report::write_whole`: beside the destination, through any symlink, with
  its mode, then renamed; an interrupt removes the half-written one): a
  failed write used to leave the last report cut off. `--cache` with `--no-cache` only beside
  `--clear-cache`.
- **Cache**: one file per machine, `$XDG_CACHE_HOME/img-fp/analysis-IMGFPC13.bin`
  (named for its format, so upgrades start a new file). Records keyed by
  canonical path + size + mtime + ctime, settings per record. Records for
  paths a run did not walk are carried over while the file exists, or
  while its volume (`/media`, `/run/media`, `/mnt`, gvfs, `/Volumes`) is
  unmounted (`cache::volume_root`). Appended as made; compacted only when
  dead bytes reach a quarter. Runs sharing the file hold it with `flock`
  shared; compaction, header writing and torn-tail repair need it alone
  (`exclusive`, never waited for) and are skipped otherwise: a compaction
  under another run sent its later appends to an unlinked file. A
  compaction keeps what another run appended after this one read the file
  (`Store::foreign`), and counts it live: it used to drop it. A record's
  file is gone only on "not found" (`cache::gone`): an unreadable folder or
  a dropped sshfs/NFS mount is not a deletion.
  A JPEG's colour, measured later by the suggestion, is written in place into
  its record's colour field as `-2 - c` (below the decode's -1, so a build
  of the same format that predates it reads "not measured"); the value used
  is `cache::kept_colour(c)`, so cold and cached runs decide alike.
  Another format version's file is left untouched. Damage costs only the
  damaged record. **Cost measurements must pass `--no-cache`.** Sweeps
  should use their own `--cache`. `--prune-cache` keeps only what the scan
  used; refuses after a partial walk.
- Extensions: DDS not taken; `.hdr`, `.pam`, `.apng`, `.dib`, `.avifs`,
  `.heics`, `.jif`, `.jfi`, `.pjpeg`, `.pjp`, `.jpg_large`, `.jpg_orig`,
  `.png_large` are. ICO with an RGB PNG is read (`decode::ico_png`). HEIF is
  recognised by any `ftyp` brand.
- Venvs: `vendor/venv` general; `vendor/venv-imagededup` (torch) for SSCD
  and imagededup; `vendor/venv-imgdupes` for imgdupes.

## Benchmarking discipline

- One tool at a time; >= 60 s cooldown then wait for measured idle + 3 C
  (never a fixed temperature). Clocks swing 1.2-3.0 GHz; wall moves 25% with
  temperature. Compare only alternating pairs in one session (A B B A),
  never a single before/after or `cpu x MHz`.
- Clear tool caches (img-fp `--no-cache`, czkawka `-H`, imgdupes
  `--no-cache`, fresh dupeGuru db, SSCD without `--embeddings`) and evict the
  corpus with `posix_fadvise(DONTNEED)` (no sudo here).
- Peak memory: RssAnon summed over the process tree, plus swap (swappiness
  100). PSS also counts `spill.rs`'s mapped file pages, which are reclaimable.
- `bench.py --only <tool>` merges into `metrics.json`; give it a fresh
  `--out`.
- `derived/Desktop` (636 files) is the quick subset; `--cache` isolates the
  matcher.
- Accuracy reproduces to the pair; cost figures are soft (third significant
  figure means nothing).

## Gotchas already paid for

- czkawka exits 11 on finding duplicates (`-W`), skips files < 16 KB unless
  `-m 1`; use `-z Lanczos3 --geometric-invariance mirror-flip-rotate90`.
- findimagedupes: space-separated output; needs a hard-link farm under
  `~/.cache` (it resolves symlinks).
- imagededup: no CLI, not recursive, keys by basename; runner builds a flat
  symlink farm.
- SSCD: unreadable files need a placeholder tensor of the batch's shape.
- difPy: run `.../site-packages/difPy/dif.py` directly; `-s 200` already
  merges everything.
- dupeGuru: runner supplies a PIL-backed `Photo` subclass.
- A tool that cannot decode a format looks like one that failed to match.

## Working style

- The user reads results critically and pushes back on measurement
  artifacts; check numbers against a second method before reporting.
- Do not start long tool runs without being asked.
- Report failures as results (they are in `BASELINE.md`).
