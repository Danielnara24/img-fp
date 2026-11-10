# img-fp: measurement history

What each version changed, how it was proved, and its numbers, moved out of
`CLAUDE.md` on 2026-11-10 (that file keeps how things work now and where they
stand). Before 0.44.0 the history is `git show 5de81bc:CLAUDE.md`. Figures
here are as measured then: several baselines were later found to read stale
cache records (see 0.81.0), so quote a figure with its date and build.

## Precision is measured where siblings cannot meet (found 0.72.0, checked)

`benchmark/out/v45-k/RESULTS.md`. **`-k` is not the cause**: full IMGS5 at `-k`
300-5,460 is identical (candidate lists are "files sharing a word", shorter
than 300), small families are flat from `-k` 50 (IMGS5) and 10-400 (IMGS-ALL);
the "merges fall as `-k` falls" rows are candidate starvation. **The
vocabulary is**: correspondences are sought only within shared words, and a
vocabulary trained on ~90 copies of each seed is finest around each seed's
descriptors, so a near-miss sibling's matches collapse (Puerto1/2: 489 matches
seeds alone, 300 at 2 copies, 125 at 10, 47 at 30, 19 in the full corpus; its
`blk` 0.71-0.73 in all) and fall under the points bar or are never met. Corpus
size alone does not do it: originals among 9,285 unrelated found photos pair
as many siblings as alone (IMGS-ALL 6, IMGS5 8). A real library is that regime.
There, at 1-2 copies a photo (3 draws each), IMGS-ALL joins **5-11** sibling
sets directly at pair precision **90.6-96.8%** (full corpus: 0, 99.78%);
IMGS5 7-13 at 64-71%. Recall does not move (IMGS-ALL 93-97%).

- **Not to be trusted as safety**: every precision, cross-family and "no
  merge" figure below and in *Parameters* measured on the full corpora; the
  cliff table; every constant justified by a merge cliff (`CLEAN_WORST_BLOCK`,
  `GRID / BLOCK`, flat-block bar, inlier tolerance, the claim-graph survey).
  The claim-graph rules guard merges of large families; at 1-2 copies the
  error is one direct verdict between two small clusters, which they pass.
- **Still good**: recall; pixel-check values (corpus-independent); the 0.72.0
  correlation choice (made on seeds alone); keep rule on IMGS-ALL (no DELETE
  rests on another photograph at any family size).
- **Where families are small**: correlation is the only bar that separates
  siblings (IMGS-ALL seeds 0.5-0.9: 18/13/9/6/5/4/2; Segovia1/2 and game2/3
  pass at 0.9); overlap one step down, 0.75, takes IMGS5 seeds 8 -> 14 (a
  portrait from the landscape's spot is a crop of it); points 10 -> 3 adds no
  sibling at <= 2 copies and +1.4 recall (it guarded siblings only where the
  vocabulary had already thinned them). `blk_min` alone does not separate:
  18-20% of true verdicts (warps, photo_of_screen, vignette) are at or below
  Segovia1/2's 0.70.
- **IMGS5 keep rule, never scored**: DELETEs resting on a sibling at every
  family size (bird1/2 burst nearly all; Floriana, Puerto3/4, Cemetery3/4), 123
  rows in the full run.
- **Open, to rethink**: (1) score precision on small families (seeds + 1-2
  variants, several draws; seeds among found photos), counting sibling sets
  joined, not >= 300-pair merges; full corpora for recall only. (2) A verdict
  that depends on what else is scanned: the same pair passes in a folder of 60
  and not of 5,460 copies. Verification independent of the vocabulary's
  fineness (correspondences not limited to leaf words) would make one answer,
  then the bars need re-choosing on (1).

## Scoring on small families, and guided matching (session v46, shipped in 0.73.0)

**Shipped in 0.73.0**: guided matching at D 320, ungated (`verify::guided`,
`GUIDE_D2`); `-k` counting rejections (`pipeline::retrieve_and_verify`'s
windows); the shared-part rule (`claims::demote_shared_parts`). Everything
below names them by the env switches they were measured under; those and the
rest of the scaffolding are gone from the tree (`IMGFP_GUIDE_D`,
`IMGFP_K_REJECT`, `IMGFP_PART_RULE`; not shipped: `IMGFP_GUIDE_GATED`, the
word floor `IMGFP_WORD_R` (shipped in 0.75.0 as `index::WORD_FLOOR`), `IMGFP_WATCH`, `IMGFP_CLAIMS_OFF`, `src/probe.rs`;
all in commit 5e6edeb). 0.73.0 reproduces the measured stack runs byte for
byte (full IMGS-ALL, full IMGS5).

`benchmark/families.py` scores precision where siblings meet (`run BINARY OUT
--cache C`, `score`, `compare`): seeds alone, seeds + 1 / 2 variants (three draws
each), seeds among the found photos, `--only full` for the whole corpus. The
figure to drive down is **total joined** (seed pairs reported as one picture,
n0 + mean n1 + mean n2 + f): 0.72.0 **IMGS-ALL 27.0, IMGS5 36.7**. Read recall
from the full corpora. A brute-force probe (every keypoint against every
keypoint, `src/probe.rs`, test-only scaffolding) passes 7 / 8 sibling pairs at
the default bars, the seeds-alone answer.

Scaffolding in `verify::guided`, on only under `IMGFP_GUIDE_D` (off: 17 reports
byte-identical to 0.72.0): the vocabulary's transform finds the correspondences
again by position, nearest descriptor under D within the inlier tolerance,
refit twice, so a verdict no longer depends on the vocabulary's fineness.
`benchmark/out/v46-matcher/RESULTS.md`. **D is derived, 320**: the distance at
which a pair expects one coincidental correspondence (the 1/C quantile of its
own off-window null, C its window trials; median over unrelated pairs, p10-p90
283-351; `probe/null.tsv`). Below ~250 D is a descriptor-similarity cut that
takes siblings, warps and recall down together (a ramp: joined 17.3 / 18.3 at
100), so it would be fitted. From 250 to 1000 (no bar) small families are a
plateau (joined 25.7-29.3 / 39.0-42.0) and no seed pairs with a found photo:
there the pixel check decides. D cannot go: with no bar an unrelated pair has
314 inliers (5.6 at 320), which the points bar and `encloses_centre` read. At
320 full IMGS-ALL F1 0.9773, precision 99.72%, recall 95.82%, cross-family 412
(no seed pair >= 300), perfect transforms 58; keep rule 0 lost, 298 / 304; small
families joined 26.0 / 40.3 (0.72.0 27.0 / 36.7). Cost +17% CPU on the found mix
(guided 68 s, pixel checks 1.3 -> 12 s); `IMGFP_GUIDE_GATED` (refine only what
clears the points bar already) +3.4%, small-family joined identical, recall
-0.4 to -0.9 points, full IMGS-ALL F1 0.9722, but a hypothesis the vocabulary
thinned below 10 is again left alone. The loop's rewrite (row runs, early stop)
is byte-identical and ~1.5%. Siblings against warps: no separator in descriptor
distance, residual, coverage, local alignment (a lattice-grown smooth field
helps warps, hurts composites, leaves siblings) or leave-one-out local
consistency over keypoint scale (only camera-moved siblings stand out, and
`wave_vertical` overlaps them). The nearest siblings (bird, Mural, Segovia) are
a static camera and changed content, which the truth calls an edit.
Where a sibling verdict depends on the corpus (`IMGFP_WATCH`, `watch/why.py`):
of the pairs standing on their originals alone (12 IMGS-ALL, 13 IMGS5), the
full corpus gives the same n_in wherever it meets them, but meets only 6 / 7
(ranked past `-k` or sharing no word: the score is 1-19% of its value alone),
and the claim graph refuses most of those met. On originals alone retrieval
does not filter (every file is inside `-k`). **Word floor**
(`IMGFP_WORD_R`, `Vocabulary::with_floor`): leaves of one parent within r = 143
(median over true pairs of the p90 descriptor distance between corresponding
keypoints) are one word. Meets 9 / 10; originals-alone reports identical, small
families flat over r 100-250, full IMGS-ALL F1 0.9773 -> 0.9780, recall 95.96%,
cross-family 502 (none >= 300), keep rule unchanged; +8% CPU on full IMGS5,
+2.6% on the found mix (shipped in 0.75.0, which found what it is for: a
low-detail picture and its copy sharing no word at all). **`-k` counts rejections** (`IMGFP_K_REJECT`: a file
whose window held standing verdicts asks for as many more, until `-k` are
rejected): meets 10 / 11, small families identical, full IMGS-ALL F1 0.9781,
CPU within the runs' spread (verified candidates +60% on IMGS-ALL, mostly
rejected early); no newly met pair is stated, the claim graph refuses them.
**The claim graph's rules off alone** (`IMGFP_CLAIMS_OFF`): all still
load-bearing. Cover test: small families joined 26.0 / 41.0 -> 31.0 / 48.0;
bridge test: bust1/2 merges on IMGS-ALL, Mural, Puerto3/4, Macizo5/6 on IMGS5;
cycle rule: Segovia1/2, and Mural, Parish Church; every anchor clean: 229
merges. No rule moves n0 or f: a pair of lone files is one verdict, so there
precision is the verdict's alone. Left: scores near the noise (Cemetery3/4,
Shenzhen1/2).
**Macizo1/2** merges through Macizo1's `crop_off_centre` (the distant
mountain both share): two Macizo2 copies match it clean, and two links into
one file are no bridge. The floor tips the second over `CLEAN_WORST_BLOCK`
(0.9005); guided + `-k` is 0.0067 short. **Shared-part rule**
(`IMGFP_PART_RULE`, `claims::demote_shared_parts`): a clean link holding one
way only (overlap bar one side) makes a part; a part whose containers lie in
two clusters (of the two-way clean links; a file with none is of the cluster
most of its one-way links reach) keeps only the most-linked of those
clusters (its own on a tie). First built with lone files as clusters of
their own: it cut every family's composites out, and Henares1/2 merged at
correlation 0.4 (cycle rule's exception, `hen/replay3.py`). Then own cluster
first (v2): IMGS-ALL lost 378 true pairs, 375 in `low-light` (two crops
two-way to each other drew four composites, and ~50 links into the main 57
went; `low/trace.py`). Now, on guided + `-k`: small families identical,
full IMGS-ALL F1 0.9773 -> 0.9774, no true pair lost (+3), perfect
transforms 58, keep rule unchanged, every demotion same-family; full IMGS5
F1 0.9340 -> 0.9425 (recall 96.30 -> 96.17), cross-family 21,939 -> 17,418,
Tree1/2 gone, Floriana3/4 2,665 -> 473, Fountain3/5 and Floriana1/2 as off
(crop-like, may merge: user's verdict; v2 cut them to 430-550, F1 0.9569),
**Mural1/2 248 -> 327** (over 300; fine by eye); IMGS-ALL cliffs identical
in merges at every step, IMGS5 never more. With the floor, Macizo1/2 5,813
-> 3 pairs (F1 0.9535). RESULTS section 11.
The stack (guided + `-k` + this rule) against 0.72.0, cached, ABBA:
found mix +16% CPU, full IMGS5 +70% (78 -> 132 CPU-s; `cost5.out`). On
IMGS5 guided is +45 s (its own refinement, not the pixel check), `-k` +13.5
s (candidates verified 487 k -> 866 k), the rule nothing (`prof5/`).

## Versions and sessions, 0.47.0 to 0.84.0

- **The field, 2026-10-28** (`benchmark/BASELINE.md`, `out/v54-field`), cold,
  one session, img-fp 0.75.0 plain build. IMGS-ALL (five corpora, 33,136
  files): img-fp F1 **0.974**, 54 / 87 perfect, 590 s, 3,751 CPU-s, 606 MB
  anon+swap; SSCD 0.729 (12,745 s, 71,057 CPU-s), imagededup_cnn 0.571,
  imagededup_phash 0.504, czkawka_tuned 0.414, pdq 0.402, every other tool
  under 0.12 and zero perfect (fclones' one is `copy_exact`); difPy finished
  this time (5.2 h, F1 0.004). IMGS-ALL2 (hard): img-fp **0.939**, 11 / 88
  perfect; SSCD 0.472 (80.4% precision, 33.4% recall, 67,821 CPU-s, run
  alone that evening), imagededup_phash 0.423, pdq 0.299, imagededup_cnn
  0.249, czkawka_tuned 0.197; difPy 0.003 (2.4 h, run alone the
  next night). Competitors lead on non-affine
  warps and overlays (SSCD on keystone / perspective / wave on IMGS-ALL, and
  on `perspective`, `pinch`, `swirl`, `wave` on IMGS-ALL2; phash on
  `tiled_watermark`, `jpeg_low`, `noise`, pdq on `blur` on IMGS-ALL2);
  containment is img-fp's alone on both (SSCD best of the rest on IMGS-ALL2,
  71-203 of 364 where img-fp has 358-363).
- Baseline corpus **IMGS-ALL** (IMGS + IMGS2 + IMGS3 + IMGS4: 27,659 files,
  304 seeds, 90 transforms). At the default `--work-size 512`: **F1 0.9821**,
  99.57% precision, 96.90% recall, 47 perfect transforms; 7 cross-family
  pairs, no family merge (`out/v33-fit-pyramid/RESULTS.md`; 0.45.0 was
  0.9805 / 95 cross-family). At 640 (0.51.0, `out/v36-cliffs`): F1
  0.9846, recall 97.40%, 8 cross-family, no merge. SSCD, the best competitor:
  F1 0.748, and every other tool had zero perfect transforms (the four-corpus
  field, since replaced by `out/v54-field`).
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
- 0.51.0 (audit fixes A-F; cache format IMGFPC14):
  - **A**: a lossless file is not deleted for a file of its frame and size
    whose pixels differ (`suggest::Ctx::own_pixels`): two lossless files
    are an undecided pair (REVIEW), a lossy one does not stand in for a
    lossless one. Why: invoice screenshots differing in the total ($1,250 /
    $9,750) or a line of text agreed at 1.00 and one was deleted (with a
    re-encode as third file, the other invoice; with a changed line, the
    original). Nothing the run keeps can see it: their keypoints matched at
    descriptor distance 0, and a residual at full working resolution read
    5.4 for the other invoice against 9-10.6 for a plain downscale. So lossy
    documents (two JPEG invoices) are **still open**.
  - **B**: `Traits::frames` (GIF/APNG/WebP animation, TIFF pages, HEIF
    sequence brands, JXL animation): deleted only for its own bytes, never a
    same-pixels twin (`Same::settle` skips it). Before: two GIFs sharing a
    first frame were "same pixels" and one DELETE; a 3-page TIFF DELETE for
    a PNG of page 1.
  - **C**: TIFF-container camera raw (`DNGVersion`, `CR` at byte 8, a
    reduced-resolution first directory, or `Make` + `SubIFDs`) is refused
    as a skip (`decode::Refused::CameraRaw`), still in the exact pass. Before: read
    as its 256-px preview and DELETE beside its JPEG under `-x '*'`.
  - **D**: a 16-bit master next to two JPEGs of its 8-bit export was
    outside `TAU` (7.96 summed, 0.5/cell: one requantisation) and was
    deleted for the q85. Now held by A (KEEP; the q85 stays KEEP too).
  - **E**: `decode::budget::cgroup_room` (v2 and v1, ancestors included): budget
    and `max_alloc` no longer use the host's `MemAvailable` alone. Before:
    under `MemoryMax=300M` one 1.5 MB TIFF OOM-killed the run (137).
  - **F**: TIFF read a strip / tile row at a time (`tiff::chunks`)
    and 16-bit PNG rows (`png::rows`), bit-identical to `other::whole`
    (pixels and traits; 8/16-bit, alpha, grey, tiled, LZW, deflate+
    predictor, JPEG, oriented, truncated). 12000x12000 TIFF peak RSS:
    8-bit 850 -> 20 MB, 16-bit 1.69 GB -> 19 MB.
  - Measured: IMGS-ALL pairs and groups byte-identical (1,128,340 pairs),
    0 lost, 297 / 304, 304 / 304, DELETE 16,873 -> 16,767, REVIEW 3,004 ->
    3,052 (lossless halftone / channel_swap / posterize edits now kept);
    pairs.py and IMGS-PROVENANCE unchanged to the row.
  - Cost, alternating pairs, cold unless said: IMGS-ALL -t 8 (bench.py,
    ABBAAB) base 2,735 / 2,690 / 2,647 CPU-s, new 2,804 / 2,838 / 2,794,
    every new run at a lower mean clock (1,789-1,833 vs 1,886-1,943 MHz);
    IMGS-ALL -t 4 (ABBA) the other way, new 1,632 / 1,675 vs 1,724 /
    1,746, the new runs at the higher clock. Sign follows the clock, so
    neither is the code: IMGS -t 4 -v (ABBA x2) base 262.9 / new 261.5 CPU-s
    mean, decode 93.25 / 93.5 s, features 120 / 120 s; IMGS-ALL cached
    (ABBA x2) 438.0 / 438.75; per-symbol ymm counts identical; the 304
    IMGS-ALL TIFFs decode 4% faster streamed. Peak PSS within run spread.
- 0.51.1 (audit fixes, no change to pairs or suggestions): `-o`/`--dump`
  replace only a regular file (`report::Dest`): a descriptor (`/dev/stdout`,
  `>(...)`, `>>`) is written as opened, a FIFO or device in place, with no
  probe open (it read as EOF to a FIFO's reader). "Changed since the scan" is
  to the nanosecond (`Notes::started_ns`; whole-second ctimes still read
  coarse): files written in the scan's first second were never deleted and
  the Trash refused them. `cache::gone` lists each folder once instead of a
  `stat` per carried record. The window says "review", not "weak match":
  most REVIEWs are undecided same-picture pairs at 1.00. Not measured on the
  corpora (no rule changed).
- 0.52.0 (the keep rule rebuilt, not retuned): `suggest::Component::refusal`
  answers "may `k` delete `d`" with a reason and decides nothing; what a
  reason makes of a file is the order of `Refusal` (`No < Softer < Held <
  Undecided`, by caution: the greatest of a file's refusals decides); every
  action is read once (`Facts::action`). Before: refusals were pushed into
  three side lists by a `bool` closure, read back with an unwritten order
  (Softer above the rest), so a held or undecided file a softer copy also
  refused was kept and deleted others. Proved first with that order kept: a
  random differential test (40,000 families of up to six files, every
  reason reached thousands of times) against the old code, identical; then
  the order corrected: 112 of 40,000 families move (28 fewer deletions, one
  more, a file deleted for its own bytes). IMGS-ALL, IMGS-PROVENANCE and all
  90 `pairs.py` reports byte-identical to 0.51.1 both ways (no corpus has
  the case). `suggest::revoke` is the one place a DELETE whose kept copy or
  own file has gone or changed becomes REVIEW (`--last`, the window's
  `note_changed`, `drop_gone`, `regroup`; was six loops).
- 0.52.1 (refactor, no change to output): "is this file another's" is
  one `same::Same` (bytes, analysis, colour; was three parallel vectors read
  by hand in eight places); an edge is one `Edge { i, j, v }` (the transform
  and inversion were held twice; 112 -> 76 bytes); a stated pair's report
  row, `--against` pair and keep-rule link are read from `Stated`'s methods
  once. IMGS-ALL (cold and cached), dump, IMGS-AGAINST, IMGS-PROVENANCE,
  all 90 `pairs.py` reports and the found corpus byte-identical to 0.52.0;
  cached IMGS-ALL peak RssAnon+swap (ABBA) 729 / 685 MB base, 635 / 631 new.
- 0.52.2 (refactor): `run` (1,572 lines, one scope, `against` named 46
  times, the verify step written twice, three hand-built exits) is
  `pipeline.rs`: stage functions with typed hand-offs, a `Scope`, one
  `Pass::verify`, a `Dump`, one `finish`; see *The pipeline*. IMGS-ALL (cold,
  cached, dump), IMGS-AGAINST (report, dump, `--list`), IMGS-PROVENANCE, the
  found corpus (report, dump), all 90 `pairs.py` reports, `--last`,
  `--skip-mirrored`, `--prune-cache` and the window's worker copy identical
  to 0.52.1 but for `runtime_seconds`. An empty walk now goes the common way:
  its `runtime_seconds` is the real one (was 0.0) and its log says "groups: 0"
  and "suggested: 0". Cached IMGS-ALL (ABBA): peak RssAnon+swap 633 / 631 MB
  base, 615 / 612 new; CPU 457.8 / 435.3 base, 440.2 / 437.8 new.
- 0.53.0 (decoding rebuilt): `decode.rs` (4,327 lines; four parallel
  `match`es on the format, for analysis, preview, colour and probe, each
  repeating the ICO, TGA, JXL and HEIF quirks; per-format facts in three
  more; `Traits` measured through a thread-local reset by hand) is
  `decode/`: a file is identified once and opened once by its format's
  module, which gives `Facts` and reads its pixels into a `Sink`; see
  *Decoding*. Fingerprints of every file of IMGS-ALL, the found corpus and
  a 118-file format zoo (`decode::tests::fingerprints`: working plane at 512
  and 160, size, traits, damage, colour, chroma, `preview(256)`, four
  `preview_fit`s, probe) identical but where meant: TGA, HDR and farbfeld
  are lossless (TGA and HDR have no signature, and the lossless list was
  only asked of sniffed formats, so their entries never applied); a camera
  raw file is refused by the preview too; a TIFF directory chain is read as
  far as it reads (a broken later pointer threw away a first directory's
  raw marker); placed previews of TIFF, HEIF, 16-bit PNG and EXIF-turned
  PNG are box-reduced while read, as 8-bit PNGs already were (±1 px, the
  row path's rounding; a turned picture's factor is taken at its shown
  size). Previews claim the decode budget. JPEG, WebP and JXL: no
  difference at all. IMGS-ALL (cold, cached, dump), IMGS-AGAINST (report,
  dump, `--list`), IMGS-PROVENANCE, the found corpus, the Desktop edge runs
  (`--last`, `--skip-mirrored`, `--prune-cache`, stdout, empty walk) and all
  90 `pairs.py` reports identical to 0.52.2 but for `runtime_seconds`.
  12000x12000 TIFF (8, 16-bit) and 16-bit PNG still peak at ~19.5 MB; under
  `MemoryMax=300M` both builds finish alike. Cold IMGS -t 4 -v (ABBA x2; one
  base run at 2,109 MHz against ~2,400 dropped): CPU 302.6 / 302.4 / 307.6
  base, 306.2 / 304.2 / 305.4 / 304.3 new; decode 122 / 106 / 111 s base,
  105-107 new; peak RssAnon+swap 410-437 MB base, 409-453 new. Code lines
  (no comments or blanks) 2,246 -> 2,256. Cache format IMGFPC15, for the
  lossless change (a record of 14 would keep calling those files lossy).
- 0.54.0 (the claim graph rebuilt, `claims.rs`; see *The claim graph*): a
  verdict's acceptance is one `verify::Standing` (`Rejected < Corroborating
  < Anchor < Clean`, each clearing every bar below it), read once by
  `Policy::standing`; a composed one is `Policy::composed`; the gate is
  `Policy::gate` (were four `Rules` read by seven `accepted(&policy.X)`
  calls). `Edge`, the bridge test (now a demotion of the clean flag),
  admission and propagation moved to `claims.rs`, over one `Clusters` that
  admission builds and propagation and corroboration read (were five
  union-find / BFS passes over the same partition). The claims are one
  `Vec<Claim>` with `How` it was made (were a three-list tuple, "`n_match ==
  0` is propagated", a `bool` in `Stated::Match` and string compares).
  Proved identical first: differential tests against the old code (2 M
  random verdicts against the four tiers; 3,000 random graphs through bridge
  test, admission, propagation and corroboration), then cached IMGS-ALL
  report and `--dump`, the `ref.sh` battery (cold, cached and `--against`
  IMGS-ALL with dumps, IMGS-PROVENANCE, the found corpus, Desktop edge runs:
  84 outputs) and all 90 `pairs.py` reports, identical to 0.53.0. Then
  each rule was switched off alone (IMGS-ALL at the default and its cliffs,
  IMGS3 alone): all but one load-bearing somewhere (*Tried and rejected*).
  The one: **`CLUSTER_SLACK_POINTS` (2) is gone**; a corroborated match needs
  the anchor's aligned points or a whole frame. IMGS-ALL: F1 0.9823 ->
  0.9822, recall 96.93 -> 96.90%, 425 pairs fewer (every one corroborated,
  same-seed, 8-9 points: TP -393, traps -22), same 7 cross-family pairs,
  perfect transforms 48 -> 47 (Henares1 `mirror_crop`, 8 points at 0.547),
  halves 0.9802 / 0.9830 -> 0.9801 / 0.9828. Cliffs: IMGS-ALL identical but
  overlap 0.6 (18 -> 16 cross-family) and overlap 0.5 (`docks`, 6,645 ->
  6,644); IMGS3 identical or fewer (points 8/7/5: 8/9/11 -> 7/7/10, overlap
  0.6: 8 -> 7). Keep rule: 16,767 DELETE, none lost, 297 / 304, 304 / 304,
  two edits REVIEW -> KEEP; `pairs.py` unchanged. Verified pairs read by the
  pixels 850,290 -> 829,142 (gate 8 -> 10 points). Cached IMGS-ALL (ABBA):
  CPU 466.6 / 498.9 base, 467.4 / 507.6 new (the runs' own spread); peak
  RssAnon+swap 617 / 623 MB base, 614 / 612 new (no copy of the anchors
  held through propagation). Code lines flat (4,784 -> 4,778 in the files
  touched).
- 0.54.1 (window only): options set away from their default are kept
  between runs (`gui.json` `options`, `settings::Kept`); untouched ones
  are not written, so they follow a moved default. Checked in the window
  under Xvfb: changed, scanned, reopened, restored defaults.
- 0.55.0 (a finished scan rebuilt, `outcome.rs`; see *A finished scan*):
  the report, `--last` and the window read one `Outcome` (groups by number,
  each member's `Evidence`, the content rule's suggestions); the group rules
  and gone / changed files are its methods (`actions`, `lapse`, `look`,
  `shown`). Before: the JSON report was also the storage, parsed back by
  `last::Kept` and `gui::scan::Member`, each applying the group rules and
  lapses its own way (`last::rebuild`; the window's `prepare`, `regroup`,
  `note_changed`, `drop_gone`), the 1.0-for-one-file agreement written
  three times, `keep_scan` swapping suggestions into the report and back,
  `OutPair` a relation as four `bool`s. Drift found: `--last` fell back to
  correlation 0.6 (default 0.7); the window forgot a reference once its
  group was down to it and the Trash was used again (`--last` kept it a
  reference; now both do). Kept scans of 0.54 and before are not read
  (`last-scan/found.json`, was `result.json`): run a scan. Proved: a random
  differential test (20,000 scans, three rules, random gone / changed sets
  and window Trash / change sequences) against 0.54.1's `rebuild` and window
  code, identical but the case above; `ref.sh` battery (104 outputs) and a
  `--last` battery on a Desktop copy before and after deleting and touching
  files (every format x scan rule x `--suggest`), identical to 0.54.1. The
  window under Xvfb: restored a CLI scan with a file gone, marked 309 (=
  `--last`), trashed, switched rule (68 = `--last --suggest correlation`),
  reopened with marks, scanned (7 = `--last`). Kept file on IMGS-ALL 19.0
  -> 11.1 MB. Cached IMGS-ALL (ABBA): CPU 467.0 / 481.3 base, 454.9 / 476.8
  new; peak RssAnon+swap 610 / 609 MB base, 605 / 612 new. Code lines (no
  comments or blanks, tests included) in the files touched 9,741 -> 9,398.
- 0.56.0 (a file's analysis and the cache rebuilt; see *The analysis and
  the cache*): before, `pipeline::analyse_all` did the cache's bookkeeping:
  it held the cache's map, took each walked file's record out and compared
  keys by hand (`in_cache`, `same_key`), moved a byte copy's record to its
  original, worked out `superseded` and the bar's `rewrite`, and carried
  `names` and `span_of` through `Analysed` into the suggestion for
  `keep_colour`; what survives was assembled in `save_cache` from four
  lists (this run's spans, `carry_over` over what was left in the map,
  `other_settings` filtered by `gone`, the store's own `foreign`). `Item`
  was `ok` / `err` / `damaged` (a copy's placeholder: not ok, no error), and
  skips were told from problems by comparing error strings. Now
  `cache::open` takes the run's files and the store answers by number
  (`take`, `append`, `keep_colour`); `Store::finish` is the one keep rule;
  `Item::read` is an `analysis::Read`; `decode::Refused` is a typed error.
  Drift fixed: a record made at another `--work-size` for a file on an
  unplugged drive was dropped (it was asked `gone` alone; only this run's
  settings were asked `unmounted`): one rule now, `cache::carried`.
  Proved: 200 random histories (4,836 scans at `-t 1` over subsets of a
  scratch tree, with deletions, chmods, byte copies, rewrites, added files,
  moved folders, two working sizes, `--prune-cache`, all three rules), base
  and new on the same tree: cache file bytes, reports, logs, stdout,
  stderr and exit codes identical (one seed's logs differed in timestamp
  width only); the `ref.sh` battery identical, and its three caches
  identical record for record (compared as multisets: at `-t 8` the append
  order varies run to run); all 90 `pairs.py` reports and the cache they
  leave identical; the `--last` battery identical. Interrupted runs keep
  their records and the next run reads them; two runs started at once on a
  new cache file kept both runs' records in 12 full trials and 80 small ones
  per build (one early base trial lost the second run's records, not
  reproduced; the open/lock code is unchanged). Window under Xvfb: its
  worker fills a cache the CLI then reads whole (635 / 635), and a scan from
  the window shows the CLI's 64 groups. Cached IMGS-ALL (ABBA): CPU 530.4 / 517.4 base (the first at 2,010 MHz, idle 83 C), 494.0 / 517.2 new; peak RssAnon+swap 626 / 604 MB base, 603 / 601 new; reports identical. Code lines (no comments or
  blanks, tests included) in the files touched 5,997 -> 6,106 (cache.rs
  +138, of which tests +47; pipeline.rs -84; lib.rs -61; analysis.rs +104
  new): the cache's API is wider (`Asked`, `Unasked`, `Kept`, `Compacted`)
  and the pipeline narrower.
- 0.57.0 (retrieval rebuilt, `retrieve.rs`; see *Retrieval*): before, the
  question "which files may this one be" was written twice: the direct pass
  (with the library rounds woven in by `if scope.against` / `first`) and the
  second look (its own query, rank and verify loop, three `Variant` literals
  indexed as `v as u8`), with `both_ways`, `rank_best`, `merge_variants`,
  `quantise` and `variant_features` in `lib.rs`, and the index's memory
  (postings dropped and rebuilt, the vocabulary an `Option` of `unwrap`s,
  the `ahead` lists) handled inline in the stages. Now `Index::candidates`
  answers any set of variants, `Index` owns the postings' and vocabulary's
  lifetimes (`let_go`, `turn`, `turned`), `sift::Variant` is `SAME` /
  `TURNED` with `Features::seen_as`, and who asks is `Scope::reached` /
  `re_askers`. The second look's verdicts join the direct list
  (`Pass::verify` keeps whatever stands; the dump's kind is read off
  `Verdict::variant`), and corroboration reads `Variant::SAME` verdicts by
  rule, with its measured reason (*Tried and rejected*). Proved: the
  `ref.sh` battery (cold, cached, `--against` IMGS-ALL with dumps,
  IMGS-PROVENANCE, the found corpus, Desktop edge runs) and all 90
  `pairs.py` reports identical to 0.56.0, both after the refactor and with
  the one list; per-symbol `ymm` counts identical. Cached IMGS-ALL (ABBA):
  CPU 560.8 (at 1,981 MHz) / 485.6 base, 497.3 / 503.7 new; peak
  RssAnon+swap 605 / 601 MB base, 600 / 604 new. Cached found corpus
  (ABBA): CPU 343.3 (2,006 MHz) / 293.7 base, 302.0 / 297.0 new; peak 182
  MB both. Code lines (no comments or blanks, tests included) in the files
  touched 7,049 -> 7,110 (`retrieve.rs` 198, of which tests 45).
- 0.58.0 (`--against` is an `Outcome`; see *A finished scan*): before, a
  run against a library ended in a second finish (`write_answers` beside
  `write_groups`) with its own copy of everything after the pairs:
  `against::Pair` / `Facts` / `Header` / `Row`, three writers, the stdout
  and broken-pipe handling, the evidence text, a holder-chain walk of its
  own and a sort by verdict string, and `Found` carried `against_pairs`
  beside `graph`. Drift: the text dropped "corroborated" (`report::evidence`
  said it; the copy wrote "9 points"), and a looping chain named a file kept
  where `kept_at_end` names none. And it could not be kept for `--last`.
  Now `Outcome::against` holds each scanned file's holder and library pairs,
  `against::answers` reads the verdicts as things stand, `report.rs` writes
  both layouts through one `write_layout`, `pipeline::finish` is one path
  (`outcome` + `numbered_groups`), and `--last` / `--last --list` rewrite a
  comparison (the window leaves one alone). Proved: a random differential
  test (old `answer` + `rows` + writers kept in a test module, 20,000
  libraries of up to 13 files with chains, mirrored / inverted / clean
  pairs, non-UTF-8 names) identical in all three layouts; with corroborated
  pairs too, 7,345 of 20,000 differ, all by the restored clause in the text
  alone. `ref.sh` battery (now with `--against` text, CSV, stdout and
  `--list -o`, Desktop and IMGS-AGAINST) identical to 0.57.0 (no IMGS-AGAINST
  row rests on a corroborated pair); all 90 `pairs.py` reports identical;
  `--last` of each `--against` report identical to the scan's (JSON less
  `pairs`), `--list` too. On a symlink farm of IMGS-AGAINST with a library
  file and a scanned holder deleted and a scanned file replaced: 11 repeats
  answered again (9 better by another library copy, 2 new), the changed file
  not a repeat, the gone ones left out. Cached IMGS-AGAINST (ABBA): CPU
  539.6 (2,140 MHz) / 508.3 base, 517.2 / 513.2 new; peak RssAnon+swap
  602 / 602 MB base, 604 / 602 new (the new one also keeps an 11.1 MB scan).
  Code lines (no comments or blanks, tests included) in the files touched
  6,452 -> 6,541: `against.rs` 375 -> 184, `report.rs` 741 -> 949 (the
  second layout), `last.rs` +30 (`--last` of a comparison), tests +60.
- 0.59.0 (the window's options are a table, `gui/options.rs`; see *The
  window*): before, each option the window shows was written out by hand in
  about thirteen places (a field of `Settings`, of `Kept`, two `From`
  lines, `Default`, `argv`; a field of `Setup`, its widget, struct init,
  `show`, `read`, a tooltip; `lib::Defaults`), so a place missed compiled
  and passed (the frozen defaults before 0.19, the clear-cache that deleted
  the default file). Now an option is one `Opt` (key, clap id, kind, words),
  its default read from clap itself (`img_fp::flag`; `Defaults` gone),
  `Settings` a value per option, `Setup` a `Control` per option, and a test
  (`every_flag_is_decided`) fails until every flag of `Args` is shown,
  passed by hand (`BY_HAND`) or not the window's (`NOT_SHOWN`). `--last`
  takes an allowlist (`WITH_LAST`, checked in `lib::parse`) instead of
  `conflicts_with_all` over eighteen scan options, so a new scan option is
  refused by it unnamed; same refusals, exit 2, clap's usage line worded a
  little differently. A settings value of the wrong type now falls back
  alone (it made the whole file the defaults, folders and all). Proved: a
  differential test (old `settings.rs` in a temporary test module, 20,000
  random settings stepped like spin buttons): img-fp reads the same options
  off both command lines (`img_fp::given`; only their order differs), the
  files hold the same JSON and each build reads the other's, and a flipped
  option always changes the command line; under Xvfb, base vs new with a
  kept `gui.json`: both tabs, the results page and "Restore defaults"
  pixel-identical, same worker argv (reordered), same saved `gui.json`,
  same report; cached Desktop scan and its `--last` txt / csv / json /
  `--suggest correlation` identical but `runtime_seconds`. Code lines (no
  comments or blanks) outside tests in the files touched 2,329 -> 2,323;
  tests +91.
- 0.60.0 (the window's keys and views, `gui/commands.rs`; see *The
  window*): before, a key was written in up to five places (an arm of one of
  six key controllers: the results page's image, Alt and Ctrl ones, the
  large view's, the scan page's, F1; a line of the hand-written F1 list; the
  results menu's two tables, one a view; its `gio` action; the mnemonic
  test's lists), and the page's two views were `tree_view: bool`, tested in
  ten functions, with three versions of "show what the view has selected"
  (`rebuild`, `toggle_view`, `reshow`). Now each part of the window that
  takes keys is a `commands::Section` of `Row`s (an F1 line and the chords
  it describes); a controller asks `Section::find` and does the command
  (`Results::key` decides only which keys the page takes from the focused
  widget, `Results::run` what each does, for keys, the menu and its
  actions alike); the F1 list, the menu, its actions and the quit
  accelerators are read off the tables. The views are `results::View`, each
  list one owner of its widgets (`Strip`, `Folders`), and `show_view`,
  `step`, `scope`, `side_list` the one `match` each. Tests: no key bound
  twice on a page, every command bound, every menu item a mnemonic and only
  those, a menu letter the same in both views, the F1 list as it was. Found
  by the table and dropped (user's decision): the large view also took Page
  Up / Page Down, and a folder list Backspace, which the F1 list never said.
  Proved under Xvfb with metacity, base vs new, three scripted runs (a
  kept `derived/Desktop` scan; a hard-link farm of it in nested folders,
  for the tree): every key of every section pressed, in both views, from
  the images, the lists and buttons, 60 screenshots pixel-identical and the
  marks files identical, but for the progress bar of a scan cancelled
  mid-run (before the two keys were dropped). Non-test code in the files touched 3,554 -> 3,783 (`commands.rs`
  321; `main.rs` -69; `results.rs` -20, `Results::new` 685 -> 529 lines);
  tests 111 -> 264.
- 0.61.0 (refactor, no change to output): what a run skips or could not
  do is one `problems::Note` per category (its weight, log tag and summary
  label in `Note::says`, its order in `Note::ALL`), counted by
  `Problems::note`. Before, each of the 16 categories was a `Tally` field,
  a line in `new`, a method, and an entry in `skips()` / `problems()` with a
  hand-counted array length, and a missed place compiled. The byte pass
  (`exact_groups`, its sample and whole-file hashes, the byte compare)
  moved from `lib.rs` to `same.rs`, beside the other two sameness answers.
  A transform's determinant, scale, mirroring and inverse are written once
  in `verify` (`det`, `linear_scale`, `mirrored`, `invert_affine`); the keep
  rule had its own `invert` and `mirrored` (its inverse divided by the
  determinant, `verify`'s multiplies by its reciprocal, so the last bit
  could differ). Proved: a tree that hits every category, ten configurations
  (wildcard, extension list, followed links, excludes, an unwritable cache
  and a cache path that is a folder, `--log-file` to a file and to stdout,
  JSON), stdout, stderr, log, report and exit code identical to 0.60.0;
  cached IMGS-ALL report (1,110,436 pairs, every suggestion) and console
  identical but `runtime_seconds`; all 90 `pairs.py` reports and lists
  identical; per-symbol `ymm` counts identical. Code and test lines (no
  comments or blanks) in the files touched 8,242 -> 8,212.
- 0.62.0 (compare view): the window's large view shows an image beside
  its group's reference (`C`, *Compare with reference*), the part each shows
  of the other outlined and the rest dimmed (`gui::results::Outline`). Drawn
  from `outcome::Member::at`, a `Place`: the verdict's transform put in
  fractions of each picture, four decimals, from representative to member;
  made only for representative-member pairs (`pipeline::placements`, after
  `group::find`, which moved from `finish` into the assembly). First built
  in `Evidence`: every one of the 1.1 M report pairs grew 28 bytes and the
  stable sort holds them twice, peak 640 / 643 -> 706 / 707 MB; now
  base 624 / 624 vs new 627 / 634 (cached IMGS-ALL, ABBA), CPU within spread
  (489-497 base at >= 2,200 MHz, 496-504 new). Kept scan 11.1 -> 13.6 MB
  (51,289 members). Report byte-identical (less `runtime_seconds`); kept
  scan identical less `at`. Checked against the generated truth (15,727
  same-seed members with plain or `flip_h` geometry): corner error of the
  shared area median 0.04% of its diagonal, p99 1.2%; 14 over 5%, each a
  match the scan itself verified that way (a symmetric scene matched
  unmirrored at 0.79-0.84, an Excel sheet one grid cell over). A scan kept
  before has no places: the view says to scan again. Orientation: the
  reference's right (X, red) and up (Y, green: towards its frame's top side)
  as arrows from the centroid of the other's frame's part on the picture
  (`axes_origin`: the frame's middle when all of it is on the picture, the
  picture's when the frame holds it all; it was the frame's middle whenever
  that was on the picture, so a slide's middle put a photo's arrows near
  its top left), its own on the reference and through the
  transform on the member (`overlays`, `axes_in`, `paint_overlay`), each
  reaching half way to the side of the frame's part on the picture it
  points at (`AXIS_REACH`, at least `AXIS_LEAST` px; `axis_tips`,
  `to_edge`), 1.5-2.5 px thick by length, both dark edges drawn under both
  arrows, and the caption in words (`turned`: whole
  degrees, a quarter turn snapped within 0.5°; exact copies of a seed read
  0.02-0.06° off, `rotate_small` 1.14°). Tested by rendering off screen and
  reading the pixels back (`the_axes_are_drawn_where_and_as_the_transform_
  puts_them`: 23 orientations, both sides, direction within 3°, origin
  within 4 px, length half way to the outline or the picture's edge and,
  for a frame wholly on the member, to its sides' middles within 3 px; it
  fails with X/Y swapped, the matrix transposed, the origin moved, Y down,
  a fixed length, a reach of 0.6, the frame unclipped, no least length or the old origin rule),
  and on window screenshots of a seed's crop, mirror, 90°, 180° and 1.14°
  copies: every direction within 0.9°, origins within 0.6 px of the frame's
  middle (1.8 px on the landscape one), lengths within 1.5 px of half way
  to the clipped frame's side (the round cap).
- 0.62.1 (window only): the large view no longer blocks the page. It
  shows the page's selected image and follows it (its arrows, the page's,
  a click on a tile), marks stay in step both ways, a group chosen on the
  page shows its first image, and asking again raises it; the info is three
  lines (path; size; evidence and suggestion). Checked under Xvfb; GUI
  tests pass.
- 0.63.0 (the stated pairs, `stated.rs`; see *The stated pairs*): a
  pair is stated once whichever way round it is met (`stated::unique`), and
  what it says is worked out once into a `Pair` (evidence, file scale,
  `Place`, clean) by file numbers, which the groups, `--against` and the
  JSON's `pairs` read. Before: the JSON's `OutPair`s (two `Arc<str>` names,
  two byte strings) were also where `numbered_groups` looked a member's
  evidence up (a map filled in name order, the last winning); the place
  was `pipeline::placements`, a second map copying that order by comparing
  names; `--against` took the first pair met (`(a, b, Evidence, bool)`
  tuples). Three answers to which pair states two files, held apart only by
  no pair being stated both ways (IMGS-ALL: 0 of 1,110,436). Proved: the
  `ref.sh` battery, `last.sh`, `lastag.sh` (only "N minutes ago" differs)
  and all 90 `pairs.py` reports and lists identical to 0.62.1; cached
  IMGS-ALL report and kept scan identical, `at` of all 51,289 members
  included. Cached IMGS-ALL (ABBA): peak RssAnon+swap 608 / 595 MB base,
  455 / 457 new; CPU 470.5 / 476.1 base, 444.9 / 459.1 new. Code lines (no
  comments, blanks or tests) in the files touched 3,691 -> 3,699.
- 0.64.0 (the report's records, `table.rs`; see *The report's records*):
  a row of a report or of `--dump` is written from one list of `Column`s
  (name, which layouts carry it, how its `Cell` is read off the row), from
  which the CSV header, the CSV cells and the JSON keys are all read. Before:
  a column was a field of `report::Row` / `AnswerRow` with serde attributes,
  a line where the row was filled in, an entry in `CSV_HEADER: [&str; 15]` /
  `ANSWERS_CSV_HEADER: [&str; 20]` (hand-counted) and a cell in a list that
  matched the header by position only (the dump's too), so a swapped cell
  compiled and passed; a path was three fields (text, bytes, raw) written by
  hand seven times; the six evidence columns were copied into every place.
  Changed on purpose (user's decision): a group row's JSON keys are in the
  CSV's order, so `path` follows `action` (was first). Proved: a random
  differential test (old writers in a temporary test module, 20,000 scans and
  libraries with odd names, every relation, lapses, all three formats and
  rules): text and CSV byte-identical, JSON equal as parsed (it fails with
  one cell swapped); `ref.sh` battery against 0.63.0 the same way. Code lines
  (no comments, blanks or tests) 1,800 -> 1,782 (`report.rs` 696 -> 524,
  `table.rs` 137, `write_dump` +17).
- 0.65.0 (where a run's output goes, `outputs.rs`; see *Where a run's
  output goes*): every rule about the report, `--dump`, `--log-file` and
  `--list` is `outputs::plan`, asked by the run and the window's pre-check
  alike, and each output is a `To` written one way. Before: four checks in
  `lib.rs`, each building its own list of the flags; whether the report went
  to stdout decided three times; four stdout writers. Changed on purpose
  (user's decision): (a) the log through a descriptor, FIFO or device is
  written as opened, as `-o` is (`--log-file /dev/stdout >> run.log` emptied
  `run.log`); (b) `--dump -` into a closed pipe ends quietly, as the report
  does (was exit 1, and no report written); (c) `-o -` under the window is
  refused (was accepted and not written); (d) the window's pre-check asks
  every rule the worker does (`--dump -` was refused only once it ran); (e)
  no output may name the kept scan (`-o .../last-scan/found.json` broke the
  next `--last`). Proved: a 75-case matrix of flag combinations and
  destinations (stdout, `./-`, `/dev/stdout` under `>` and `>>`, `/dev/fd/3`,
  `/dev/null`, FIFOs, links to files not there, folders, missing parents,
  pictures, two flags on one file, the cache, the kept scan, closed pipes,
  `--last`, `--against`, `--list`), 0.64.0 against new: exit code, stdout,
  stderr and every file left identical in 69, the other 6 exactly (a), (b),
  (e); the window's worker for (c); the quick `ref.sh` battery and
  `last.sh` identical, `lastag.sh` but for "N minutes ago". Code lines (no
  comments, blanks or tests) in the files touched 3,007 -> 3,030 (`lib.rs`
  -70, `report.rs` -151 of which ~145 moved, `outputs.rs` 265): simpler, not
  smaller. Also: 0.62.0-0.64.0 were never released: the compare view's
  arrows called `f64::hypot`, which made the window need `hypot@GLIBC_2.35`,
  and the release check refused it (`gui::results::length` now); 0.65.0 is
  the first release since 0.61.0.
- 0.66.0 (the progress bar rebuilt, `progress.rs`; see *The progress
  bar*): the bar is driven by the time left (each frame closes `dt / left`
  of its distance to the end), and `left` is counted work at a measured
  pace: the analysis timed file by file (decode by format, the rest by
  pixels analysed at the size `decode::working_size` and
  `sift::enlargement` give), the running stage by itself, a stage not yet
  run by one prior per stage scaled by this run's measured speed. Before:
  a `Forecast` of ~25 constants fitted to the found corpus (per-format ns
  per pixel and byte, pairs per query 0.75, lonely 0.8, edges per image,
  a second-look overhead), stretches fixed when a stage began, an
  `analysis_cost` that sized the working plane itself and missed the area
  rule, later library rounds and the assembly and suggestion in no stage.
  Bar error, mean |bar - elapsed share|, alternating base/new runs on
  first runs: found corpus 0.031 / 0.029 -> 0.009 / 0.013, `--against`
  (IMGS-AGAINST/IMGS) 0.053 / 0.036 -> 0.018 / 0.015, IMGS-ELONG 0.061 /
  0.053 -> 0.034 / 0.031, Desktop 0.090 / 0.100 -> 0.069 / 0.090, IMGS
  0.040 / 0.069 -> 0.042 / 0.072 (scratchpad `bar.py`: the window worker's
  frames, timestamped). Rescans, once the cache's answers and the second
  look's askers are counted as they are decided and the disk stages left
  out of the speed (three alternating rounds against the bar as first
  built): IMGS 0.230 / 0.165 / 0.208 -> 0.117 / 0.112 / 0.116, the found
  corpus 0.066 / 0.142 / 0.070 -> 0.042 / 0.037 / 0.047, Desktop (1 s)
  ~0.21 -> ~0.20; first runs within their spread under 6.5 GB of swap.
  Outputs: the full `ref.sh` battery, `last.sh` and `lastag.sh` identical
  to 0.65.0 (but "N minutes ago"); `-v` and `--log-file` end with a
  `pace:` line per stage, which is what the priors are read from. Per-symbol
  `ymm` counts identical; glibc floor 2.34. Code lines (no comments, blanks
  or tests) of `progress.rs` 397 -> 540, `pipeline.rs` 1,115 -> 1,105: the analysis's
  per-format pace and the stage table are new, not smaller.
- 0.68.0 (audit fixes A-J, not measured on the corpora; `Foto 38622.png`,
  IMGS's truncated `quadrant_rotate` trap of `car2`, is now analysed as
  DAMAGED and forms no pair in its family): `--against` gives a file both
  sides reach to the nearer root, and `-e` never covers a whole library
  (a library inside the scanned folder was empty); empty files and files
  of no picture format form no pair; failures say their cause
  (`img_fp::why`); a cut-off PNG is analysed as far as it goes; the cache
  keeps records of an fstab or left-behind mount point that is away; the
  crate leaves out `flatpak/` and `.github/`, the Flatpak's local build
  `repo-flatpak`, `.git`, `.claude`; the window's large view fits at the
  largest monitor's size and has *Actual size* (1), and its texture budget
  is in screens (`thumbs::keep_for`).
- 0.69.0 (audit fixes; cache format IMGFPC16):
  - **Window, gone files**: before acting (*Mark suggested deletions*, the
    Trash) and on showing results, the window looked only for files
    *changed* since the scan (`look().1`); a kept copy deleted in another
    program while the results were open left its DELETEs standing, and the
    Trash took the last copy. Now `Results::check_changed` takes in gone
    files too (`drop_gone`), and runs the `stat`s on another thread
    (`Outcome::to_look`, a `Look` that is `Send`; `gio::spawn_blocking`):
    on a share they were a round trip each on the GTK thread. An action
    waits for the look (`Results::checked`); a look that returns after
    another scan is shown is dropped (`showing_scan`). Checked under Xvfb:
    the KEEP deleted with `rm`, Alt+D marked nothing, the copies read
    "review: its kept copy is gone".
  - **Window, no Trash**: files gio will not trash (`NotSupported`: system
    mounts, shares with no `.Trash-$uid`) are offered for permanent deletion
    in one second dialog (`Results::delete_instead`), each checked with
    `changed_since` first. Before, they could only be reported as not moved.
    Checked under Xvfb on `/dev/shm`.
  - **Desktop entry**: no `MimeType=inode/directory`. Where the desktop sets
    no default for folders, the XDG order made img-fp the folder opener (a
    Flatpak's exports precede `/usr/share`; reproduced with `gio mime`). It
    is still reached by "Open With Other Application".
  - **Group head**: among files of equal reach, the content rule's KEEP,
    then REVIEW, then lossless, pixels, bytes, then the lowest index
    (`group::find`'s `better`; it was the lowest index, i.e. the path that
    sorts first). Six copies were headed by `UPPER.JPG` (q80), and
    `--suggest representative` deleted the lossless original. The content
    rule is now worked out on every run, whatever `--suggest` shows. On
    `derived/Desktop`: pairs and every action identical, 54 of 64 heads
    changed, heads that are KEEP 25 -> 61. IMGS-ALL cold, 0.68.0 against
    new: all 1,110,262 pairs and every action identical, groups 686 -> 687,
    heads that are KEEP 373 -> 657; keep rule 16,763 DELETE, none lost,
    295 / 304, 304 / 304 both (295, not 297, since 0.68.0 made `Foto
    38622.png` DAMAGED; not recorded then).
  - **Cache, moved files**: a walked file with no record under its name is
    answered by another name's record whose key is its own to the
    nanosecond (`Key::says_which_file`: ctime not a whole millisecond, so
    not FAT/exFAT, whose coarse clocks can give two pictures one key). Such
    records are unpacked during the read (`open`'s `by_key`), and the first
    `take` appends the record's bytes again under the new name and key
    (`Store::relabel`, no repacking); the old one goes with its path. A
    renamed folder or a drive mounted elsewhere was analysed again whole. A
    metadata-only change (chmod, a new hard link) still re-analyses: telling
    it from a rewrite needs a content hash in the record. Report after a
    rename identical to a cold run's. **Reverted after 0.69.0** (see the
    next entry and *Tried and rejected*): the key does not say which file.
  - **Cache, emptied folders**: a record carries the device its file lay on
    (`Key::dev`, not compared by `Key`'s `eq`), and `away`'s left-behind
    mount point needs the empty folder to be on another device than the
    files were: `rm -r photos/2019` from a `photos` holding nothing else
    kept its records for good.
  - **EXIF** is each format's `Facts::exif`: a JPEG's APP1 in its first
    128 KB, a PNG's `eXIf` anywhere before IEND, a WebP's `EXIF` chunk
    (after the picture, no `Exif\0\0` marker: no WebP had EXIF), a TIFF's
    EXIF directory tag 34665 (none had), a HEIF's or JPEG XL's marker
    anywhere in the file.
  - **Describing is under the decode budget** (`decode::describing`, 56
    bytes a pixel of the plane described, measured 47-58 at work sizes
    2048, 4000 and 0): at `--work-size 0`, eight 24 MP photos at `-t 8`
    peaked at 3.6-3.9 GB and took 200-365 s in swap; now 1.9-2.0 GB, 20 s,
    report identical. At the default, `derived/Desktop` cold (ABBA) 44.1 /
    47.5 CPU-s base, 44.8 / 46.7 new.
- 0.70.0 (audit fixes; cache format IMGFPC17; `out/v38-carries`). IMGS-ALL
  cold against 0.69.0: all 1,110,262 pairs and every action identical (0
  lost, 295 / 304, 304 / 304). `pairs.py`: only `strip_metadata` moves,
  originals deleted for their stripped copy 244 -> 113, stripped copies 55
  -> 186; loss 24 both. (With `carries_more` on every twin, not only proven
  ones: 95 / 204, and IMGS-ALL 292 / 304, see `bw3` below.)
  - **A moved file is analysed again** (intended). 0.69.0's moved-file
    records are gone (`by_key`, `Store::relabel`, `Key::says_which_file`);
    `Key::dev` stays for `away`. Why: ctime is the kernel's coarse clock
    (a tick), so files written or `touch`ed together share (length, mtime,
    ctime) to the nanosecond: 16 of 20 files `touch -d`'d at once had one
    key. Six same-size BMPs touched together, scanned as `z/sub1` and
    `z/sub2` in two runs: `sub2/p3` took `sub1/p1`'s analysis, and a scan
    of `z` reported them "same pixels", one DELETE. No rename was needed.
  - **Same pixels are not the same file** (`suggest::Ctx::carries_more`):
    of twins *proven* the same pixels (`Link::Exact`: the same bytes, or two
    JPEGs of one coded picture, `same::Kind::SameCoding`, a proof the colour
    check already made) the keeper is the one carrying most besides its
    pixels, then the smallest (was the smallest alone, before EXIF was ever
    asked), and a twin is not deleted for one lacking what it carries
    (`Refusal::Held`): more than eight bits a sample (`Traits::deep`), a
    colour profile (`Traits::profile`), EXIF. A camera JPEG lost to its
    EXIF-stripped copy before. Twins only read alike (`Link::Twin`: one
    analysis and one colour) are chosen by size as before, by the user's
    decision: applied to them, IMGS2 `bw3`'s posterized PNG (up to 18 levels
    off, carrying the profile) was kept over its lossless WebP. So an Adobe
    RGB PNG still loses to its untagged copy, and a 16-bit master (64,352
    tones) to its 8-bit PNG export: nothing proves those twins' pixels the
    same without decoding them. A JPEG whose MPF index holds more than one picture
    (Ultra HDR gain map, MPO second eye) has `Traits::frames`, so it is no
    twin and is deleted only for its bytes (both it and its stripped copy
    are KEEP: a held file deletes nothing). Profiles are compared by
    presence, not content: two files with different profiles are still
    twins. HEIF says no depth. `derived/Desktop` of IMGS, IMGS3, IMGS4:
    pairs identical; one action pair moved (IMGS4 `aerial2` `to_png`, which
    keeps the seed's iCCP, now KEEP over its `webp_lossless` twin).
  - **`away`**: an `/etc/fstab` mount point that is mounted no longer ends
    the question: the left-behind sign is still asked, so an sshfs share
    under a `/home` partition (Fedora's default layout lists `/home`) keeps
    its records while it is away. Probe: away=false before, true now.
  - **`suggest_score.py` strict lines**: the same losses and families with
    no composite and no mirror image or rotation counting as a holder.
    `holds` lets either stand in for a deleted original, IMGS-ALL has dozens
    a family and a real library rarely has one. 0.69.0: 0 losses by the old
    line, 56 by the strict one; families whose picture survives only in a
    composite, a mirror/rotation or an edit, 4 (`b&w2`, `document1`, `logo`,
    `bw1`: originals deleted for `temperature` or `jpeg_low`). Their
    lossless copies (`to_png`, `to_tiff`, `webp_lossless`) went with the
    original for the same edit, so `own_pixels` does not see them: it looks
    as if pixel twins stand as one lossy file there. Not yet checked.
  - **No output is named as a picture** (`outputs::plan`, by extension):
    `-o report.png` wrote a text report, and every later scan of its folder
    counted it a picture that would not open (exit 2).
- 0.71.0 (audit fixes B-H; cache format IMGFPC18;
  `out/v39-audit`). IMGS-ALL cold against 0.70.0 (`v38-carries/all-new2`):
  all 1,110,262 pairs and 687 groups identical; 0 lost, 295 / 304, 304 / 304;
  DELETE 16,763 -> 16,697, REVIEW 3,074 -> 3,156 (E, below). `pairs.py`
  identical to the row (352 originals deleted, loss 24, REVIEW 29,391).
  - **B** a JPEG holding a video or another JPEG after its picture (Google /
    Samsung Motion Photo, a gain map with no MPF) has `Traits::frames`
    (`jpeg::end`, `holds_media`): its still copy was "same pixels", smaller,
    and the Motion Photo DELETE.
  - **F** an MPF index counts its MP entries less large thumbnails (types
    0x010001/2): many cameras list one, and every such JPEG was a file of
    several pictures, so a copy differing in a tag was a 444-point match and
    both KEEP, not "same pixels".
  - **C** `Note::EmptyLibrary`: a `--against` path that puts no image in the
    library is a problem (was "against 0 library images", all `new`, exit 0).
  - **D** the cache's index of heads (`cache::heads`; see *The analysis and
    the cache*).
  - **E** the undecided rule between two files of exactly one frame counts
    only files of that frame (`Component::pictures_of_one_frame`): a crop from
    the other side was a third picture, so a q92 file was deleted for its own
    q88 re-encode, which alone were undecided. Between two mutual crops the
    class is counted whole as before (counted narrowly, 200 deletions of
    slight crops went to REVIEW). Cost on IMGS-ALL: 66 DELETE -> REVIEW, all
    derivatives (`rotate_small` 30, `print_scan` 21) whose original stays.
  - **G** `matched.rs`: an unchanged rescan takes the last run's matching.
  - **H** the CLI ships built on glibc 2.28 (see *Speed and memory*).
  - Not done (user's call): **A**, a quarter or half turn deletes like a
    plain copy, so in a family of three a q95 upside-down copy was KEEP and
    the upright q87 original DELETE, and `--against` called an upright photo
    a `repeat` of a sideways library copy; a mirror deletes nothing.
- 0.72.0 (audit fixes C-I; C-H change no pair or suggestion: IMGS-ALL report
  and all 90 `pairs.py` reports identical to 0.71.0's with I off; I moves
  them, below):
  - **C** a rescan no longer reads byte-identical copies again: the pairs
    the last run compared and found identical are kept beside the cache
    (`<cache>.same`) and taken while both keys are unchanged (see *The
    analysis and the cache*). 47 duplicate pairs, 251 MB, page cache
    evicted: 0.72 s -> 0.10 s; a copy rewritten at its size (mtime put
    back), touched, added, or its first file deleted: handled as a cold run.
  - **D** the kept matching's fingerprint is the build (`IMGFP_BUILD`, from
    `build.rs`) rather than the program's bytes, and leaves out the thread
    count: IMGS-ALL matched at `-t 3` gives the `-t 8` report to the byte;
    the window's worker takes the matching a CLI scan kept.
  - **E** a key is the kernel's signed times: a file dated before 1970 is
    cached, and a run holding one takes the kept matching (one such file
    in 636: 1.08 s a rescan -> ~0.2 s).
  - **F** `Note::Subfolder`: the subfolders a scan without `-r` passes over
    are a skip, counted in the summary.
  - **H** `--list` without `-0` refuses a path holding a line break (exit 1,
    nothing listed).
  - **A, B** (the keeper among lossy copies, a lossless file of degraded
    pixels): tried and reverted, see *Tried and rejected*.
  - **I** a block of the pixel check that is flat in one picture only now
    counts (`verify::pixel_check_with`), at its |r| times SSIM's contrast
    term, `2 |cov| / (va + vb)`; a block flat in both still abstains. Since
    the first version it abstained when flat in *either* (the comments said
    "both"), so detail added onto a flat area or painted out agreed at 0.9
    a block: a person added onto a plain wall joined IMGS5's Mural1/2.
    First built at r = 0, which misfired on blocks whose two sides lie
    either side of the flatness bar (variance 10.2 against 16.1 at a bar of
    16, measured r 0.994-0.999; `crop_strip_top` pairs went to REVIEW, and
    bird1/2's burst split); plain |r| scored like the contrast term here
    but lets a near-flat ramp agree with stripes (|r| 0.88 seen), and a
    plane fitted out of each block lost real shared gradients. No new
    constant. `--min-pixel-correlation` stays **0.7** (user's decision,
    after the seeds-alone sweep below; the rule was first shipped in this
    tree at 0.6 with `CUT` 0.75). Measured at every step of every bar on
    IMGS-ALL and IMGS5 (`out/v44-flat-contrast`, `summary.py` there: 0.71.0
    and r = 0 from `out/v42-flat-cliffs`; the rule at 0.6 in
    `out/v43-flat06`; page https://claude.ai/artifact/6HBZCeWVkzTFoiSBpNyGYP).
    0.71.0 -> r = 0 -> shipped, at 0.7: IMGS-ALL F1 0.9747 -> 0.9683 ->
    **0.9710**, precision 99.60 -> 99.79 -> 99.78%, recall 95.43 -> 94.04 ->
    94.56%, traps 4,398 -> 2,238 -> 2,359, cross-family 7 -> 10 -> 10
    (largest 2), perfect transforms 49 -> 45 -> 47, no merge; IMGS5 F1 0.9119
    -> 0.9405 -> 0.9325, precision 86.35 -> 93.15 -> 90.84%, recall 96.61 ->
    94.96 -> 95.79%, merges 8 -> 5 -> 5 (Mural1/2, Puerto3/4, Macizo5/6 gone;
    bird1/2, Fountain3/5, both Floriana pairs, Tree1/2 stay; the precision
    r = 0 has over the rule is bird1/2 split, which by eye may merge). Cliffs:
    IMGS-ALL merges nothing at any step; IMGS5 never more merges than 0.71.0
    at a step and fewer at most (points 4-3: 6 against 8; overlap 0.75-0.3:
    7, 0.25: 8; correlation 0.9-0.2: 5 against 7-8), higher F1 at every step
    but overlap 0.25 and `-k` 10. Keep rule: IMGS-ALL 0 lost, 295 -> 297 /
    304, 304 / 304, DELETE 16,697 -> 16,658, REVIEW 3,156 -> 3,073, strict
    line 56, picture only in an edit 4 -> 3 (`logo` back); `pairs.py` (0.71.0
    -> r = 0 -> shipped) loss 24 -> 20 -> 23, originals deleted 352 -> 335
    -> 344, copies deleted 5,043 -> 4,849 -> 5,024, REVIEW 29,391 -> 29,502
    -> 29,291; IMGS-PROVENANCE unchanged (126 / 266, 0 lost). Found corpus
    (unlabelled): pairs 2,230 -> 1,732 -> 1,855, groups 737 -> 730 -> 734.
    **IMGS5's 60 seeds alone** (no variants, so every seed is compared with
    its sibling; inside the full corpus a seed's candidates are its own
    variants and the sibling is never asked, see the section at the top):
    the shipped rule pairs exactly what 0.71.0 pairs at every bar 0.9-0.5,
    8 seed pairs at 0.7 and 13 at 0.6, five of them the ones 0.54.0 named
    (GanyutingStore, Kaper-12/13, Lizard, Los Gigantes, Macizo1/2); that, not
    F1, is why the bar stayed at 0.7. Puerto3/4, Macizo5/6, Tree1/2 and both
    Floriana pairs pair as seeds at no bar down to 0.2; IMGS5's merges are
    made through variants. Cross-family 7 -> 10 (0.6 had 13), not a bug:
    the rule can only lower a verdict's agreement; eleven of 0.6's thirteen
    were `tiled_watermark` on both sides (one generated stamp over two
    photographs), and IMGS2 ducks1's contact sheet, whose only clean link in
    its family (to its `print_scan`) lost it to one block where the sheet's
    last sample row takes in a thumbnail pixel of the next cell (measured r
    0.04), so as a lone file it joined IMGS4 harvester's sheet (same
    generated filler; the verdict, r 0.81, was refused in 0.71.0 by the
    cluster rules).
- 0.73.0 (the matcher: guided matching, `-k` counting rejections, the
  shared-part rule; see *Scoring on small families* and *How it works*;
  `out/v46-matcher`, RESULTS sections 5-11). Against 0.72.0 at the defaults:
  IMGS-ALL F1 0.9710 -> **0.9774**, precision 99.78 -> 99.73%, recall 94.56
  -> 95.83%, perfect transforms 47 -> 58, cross-family 10 -> 479 (largest
  Acueducto1/2 139), no merge; IMGS5 F1 0.9325 -> 0.9425, precision 90.84 ->
  92.40%, recall 95.79 -> 96.17%, cross-family 21,375 -> 17,418, merges 5 ->
  5 (Tree1/2 out, Floriana3/4 2,654 -> 473, Mural1/2 in at 327; Fountain3/5,
  both Floriana pairs and Mural1/2 judged acceptable by eye, user's verdict);
  Macizo1/2 does not merge (it did with the word floor, 5,813, before the
  rule). Small families joined IMGS-ALL 27.0 -> 26.0, **IMGS5 36.7 -> 40.3**
  (guided matching: siblings the vocabulary thinned are now judged on their
  pixels). Cliffs, every 0.72.0 step (`cliff6_table.out`, page
  https://claude.ai/artifact/UQpb24tWzdheHiFdKfANx8): IMGS-ALL recall higher
  at all 24, F1 at 23 (equal at correlation 0.2), precision lower at all 24
  (0.01-0.6 points, 1.0 at overlap 0.25); **new merges at the far steps**,
  none in 0.72.0: docks1/2 at overlap 0.5-0.25, Acueducto3/4 at 0.25,
  red_rocks_sign1/2 at correlation 0.3-0.2, Henares1/2 at 0.2 (the same with
  the rule off: guided matching's). IMGS5 F1 higher at 23 (overlap 0.3
  level), merges fewer at points 4-3 and correlation 0.9-0.8, more at overlap
  0.3 and `-k` 50-10 (0.72.0 starved there). Keep rule: IMGS-ALL 0 lost, 297
  -> 298 / 304, 304 / 304, strict line 56 -> 55; `pairs.py` loss 23 -> 23,
  originals deleted 344 -> 341, copies 5,024 -> 5,036, REVIEW 29,291 ->
  29,476. Cost, cached matching (ABBA): found mix +16% CPU, full IMGS5 +70%
  (78 -> 132 CPU-s; guided +45 s of it, `-k` +13.5 s, the rule nothing).
  Released build identical to the measured env-switched runs (full IMGS-ALL
  and IMGS5 reports byte for byte); tests pass.
- 0.73.1 (matching faster, output byte-identical): guided matching
  (`verify::guided`) scans a keypoint's three grid rows as one branch-free
  loop with a select for the nearest (-17% of the function), keeps a
  keypoint's last match when a refit moved it less than its proven room
  (`Held`: to its cell's side, and `e^2 + 2 tol e < |r^2 - tol^2|` for every
  B point it looked at, a 1e-3 tolerance guard; asked only when a corner of
  A's frame moved under half the tolerance), prefetches B's descriptors,
  and builds the grid in one pass fewer; held to the old loops bit for bit
  by `guided_matching_finds_what_its_reference_does`. The misses were not
  the cost: reusing distances across rounds was slower; branch
  mispredictions in the row scan were. `-k` counting rejections no longer
  queries a file again to count its window (each pair carries which end
  proposed it), `stood` grows by the round's verdicts instead of being
  rebuilt, and a query ranks two windows, the second kept for the next
  round (`ahead`; `rank_best` is a total order, so a prefix is the same
  answer). Identical to 0.73.0: cached and cold IMGS-ALL, IMGS5 and the
  found mix, IMGS5 `--dump`, IMGS-AGAINST `--against`, all 90 `pairs.py`
  reports. Cached matching (ABBA, plain builds): IMGS-ALL 713 -> ~600 CPU-s,
  wall 113 -> ~97 s; IMGS5 130 -> ~113; found mix 329 -> ~315. Cold (ABBA,
  machine throttled at 1.4 GHz): IMGS5 422 -> 398 CPU-s, found mix 873 ->
  863, IMGS-ALL 2,826 -> 2,822 (spread 2,725-2,918, larger than the ~90
  expected). Peak RssAnon+swap cached IMGS-ALL +18 MB (the kept windows,
  600 bytes a file; it shows in the run's peak); cold IMGS-ALL 580 / 566
  base, 598 / 671 new, not settled.
- 0.74.0 (audit fixes A-F; cache format IMGFPC19):
  - **A** `index::query_touched` (an index where a word is in every image:
    a small folder, or one all one picture) listed an image once per such
    word, since its score stayed 0; `-k` counting rejections then asked
    through every repeat (3 files: 209 rounds; 5 files at `-t 1024`: 9.2 s,
    now 1.9 s). Listed once; reports identical.
  - **C** an EXIF-turned picture is reduced straight into the plane as
    shown (`reduce::Turned`), its plane the upright copy's to the bit: a
    turned PNG and its upright copy were 482 points at 1.00, both REVIEW, at
    512; now same pixels. No corpus has a turned file; +2% CPU on turned
    12 MP JPEGs.
  - **D** the kept matching's fingerprint names files by their canonical
    path: `photos`, `./photos/`, an absolute path or a followed link matched
    again (Desktop 1.4 s against 0.16 s), and the window never took a CLI
    scan's. Reports identical to a fresh match for every spelling.
  - **F** the window's Trash and permanent delete look at each marked
    deletion's kept copy again just before moving it (`results::Move`): one
    deleted while the dialog waited took the last copy (checked under Xvfb,
    base vs new). The dialog also says when a kept copy has already gone.
  - **B** `-k` counting rejections credits no verdict with a file already
    clean-joined to the asker (see *How it works*): 1,600 copies 1,251,350
    candidates, 89-97 s -> 258,498, 32 s. IMGS-ALL pairs and actions
    identical, IMGS5 F1 to four places (2 TP fewer), `families.py`
    identical, found corpus byte for byte.
  - Measured, not done (user's decision: cold and cached runs stay equal):
    incremental matching. One unrelated file added to the found corpus moved
    818 pairs out and 393 in (215 files' suggestions), all marginal (median
    17 points, correlation 0.72), through the vocabulary retrained on 60 more
    descriptors; re-matching the same set is byte-identical, and file order
    does not matter.
- 0.75.0 (audit A-C; `out/v53-audit/RESULTS.md`; no cache format change):
  - **A, the word floor** (`index::WORD_FLOOR` = 143, `Vocabulary::
    with_floor`; v46's `IMGFP_WORD_R` made permanent): leaves of one parent
    within the floor are one word. Why: an image with fewer descriptors than
    its share of the vocabulary sample (`per` in `index_all`) puts all of
    them in, and so does each copy; k-means then gave each copy of a
    descriptor a leaf of its own. A 42-keypoint logo and its PNG or q95 copy,
    among 15-120 found photos, shared no word in 28 of 30 folders (every word
    one leaf over from its twin's), so the copy was never a candidate; now
    0 of 30. IMGS-ALL F1 0.9735 -> 0.9739 (recall 95.86 -> 95.98%, precision
    98.88 -> 98.85%, cross-family 11,932 -> 12,227, same five merges over 300,
    Floriana3/4 474 -> 554, perfect transforms 51 -> 54); IMGS-ALL2 0.9356 ->
    0.9390 (89.01 -> 89.60%, cross-family 13,515 -> 13,091, Henares1/2 200 ->
    0, perfect 7 -> 11); small families joined 66.0 -> 65.0, 63.7 -> 63.0,
    n1 recall +0.57 points on both. Keep rule 0 lost; originals kept 358 ->
    358, IMGS-ALL2 352 -> 351 (three families out, two in, each where
    `webp_lossless` was the last lossless survivor). Cost, cached matching
    (ABBA): IMGS-ALL +7.9% CPU, found mix +6.0%. Rejected beside it: training
    the deepest level on no two members within the floor (`IMGFP_SAMPLE_R`),
    +1.1% / +3.4% CPU, same recall, but small families joined 67.0 / 65.7
    (boat_photo1/2 directly as originals; galaxy | sea_photo4, two unrelated
    seeds, in one draw). The released build reproduces the measured
    switch-on runs byte for byte (IMGS-ALL, IMGS-ALL2, `families.py`).
  - **B, a damaged file is not preferred** (`suggest::Picture::damaged`):
    of a class the keeper is an intact file where there is one (`choose`),
    a damaged file may not delete an intact one (`Refusal::Softer`, beside
    `softer`: it shows less, by what was cut off), and among files of equal
    reach an intact one heads the group (`assemble`'s `better`). Before, a
    JPEG missing only its EOI marker (same pixels, 2 bytes smaller) was KEEP
    and its intact original DELETE under every rule, and a copy cut at 85%
    beside two intact copies headed the group and `--suggest representative`
    deleted both intact files. Left: in that case the content rule still
    gives the cut file KEEP (best match 0.89, under `REVIEW`, lowered by the
    damage itself). IMGS-ALL and IMGS-ALL2 reports identical with A off.
  - **C** `spill.rs`'s `O_TMPFILE` is `__O_TMPFILE | O_DIRECTORY` per
    architecture (`O_DIRECTORY` is 0o40000 on ARM and POWER, where the old
    value was `O_DIRECT`, EINVAL, and every scratch file the named
    fallback); values checked against the `libc` crate's.
  - `bench.py`: each tool runs with `oom_score_adj` 1000 (the kernel kills it,
    not the desktop, when memory runs out), in a session of its own,
    `--timeout HOURS`, and `--only` runs tools in the order given.
- 0.76.0 (audit A-I; cache format IMGFPC20). IMGS cold against 0.75.0:
  report and console identical with `Foto 38622.png` (the one cut-off file)
  excluded; with it, its own analysis changes (C), which retrains the
  vocabulary and moves marginal pairs (224,076 -> 224,181; F1 0.972 both,
  TP +115, FP -15); that file pairs with nothing either way.
  - **A** `--cache` naming a file that is not a cache (`cache::is_stranger`:
    not a regular file, or not beginning with `IMGFPC`, however short) is
    left alone and the run uses no cache, a problem. Before: "ignoring X:
    not a cache file" and X replaced by an empty cache (a JPEG, a note), and
    `--clear-cache --cache X` deleted any X. `cache::clear` deletes the
    files beside a cache only when they begin `IMGFP`.
  - **B** no output may name the `--from-file` list, nor stdin's file when
    `-` reads it (`outputs_spare_the_lists`): `--log-file list.txt
    --from-file list.txt` emptied the list before it was read and scanned
    the log's header lines as paths.
  - **C** a cut-off file is analysed as the part it holds
    (`decode::trim_fill`): edge rows / columns of the working plane that
    are the decoders' grey fill (128/255, within half a level) are cut away
    and the size shown cut in proportion. A JPEG cut at 85% was KEEP beside
    its original (0.84) and `new` against a library holding it; one cut at
    half paired with nothing. Now DELETE at 1.00 / 0.99, `repeat`.
  - **D** an icon of several entries has `Traits::frames`, and is lossless
    only as the entry read is (a palette PNG, a <= 8-bit bitmap is not): a
    16/32/48/256 icon was "same pixels" as a PNG of its 256 entry, DELETE.
  - **E** `--last` names the folders its scan was of; `--help` and README
    say a scan run with `--cache` is written again with the same `--cache`
    (each cache keeps its own, by design: a sweep's own `--cache` keeps its
    scans out of the window's). Before, `--last` after a `--cache X` scan
    wrote a day-old scan of other folders, said only as "1 day ago".
  - **F** the default folder's caches of other formats are removed
    (`cache::remove_other_formats`, user's decision; was kept for good, 2.5
    GB of IMGFPC15 here): only files named as a default cache, holding a
    cache header, and not held by a running scan (`flock` exclusive, not
    waited for). Two versions installed side by side each start afresh
    after the other ran.
  - **G** the byte pass compares a same-size set side by side
    (`same::byte_classes`), each file read once: six identical 30 MB files
    were 480 MB read (whole hash, then the first file again per copy), now
    180 MB. A set larger than `most_open` (`COMPARE_BYTES` 2 MB over
    `LEAST_PIECE` 128 KB = 16, and a worker's share of half the open-file
    limit) is hashed first and compared in batches beside its first file.
  - **H** `-t` above the cores runs on the cores, with a note
    (`img_fp::threads`): 10 images at `-t 5000` took 215 s (0.25 s at 8),
    a cached 1,033-file folder 25% slower at `-t 32`. The window's limits
    are what a value can mean: threads the cores, aligned points `FEATURES`
    (600), candidates `index::MOST_IMAGES` (were 1,024, 200, 2,000).
  - **I** an empty `--against` library says why: its images are all in a
    folder scanned (`img-fp . --against .`), or it has subfolders and no
    `-r`, or it holds no image (was always the `-r` hint).
- **IMGS5** `/home/daniel/Documents/IMGS5` (held out until 0.54.0, now read
  once): 60 seeds in 25 near-miss sets, 5,400 variants. 0.53.0 and 0.54.0
  alike: F1 0.9131, precision 85.5%, recall 97.97 / 97.94%, **11 seed pairs
  merged** (8 past 300 pairs: Mural, bird, Fountain3/5, Puerto3/4,
  Floriana3/4, Macizo5/6, Floriana1/2, Tree1/2). Every one is joined by
  *clean* cross-family anchors, which join clusters unasked. Judged by eye:
  bird1/2 (a burst) and Floriana1/2, 3/4 (a portrait shot of the landscape
  from the same spot; an original agrees with the other's crop at
  `blk_min` 0.96, 358-389 clean links) are too near and may merge; Mural
  (a person added), Puerto3/4, Macizo5/6, Tree1/2 and Parish Church are
  real merges through 2-6 clean links, nearly all from `crop_micro`,
  `embed_tiny`, `crop_strip_top` variants where the differing part is cut
  away or too small to read; Fountain3/5 (a closer shot, 73 links) between.
  The bridge test stops one such link, not two. **Open**, and IMGS5 is now
  development data: the next rule needs fresh seeds again.
- Per-corpus at 512 (0.51.0, `out/v36-cliffs`): IMGS 0.9719, IMGS2 0.9846,
  IMGS3 0.9871, IMGS4 0.9860; at 640: 0.9764, 0.9867, 0.9885, 0.9882.
- Cost: ~2,500 CPU-s / ~6 min cold on four-corpus IMGS-ALL (SSCD ~55,000); on
  five corpora 3,751 CPU-s / 590 s (0.75.0, SSCD 71,057).
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
- **Default `--min-pixel-correlation` 0.8: tried and reverted to 0.7**
  (2026-10-29, both the user's decisions). Raised because on 50,000 DISC21
  references (unrelated YFCC photos, `~/Documents/DISC21/m50k`) the pairs
  the user judged false by eye were nearly all below 0.8 (of 175 group
  members 172 at >= 0.7, 83 at >= 0.8). Cost, cold, 0.76.0
  (`~/Documents/DISC21/fixtest`): IMGS-ALL F1 0.9739 -> 0.9531, precision
  98.85 -> 98.99%, recall 95.98 -> 91.91%, trap FPs 3,075 -> 1,988,
  cross-family 12,227 -> 10,908, merges 53 -> 28; IMGS-ALL2 F1 0.9390 ->
  0.8971, recall 89.60 -> 82.21%: four times the 0.6 -> 0.7 step's recall
  cost. Reverted ("it's worth it") on DISC21 itself, 25k dev queries
  against 25k references (`~/Documents/DISC21/p25`, 126 true copies in
  reach), ranked by aligned points: 0.8 finds 65 with 13 false, µAP 0.494;
  0.7 finds 80 with 23 false, µAP 0.607, recall at 90% precision 39.7 ->
  51.6%. Of the 23 false at >= 0.7, by eye with the claimed region drawn
  (page https://claude.ai/artifact/LHEcySeL46hfUWE2v2NVsL): 18 another frame
  of one person's video, an urn that may be an unlisted copy, two dashcam
  frames of one road, a logo the query holds, and **2 different pictures**
  (orange streaks on a train photo's rails; two horizons). Neither of the
  2 is a code bug: plausible similarities resting on line-like structure,
  which points and blocks cannot tell apart. Open.
- **DISC21 dev, whole** (2026-10-29/30, `~/Documents/DISC21`; record in
  `benchmark/BASELINE.md`): 50,000 dev queries against 1,000,000 references
  (10,000 true copies), the build after 0.76.0 with the sliver bound and
  copied leaves, run as 20 shards (`loop.sh`: all queries `--against` one
  50k zip each, `--min-pixel-correlation 0.5 --dump`, zip deleted after),
  each query-reference pair's best (points, correlation) over shards,
  ranked by aligned points (correlation / 2 breaks ties; ties either way
  65.2-65.4), `score_shards.py`, checked with `score_disc.py` (0.6531).
  Bars read off the 0.5 run (a real 0.7 run on `p25` agreed 102 / 103):
  0.5 **µAP 65.3**, recall 71.3%, recall at 90% precision 51.7%; 0.6 64.7 /
  55.2%; 0.7 (default) 59.9 / 54.1%; 0.8 50.1 / 49.0%. SSCD's published dev
  µAP (percent, as the paper writes it): ResNet50 61.5, large 63.7; with
  score normalization 72.5, 75.3. Ranking by correlation is worse at every
  bar. Caveats: up to 150 candidates per shard, not overall; ranking and
  bar chosen on dev; img-fp uses no training data (SSCD trains on DISC21's
  training set); no normalization. Cost 9.1 h wall, 37.6 CPU-h, 24-36 min a
  shard, peak under 1.7 GB. **Score normalization** (SSCD's: a query's
  similarity less β times its mean similarity to its 1st-3rd nearest
  neighbours in DISC21's training set, β 1), simulated offline with the
  other dev queries as the background (no two queries copy one reference,
  so no true copy is in it; `norm/score_norm.py`): points less β x the mean
  of the query's 1-3 best query-query verdicts, any correlation, gives µAP
  66.0-66.7 and recall at 90% precision 54.4-56.8% over β 0.5-1, first
  neighbour or 1st-3rd, background bar 0.2 or 0.5 (bar 0.7: no gain).
  About +1.3, not SSCD's +11: an unrelated pair rarely gives img-fp a
  verdict at all (46,185 queries have a query neighbour, mostly few points),
  so most biases are 0; it lowers queries with many look-alikes (video
  frames, textures). Not run against a real training-set background.
- **DISC21 at `--work-size 1024`** (2026-10-30, `~/Documents/DISC21/strain`,
  `w1024_0`, `w1024_1`): strain test, all 50k queries against zips 0 + 1
  (100k references, 972 true copies) in **one run of 150,000 files**:
  exit 0, 1 h 20 min, 30,400 CPU-s, 81.2 M descriptors, quantised in 160 s,
  peak RssAnon+swap 2.6 GB (MemAvailable never under 1.85 GB of 5.8),
  cache 10.2 GB, disk +23.8 GB at the peak. Accuracy is **worse than 512**,
  and it is the work size, not the sharding: at bar 0.5, 512 as two shards
  µAP 65.1 (692 true, 972 false), 1024 as two shards 54.7 (674 true, 2,672
  false), 1024 as one run 56.0 (655 true); at 0.6 65.0 / 59.2 / 59.1, at 0.7
  58.2 / 54.7 / 53.5. At 1024 unrelated pairs reach many more aligned
  points (false pairs with >= 40 points: 196 at 512, 764 at 1024; true 605
  / 593), mostly at correlation under 0.7, so they rank among the true
  copies. Two shards at 1024 from the cache: 23 min each. On the generated
  corpora (`benchmark/out/v55-worksize`, same build, 512 from the fixtest
  caches, 1024 cold into them; accuracy only, no ABBA cost): IMGS-ALL F1
  0.9737 -> 0.9745 (recall 95.94 -> 96.07%, perfect 53 -> 49), IMGS-ALL2
  0.9390 -> **0.9316** (recall 89.59 -> 88.22%, perfect 11 -> 9).
  Containment gains (IMGS-ALL2 `embed_tiny` 344 -> 357, `contact_sheet` 354
  -> 362, `crop_micro` 281 -> 291), small copies lose: IMGS-ALL2
  `thumbnail` 352 -> 240, `qr_sticker` 357 -> 193, `rot90`/`rot180` -7/-8.
  **Two causes, both a constant read at 512** (measured after 0.76.0,
  both ties reverted with the ladder, below; the code never shipped):
  (1) `MAX_SCALE` 16 bounds the factor between working planes, but its own
  reason is a part's pixels (512 / 16 = 32): a 53 x 33 thumbnail of a
  940-px photo is 18-fold at 1024 (29 points, 0.83, overlap 1.0) and was
  refused; small `rot90`/`rot180` copies the same. Tried:
  `Policy::for_work_size`, `16 x max(work, 512) / 512` (0 keeps 16).
  (2) `FEATURES` 600 at any work size: at native 640 px a QR sticker's
  modules took the features and the photo with the sticker gave no verdict
  at all against the photo (1,200 features: 273 points, 0.78). Tried:
  `lib::features_for`, `600 x work / 512`, at least 600, at most
  `index::MOST_KEYPOINTS` (2,048, an entry's 11 bits), 2,048 at work 0.
  At 512 and below both are as before: IMGS-ALL and IMGS-ALL2 reports
  byte-identical. At 1024, IMGS-ALL2 (512 / 1024 old / bound tied / both):
  F1 0.9390 / 0.9316 / 0.9339 / **0.9395**, recall 89.59 / 88.22 / 88.63 /
  89.79%, precision 98.63 / 98.68 / 98.68 / 98.51%; `thumbnail` 352 / 240 /
  358 / 358, `qr_sticker` 357 / 193 / 193 / 356, `embed_tiny` 344 / 357 /
  357 / 361, `crop_micro` 281 / 291 / 291 / 298; but **Segovia1/2 119 ->
  573 and Floriana1/2 178 -> 570 false pairs** (past 300; cross-family
  13,086 -> 14,386). IMGS-ALL: 0.9737 / 0.9745 / 0.9745 / **0.9756**
  (recall 96.31%), same five merges. Cost of the features at 1024,
  IMGS-ALL2 cold ABBA (600 / 1,200 / 1,200 / 600): 3,990 / 4,854 / 4,860 /
  4,249 CPU-s, +18%, wall 8:53 / 10:46 / 10:46 / 9:21, peak RSS 3.0 ->
  3.9 GB (runs started at 75-78 C, idle 67: the temperature wait misread a
  comma decimal). At **2048** (2,048 features, bound 64; cached caches,
  one run each): IMGS-ALL F1 0.9765, precision 98.90%, recall 96.43%,
  four merges past 300 (Mural1/2 91), 9,715 CPU-s, 26 min, peak RSS 4.2
  GB, 37.7 M descriptors; IMGS-ALL2 F1 0.9385, precision 98.32%, recall
  89.76%, cross-family 16,713 (Floriana1/2 3,011, Mural1/2 333, Floriana3/4
  304 past 300; Segovia1/2 back to 128), 7,356 CPU-s, 18 min, 4.4 GB.
  So recall flattens past 1024 and precision falls on IMGS-ALL2.
  **DISC21 at 1024 with both** (`~/Documents/DISC21/w1024b_0`, `_1`, no
  `--dump`; zips 0 + 1, 972 true copies; `norm/cmp.py`): µAP at bar 0.5
  512 65.1 / 1024 old 54.7 / both **55.4** (true 692 / 674 / 681, false
  972 / 2,672 / 3,475); 0.6 65.0 / 59.2 / 61.3; 0.7 58.2 / 54.7 / 56.7.
  The fixes help little there; 1024 is still worse. 1:17 and 1:01 a shard
  (analysis at 1,200 features included), peak RSS 4.0 GB. Of zip 0's
  true copies 33 found at 512 are not stated at 1024 at all, losses even
  across query / reference size ratios (2-5% a band, so not scale
  mismatch); alone, such pairs verify at 1024 (Q05562 | R002092 101
  points, 0.926), so they are lost in retrieval. Suspect: the vocabulary
  sample is capped at 1.28 M whatever the descriptor count, so at 1,200
  features a word holds twice the descriptors. Untested (a `-k` sweep on
  the cached analyses would show it). False pairs: median 23 points
  against 20, 3.7 times as many.
- **The work-size ladder** (2026-10-31/11-01, scaffolding `IMGFP_LADDER`,
  uncommitted; `~/Documents/DISC21/ws`, runs `*T`, logs in `run5.out`).
  Requirement (user's): a larger `--work-size` adds true positives and
  loses none. Found on the way, each measured on DISC21 zip 0 at 1024 and
  dropped: a larger vocabulary sample alone (false pairs 1,753 -> 1,360,
  losses 33 -> 31), `-k 300` (8 of 33 back), 512's features kept with
  finer ones beside (33 -> 21), plus ranking on 512's words (-> 8), plus
  verifying on them (-> 5), plus 512's exact decode (-> 1, the rest claim
  graph). What holds, as built:
  - **Analysis**: rungs 64, 128, 256, 512, ... below the work size, then
    the work size. The lowest rung is described as a run at its size
    describes it; each next rung, from its own decode, adds only keypoints
    finer than the rung below can find (`sift::Params::finer_than`), up to
    a budget in proportion (`budget`: 600 x L / 512, at most
    `index::MOST_KEYPOINTS`). Every run's keypoints are in one frame, the
    picture's plane at 512, so a smaller run's are to the bit the leading
    keypoints of a larger run's; each carries its rung in the angle's low
    four bits (inferring rungs from scale failed on a cut-off file, whose
    decoded size changes with the rung: IMGS-ALL lost 632). Thumbnail
    from the 128 decode at every size; scale bound 16 (the frame no longer
    grows, so `for_work_size` is not needed).
  - **Matching**: each lower rung matched as a run at its size matches (its
    prefix, its own vocabulary and words), and the run's claims are its own
    plus every rung's. So a run at W states every pair a run at any rung
    below states.
  - **Containment, 512 -> 1024**, true pairs of 512 not stated at 1024:
    plain build 5,151 / 17,956 / 33 (IMGS-ALL / IMGS-ALL2 / DISC21 zip 0
    at 0.5); ladder **0 / 0 / 0**. Rung 512 inside the 1024 run matches the
    512 run to the count. (Bars read off one 0.5 run show "losses" at 0.6
    and 0.7: the pair is stated, but the report shows the top level's
    claim, of lower correlation.)
  - **Accuracy**: IMGS-ALL2 plain 512 0.9390 (P 98.63, R 89.59); ladder 512
    0.9359 (98.49, 89.15), ladder 1024 **0.9418** (98.38, **90.33**).
    DISC21 zip 0 at 0.5: plain 512 µAP 64.9 (374 true, 460 false); ladder
    512 63.6 (360, 300), ladder 1024 58.1 (371, 1,081). The ladder costs
    the default some recall (a rung's share of features is fixed, not the
    strongest 600) and precision (every smaller run's false pairs come
    along). CPU: ladder 512 ~1.8x plain, 1024 ~3.5x.
  - **Every size, one build** (`img-fp-tag`, `run21.sh`, `run22.sh`): true
    pairs lost against the size below, 64 -> 256 -> 512 -> 1024 -> 2048:
    **0 at every step on IMGS-ALL, IMGS-ALL2 and DISC21 zip 0** (bar 0.5).
    | size | IMGS-ALL F1 / P / R | CPU s | IMGS-ALL2 F1 / P / R | CPU s | DISC21 zip 0 true / false / µAP | CPU s |
    |---|---|---|---|---|---|---|
    | 64 | 0.3674 / 100.00 / 22.50 | 1,432 | 0.1885 / 100.00 / 10.41 | 777 | 25 / 0 / 4.7 | 1,966 |
    | 256 | 0.9403 / 98.68 / 89.80 | 3,136 | 0.8860 / 98.61 / 80.44 | 2,138 | 303 / 76 / 55.3 | 5,556 |
    | 512 | 0.9711 / 98.19 / 96.05 | 6,152 | 0.9359 / 98.49 / 89.15 | 4,430 | 360 / 300 / 63.6 | 13,734 |
    | 1024 | 0.9743 / 98.12 / 96.75 | 11,823 | 0.9418 / 98.38 / 90.33 | 8,309 | 371 / 1,081 / 58.1 | 30,610 |
    | 2048 | 0.9749 / 98.10 / 96.88 | 19,351 | 0.9434 / 98.32 / 90.67 | 13,693 | 371 / 1,081 / 58.1 | 45,151 |
    Plain build at 512 for comparison: IMGS-ALL 0.9737 / 98.84 / 95.94,
    IMGS-ALL2 0.9390 / 98.63 / 89.59, DISC21 374 / 460 / 64.9. DISC21's
    pictures are at most 1024 px, so 2048 adds no rung there: same pairs
    as 1024. Recall rises and cost about doubles with each step; precision
    falls slowly (each size states every smaller size's false pairs too).
    **Rejected** (user's decision, 2026-11-01): containment held, but it
    costs about twice the CPU at 512 and does worse there (IMGS-ALL2 recall
    -0.4, DISC21 µAP 64.9 -> 63.6), so it buys the knob by sacrificing the
    default. Scaffolding removed. Next: `--work-size` goes, and a knob
    whose higher steps cost nothing where they find nothing replaces it.
- **`--detail`** (2026-11-01/03, 0.77.0; replaces
  `--work-size`; scripts and runs in `~/Documents/DISC21/lv`, batches
  13-22). The user's brief: going up the knob, no step costs more than
  today's `--work-size` at that size for what it finds, a step that finds
  nothing costs nothing (not kept: see below), true pairs are only added, the default may change
  (it did not: option C, user's choice). As built:
  - **Default**: today's 512 exactly (`analyse_plain`, `match_all`).
    `--fast` is today's `--work-size 256`, outside the knob (a coarser base
    is the ladder, which cost the default). Both byte-identical to HEAD
    (8de63d5) but the report's `config` (`detail`, `fast` for `work_size`):
    IMGS-ALL and IMGS-ALL2 cached, IMGS-ALL2 `--dump`, IMGS-AGAINST
    `--against`, IMGS `--fast` against `--work-size 256` (`DISC21/final`).
  - **Analysis** (`analysis::finer_levels`, `features_at`): the 512
    analysis, then each doubling up to the detail the picture reaches (the
    picture itself where it stops between two), from the same decode
    (`reduce::ReduceLevels`), each adding only keypoints finer than the
    level below finds, up to `FEATURES / 4` a doubling (measured on the
    constant). The levels' ends are kept (`Features::ends`): a coarser
    detail's analysis is a prefix of a finer one's.
  - **Matching** (`pipeline::match_by_detail`): step 512 is the default's
    matching; each finer step adds the new descriptors' words to step 512's
    vocabulary (`index_adding`; retrained, 42 of the step's 96 s for recall
    +0.05), verifies only pairs holding a file with a new level
    (`Pass::fresh`), with the scale bound its planes allow (`level_bound`),
    takes earlier steps' verdicts as they stand (`Pass::known`), ranks each
    file less the pairs already decided in windows of `-k` / 15 (10), and
    asks no second look of its own. Claims: every step's, finest first,
    less a finer step's joins of two families the default holds apart (the
    veto: precision 98.72 -> 98.82%, cross-family 13,653 -> 12,420, recall
    96.46 -> 96.42%). A step no file reaches is skipped.
  - **Cost, cold ABBA against today's `--work-size` at the same size**
    (CPU-s; F1 / precision / recall):
    | | today | `--detail` | today | `--detail` |
    |---|---|---|---|---|
    | IMGS-ALL 1024 | 5,586 / 5,871 | **5,346 / 5,603** | 0.9745 / 98.86 / 96.07 | **0.9753** / 98.82 / **96.27** |
    | IMGS-ALL2 1024 | 4,326 / 4,352 | **3,929 / 4,082** | 0.9316 / 98.68 / 88.22 | **0.9415** / 98.62 / **90.06** |
    | IMGS-ALL 2048 | 7,217 / 7,180 | **5,380 / 5,461** | 0.9743 / 98.87 / 96.03 | **0.9757** / 98.82 / **96.36** |
    | IMGS-ALL2 2048 | 4,820 / 5,670 | **4,086 / 4,138** | 0.9302 / 98.70 / 87.96 | **0.9421** / 98.61 / **90.19** |
    Wall about level (2048: 17:51 / 17:12 against 18:10 / 17:50). Default
    512 for reference: 0.9739 / 98.85 / 95.98, 0.9390 / 98.63 / 89.60.
    Merges past 300: IMGS-ALL the same four; IMGS-ALL2 three either way
    (bird1/2 1,551 -> 5,740, Floriana1/2 out, Floriana3/4 in).
  - **Containment**: true pairs lost 512 -> 1024 -> 2048: **0, 0** on
    IMGS-ALL (+4,582, +1,174) and IMGS-ALL2 (+6,464, +1,863).
  - **DISC21 proxy** (5,529 queries, 10,529 references, bar 0.5 / 0.7
    µAP; one cold run each): default 73.0 / 60.6 (1,469 CPU-s), `--detail
    1024` 73.4 / 60.9 (2,595), today's `--work-size 1024` 67.4 / 58.2
    (3,198). Its pictures stop at 1024 px: 2048 and 4096 give the 1024
    report to the byte at its cost (2,369 / 2,393, 2,420 / 2,287).
  - **DISC21 dev, whole, at `--detail 1024`** (2026-11-03,
    `~/Documents/DISC21/d1k`, as the 512 run: 20 shards, bar 0.5,
    `--dump`; `d1k/score.py`, `d1k/contain.py`). Against the 512 shards
    of 2026-10-29/30 (0.76.0, a dumping run, so before the `--dump` fix):
    µAP at 0.5 / 0.6 / 0.7 **65.07 / 64.53 / 59.85** against 65.31 / 64.72
    / 59.94, true 7,123 against 7,132, false 10,131 against 10,205, recall
    at 90% precision 51.5 against 51.7%; **56.7 CPU-h against 33.0 (+72%)**,
    wall 12.5 h against 9.1, peak RSS 4.1 / 4.3 GB. A shard with the
    queries cached: 9,900-10,700 CPU-s against 6,000-6,300. Nearly every
    reference is ~1024 px, so every file gets the level and is verified
    again: on DISC21 1024 buys nothing for its cost. "Lost" 36 true pairs
    (16 shards), none verified at all in the 1024 run: they are the old
    build's. HEAD's default (8de63d5) on shard 5 misses the same 3 (375
    true against 378; 6,265 CPU-s, 21:22), and 1024 states exactly its 375
    (0 lost, 0 added; `head5/`). Shards 0 + 1: 512 65.03, 1024 65.32 (0
    lost, +4); the two as one 1024 run of 150,000 files 64.80 (682 true,
    749 false), 25,177 CPU-s, 1:07 wall (today's `--work-size 1024` in
    one run: 56.0, 30,400 CPU-s, 1:20).
  - **4096** (kept, user's decision: it may help on corpora of large
    pictures): 2048 -> 4096 loses 0, adds 188 / 393
    true pairs (F1 0.9758, 0.9423), CPU +1.5% / +4%, peak RSS level, but
    wall 17:00 -> 27:50 and 13:15 -> 22:15: a 4096 plane is ~0.7 GB to
    describe, so the decode budget runs few at once (analysis 849 -> 1,439
    s on IMGS-ALL at about the same CPU). 8192 would be held to 2,047
    features (`MOST_KEYPOINTS`), 16384 needs ~11 GB a plane.
  - **Tried and dropped on the way**: the finer step's own second look (66
    CPU-s cached on IMGS-ALL for recall +0.02); wider finer windows, cached
    CPU-s / recall with decided pairs kept 75: 1,289 / 96.36, 40: 1,239 /
    96.34, without the second look 75 / 40 / 20 / 10: 1,227 / 1,168 / 1,092
    / 1,052 (96.34 / 96.32 / 96.24 / 96.20), decided pairs left out 20:
    1,099 / 96.31, 10: 1,045 / 96.27 (kept; today's 1024 1,031-1,053);
    propagation only in changed clusters (29,992 of 33,119 files are in
    one at 1024: nothing saved); one grey row for all levels (kept, saved
    nothing measurable: the log's "cpu: decode" is wall per thread,
    waiting on the decode budget included, not CPU).
  - **"A step that finds nothing costs nothing" cannot be kept; dropped
    from the brief** (2026-11-09, after 0.84.1; `out/v60-detail-cost`: `gains.py`,
    `gains.out`, the run scripts).
    What holds, and what `--help` says: a step costs nothing on pictures
    too small to reach it, and a doubling no picture reaches is no step.
    Where pictures reach it, the cost is describing the finer level, not
    matching it: IMGS-ALL2 at 1024 cold 3,849 CPU-s, features 2,456
    thread-s, the finer matching step ~25 s wall of 610; DISC21 shard 5 the
    same shape (describing ~1,000 s wall, finer step ~120 of 2,167). Each
    level is already one octave, only keypoints finer than the level below.
    So the only saving left is not describing a picture's level when it
    will find nothing, and nothing known before describing it says that.
    Tested the best evidence there is, the 512 matching itself (IMGS-ALL2,
    512 and 1024 `--dump` from one cache): 1024 adds 6,316 true pairs (1
    lost, 277 false). 2,295 of them (36%; `embed_tiny` 640 of 927,
    `crop_micro` 339 of 632, `contact_sheet` 234 of 363) left no verdict at
    all at 512, even at the dump's 3 points, so no rule reading 512's
    verdicts can ask for them. And describing only the side a 512 verdict
    shows magnified (where a finer level could add detail) still describes
    16,552 of the 18,096 files that have a level (91.5%) and covers 5,599
    of the 6,316: -8% of the cost for -11% of the finds. 552 of the gains
    are between two files neither of which has a level (the finer step's
    other pairs moving the claim graph). Not built.
  - **Found on the way, fixed**: since 0.82.0 a run read a record against
    its own keypoint cap (`cache::misshapen` with the run's `Settings`), so
    every record a finer `--detail` made with more than the default's 600
    keypoints was "damage" to a run at the default and its file analysed
    again (IMGS-ALL2 at 512 after 1024: 10,519 "would not read", 10,536
    analysed, 1,627 CPU-s). Now each record is checked against the cap on
    its own row: 17 analysed (the uncacheable), 759 CPU-s; 512 and 1024
    reports identical to before. Test
    `a_record_a_finer_detail_made_is_read_by_the_default`.
- **Audit after 0.77.0** (A-H, uncommitted; no report changes but (A)'s
  folders of small pictures):
  - **A** a cache record is read at any `--detail` of its working size
    (`cache::Settings::reads_as`; the feature cap stays in the record for
    `misshapen`): cut to the run's levels where it holds more
    (`Item::cached_at`), analysed again where it holds fewer, one live record
    a file. Before, each detail kept its own set and a switch analysed every
    file again: 400 found-corpus pictures (224 px, no finer level) at 4096
    after the default 40.9 CPU-s, now 0.44, and the default's matching is
    taken. Desktop on one cache, 2048 / 512 / 1024 / 4096 / 512 / 2048:
    635 / 0 / 0 / 61 / 0 / 0 files analysed, each report identical to a cold
    run at its detail; 696 records (before: 1,270 after three details).
    The matching steps only through doublings some picture reaches
    (`detail_steps`; the kept matching's fingerprint is the finest of them),
    so a run reaching none is the default's run (it went through
    `match_by_detail`'s `merge_best`, a possible small difference), and a
    `--dump` is the finest running step's (a skipped top step used to be run
    for it).
  - **B** a `--detail` run's claims hold each pair once (were every step's,
    deduplicated only by `stated::unique`): kept matching on Desktop at 4096
    812 KB -> 204 KB (512: 201 KB), report identical.
  - **C** the bar's matching estimate is learned from `detail_steps` after the
    analysis (was `log2(detail / 512) + 1` steps whatever the pictures).
  - **D** stale `--work-size` text (log line, `--prune-cache` help, comments).
  - **E** `Planes::levels` keys a finer analysis by every plane, not the 512
    one alone (a picture and its pixel-doubled copy shared levels).
  - **F** `-k` windows: membership by a `Vec<bool>` (was `ask.contains`
    over a growing list, quadratic in the files extended).
  - **G** more files than `index::MOST_IMAGES` is refused before the
    analysis (was an assertion after it); postings past `u32` an error
    (`index::postings_fit`; the count wrapped).
  - **H** the window reads a pre-0.77.0 `work_size` as its Detail.
  - Proved: base (HEAD) vs new, cold `derived/Desktop` at 512 / 1024 / 2048 /
    4096, `--fast`, `--against` at 2048, `--dump` at 512 and 2048 (CSV
    byte-identical): all identical but `runtime_seconds`. Not run on the
    full corpora.
- **0.78.0, audit after 0.77.1** (A-G; scratch runs under the session
  scratchpad `c/`, not in `out/`):
  - **A** the whole-frame check (`whole_worst`) blurred each side at its own
    quarter-octave step (`Blurs::get`), and a footprint on a step's midpoint
    went either way on a fitted scale's last digits. Every 9:16 portrait (a
    phone's screenshots: thumbnail 83 px wide, footprint -2.4998 steps) hit
    it: a q70 or q80 re-encode of a q92 screenshot, fitted at scale 0.9999,
    read 12.9 grey levels (0.1 right), over `WHOLE_RESIDUAL`, so it was not
    deleted for it (REVIEW), and `--against` a library holding the original
    called it `better`. Now `d`'s step is rounded and `k`'s is it plus the
    footprints' ratio (`Blurs::step`): one scale, one blur. Test
    `the_whole_frame_check_blurs_both_sides_of_one_scale_alike` (16.2 before).
  - **B** byte copies (`Side::proven`: the same bytes or one JPEG coding)
    left open keep one, the one carrying most besides its pixels: in a class
    of two pictures nothing decides between, every file was REVIEW, a backup
    copy too (photo + its copy + a re-encode or a PNG of it: three REVIEWs;
    now the copy is DELETE). Test `a_byte_copy_beside_an_undecided_pair_is_deleted`.
  - **C** `WHOLE_RESIDUAL` (12) swept after A, kept by the user's decision:
    a ramp with no cliff and no noise floor to derive it from; the numbers
    are on the constant. pairs.py loss 4 / 8 / 12 / 16 / 24: 44 / 51 / 57 /
    61 / 66, variants deleted 3,745 / 5,494 / 6,177 / 6,440 / 6,602; IMGS-ALL
    none lost at 2-30. **Open**: faithful crops, turns and resizes read
    5-37 there (resampling), same-frame re-encodes p99 2.1; a check that
    reads a faithful crop as low as a re-encode would let the bar be derived.
  - **D** the report's `config.features` is `features_at` the run's detail
    (said 600 at every `--detail`), and `MOST_ALIGNED_POINTS` (the window's
    Min aligned points limit) is the finest detail's, 1,650 (was 600).
  - **E** a followed link to nothing is a skip, `Note::BrokenLink`, that
    still stops `--prune-cache` (`walk_was_complete`): one link to an
    unplugged drive made every scan of a library exit 2. A root named
    outright that leads nowhere is still a problem.
  - **F** `-k` is held to `MOST_CANDIDATES` as parsed (the window's limit
    already): `from + 2 * w` wrapped near `usize::MAX`.
  - **G** `--exclude` takes patterns (`walk::Pattern`): an entry holding
    `*`, `?` or `[` that names no path; without `/`, of a name met below a
    root, with one, of a path (absolute against the current folder, its
    plain lead canonical, `**` across names), matched against the canonical
    path and every folder above it. A pattern was an unresolved path (exit 2,
    nothing excluded). A plain name without a wildcard is still a path.
  - Measured (IMGS-ALL, the audit's build against HEAD, one cache): all
    1,346,632 pairs and 915 groups identical (A-G leave the matching alone);
    0 lost both, 358 / 364,
    364 / 364, strict line 55 -> 52, DELETE 19,919 -> 19,915. pairs.py loss
    54 -> 57, variants deleted 6,184 -> 6,177: A ends the accidental refusals
    that hid three other rules' errors, `phone_screenshot` (IMGS `16.jpg`, the
    9:16 screenshot itself, deleted for its phone-screenshot composite: the
    canvas rule), `wall_poster` (`low-light2`, 12 points) and `scale_mid`
    (`16.jpg` deleted for its downscale: `extra`, as `scale_large`'s 8). Not
    fixed here. `derived/Desktop` of IMGS / IMGS2 / IMGS3: 0 / 2 / 1 actions
    moved. Tests: lib 229 (plain and v3 builds), window 44.
- **0.78.1, audit after 0.78.0** (A-E; no change to pairs or suggestions,
  not run on the corpora):
  - **A** `--exclude` patterns match in linear time (`walk::stars_match`,
    back only to the last `*` or `**`): every split point at every star was
    exponential when nothing matched (`*a*a*a*a*a*a*a*a*a*a*b` against a
    60-byte name ran past 60 s, once a file met; now 0.05 s). Held to the
    old recursive matcher on 300,000 random cases
    (`a_pattern_matches_as_trying_every_split_would`).
  - **B** the window's permanent delete (`delete_instead`) says how many
    files rest on a kept copy deleted with them, or just moved to the Trash:
    the Trash dialog had warned of a marked kept copy but called it
    restorable.
  - **C** `Move::stays` runs off the GTK thread (`Move::looked_at`,
    `gio::spawn_blocking`): two `stat`s a marked file, a round trip each on a
    share, held the window still while it moved them.
  - **D** `rust-version` 1.92 (gtk4 0.11, glib and cairo-rs 0.22 need it;
    was 1.88, which only the CLI builds with); two `last.rs` test lines bound
    a temporary that 1.88 drops (E0716). Checked: 1.92 `check --all-targets
    --features gui`, 1.88 `check --all-targets --ignore-rust-version`.
  - **E** `same::open_file_limit`: `RLIMIT_NOFILE` per architecture (5 on
    Linux MIPS, 6 on SPARC, 7 elsewhere on Linux, 8 on the BSDs and macOS)
    and `rlim_t` a `c_ulong` on Linux (32 bits on a 32-bit build).
- **0.79.0, audit after 0.78.1** (A-F; only D changes pairs):
  - **A** the keep rule was cubic in a class of same-frame copies:
    `Component::refusal` counted `pictures_of_one_frame` (quadratic in the
    class) before the cheap `!e.exact` test, once a pair as deletion spread.
    Byte copies of one picture, 1,000 / 2,000 / 3,000: 0.9 / 8.8 / 37 s;
    600 re-encodes 5.0 s in the stage. Now the test first and the count
    stops at three (all it is asked).
  - **B** what is left is the N(N-1)/2 pairs N copies are stated as (3,000
    copies: 4.5 M pairs, ~1 GB RSS, a 1.65 GB JSON). Made cheaper without
    changing a byte: `Component::new` read `frame_any(f, g)` by binary search
    for the side it was holding (most of 10 s), and the report's pairs were
    sorted by comparing two path strings a step (`pipeline::name_ranks`).
    3,000 copies 55 -> 16 s wall, 600 re-encodes 10.2 -> 6.1 s; reports of
    both and of six small folders identical. **Open** (user's call: the
    format): stating an identical set once would make it linear.
  - **C** the window's exclude list takes patterns typed by hand (`PathList::
    with_patterns`, label `PATTERN`): a pattern of names as typed, a path
    whole or `~/` (`walk::typed_exclude`: the worker has no current folder,
    so `photos/**/raw` would resolve against wherever the window started);
    a bare name with no wildcard is refused, as on the command line it is a
    path. Checked under Xvfb: refusal shown, `~/` expanded, `skip_*` kept in
    `gui.json` and its file left out of the scan.
  - **D** the pixel check takes each side to the blunter one's resolution
    (`pixel_check_with`: footprint times the other side's shortfall below a
    pixel a cell). Its comment had said so since 888f90f; the code filtered
    each side over its own footprint only. Cached, base = HEAD: IMGS-ALL F1
    0.9737 -> 0.9748 (P 98.84 -> 98.83%, R 95.94 -> 96.17%, traps +42,
    cross-family +98; past 300 five -> four: Mural1/2 329 -> 252, the
    other four as they were), `crop_micro` 306 -> 322, `contact_sheet` 349 -> 352,
    `embed_tiny` 353 -> 351; IMGS-ALL2 0.9390 -> 0.9415 (98.63 -> 98.62%,
    89.59 -> 90.06%, same three past 300), every transform that moved up
    (`crop_micro` 281 -> 291, `perspective` 285 -> 289). Keep rule: 0 lost
    both; IMGS-ALL 358 / 364, 364 / 364, strict line 52 both, DELETE +15;
    IMGS-ALL2 351 / 364, 364 / 364, strict line 38 -> 39 (not traced).
    `families.py` joined 65.0 -> 65.3 (one n1 draw), 63.0 -> 63.0. Not run:
    `pairs.py`, cliffs. **Not fixed by it**: a 48 px thumbnail of a 700 px
    photo still agrees at 0.87 (KEEP beside it, `new` against a library
    holding it): both sides hold a pixel a cell there, and what differs is
    the small file's own resize and JPEG at its pixel scale. A floor of two
    pixels would read past it, but it is a new constant.
  - **E** a compaction's copy (`<stem>.tmp.<pid>`) and the heads, matched
    and same files' temporaries left by a run killed outright (SIGKILL, the
    OOM killer; the window kills a cancelled scan after 3 s) were never
    removed. `cache::sweep_litter`, asked by `compact` once it holds the
    cache alone, removes them: with no other holder each is a dead run's,
    which a pid test could not say for a cache two machines share. A copy
    of a stem another cache in the folder shares is left.
  - **F** no warnings on aarch64 Linux and macOS (`cargo check --target
    ... --all-targets`): `POST_LINES` x86-only, `verify.rs`'s `last` / `x`,
    a test's `Thumb` import.
- **0.80.0, audit after 0.79.0** (A-D; matching format IMGFPM02, the
  pairs unchanged; scratch runs under the session scratchpad `fix/`):
  - **A** *holds* had a flat corner tolerance, 2% of the frame + 3 px
    (`TOL`, `TOL_PX`, unmeasured), also subtracted in the keeper's "least
    left out". A crop missing 1-2% of one side was one frame with its
    original, kept as the smaller file and the original deleted (default
    rule; `--suggest representative`; `--against`: a lossless original a
    `repeat` of a q90 library crop missing up to 2.2%). Measured instead
    (`suggest::FIT_ERROR`): on IMGS-ALL2's deleting direct verdicts between
    files of one frame (131,267 pairs, `--dump` of 2026-11-02), how far
    the fitted corners land outside the other frame times the pair's aligned
    points is flat in the points (p99 0.61-0.71 from 3 to 200+ points) and in
    file size (0.60-0.75): p50 0.18, p99 0.68, p99.9 1.14, max 4.60. One bar
    at p99 (0.7 / points) lost IMGS-ALL 10 families' last lossless copy:
    the 1% of one-frame pairs past it read as a frame a hair larger, which
    may delete as a crop and is never asked the one-frame rules (`own_pixels`:
    a lossy HEIF deleted its 500x667 lossless WebP, 335 points, 1.3 working
    px out). Swept with one bar, IMGS-ALL original-or-lossless / strict line:
    base 355 / 75, 0.7 345 / 28, 1.14 350 / 54, 2.0 356 / 87. So two bars:
    inside at `FIT_ERROR` 0.7 / points (p99), larger only past
    `FIT_ERROR_MOST` 4.6 / points (the max), neither deleting the other
    between (`Side::f_past`, read by `strict_in`, `more`, `Component::new`'s
    composite test). A propagated pair has no points of its own: its
    hops' 1 / n add (`Verdict::composed_points`, `claims::propagate`).
    A pair in the band whose files each show the other over the whole frame
    (`whole_worst` both ways) is undecided, both REVIEW, as two pictures of
    one frame are: without it turned copies (their fits are ~1.7x a plain
    copy's: p99 1.06-1.15 x points against 0.65, a third of `rot180` in the
    band) and `chromatic_aberration` went from REVIEW to KEEP (`pairs.py`
    `rot180` REVIEW 693 -> 118); without the whole-frame condition,
    composites matched at few points, where the band is wide (4.6 / 15 =
    30%), went from KEEP to REVIEW (IMGS-ALL `caption_bar` 279,
    `phone_screenshot` 176, `browser_window` 150, `column_roll` 97).
  - **B** `CROPS` (two crops each missing <= 10% the other has: one frame,
    on its comment "15% lost twice the composites") is gone. A left crop was
    deleted for its right twin and a re-encode of it, losing a strip up to
    a tenth, in 3 of 8 cases, and `--against` called it a `repeat` in 7 of 12.
  - **C** `at_distance` held distances below a pixel at one: past 8x,
    `extra` compared an enlargement at 8 of its pixels with its original at
    one, not at 8 / s, so a 12-14x upscale (JPEG or PNG, Lanczos or
    bicubic, up to 10000 x 11257) was kept and its original deleted on
    every seed tried. Below a pixel it is now the interpolation's, in
    proportion to the distance. **Not fixed**: a lossless Lanczos PNG
    upscale at 3-8x still beats its q92 original on 4 of 15 seeds: its
    ringing reads above `EXTRA_PER_OCTAVE`, which sits under the
    enlargements' p99 on purpose (raising it deletes real originals for
    their downscale). No content is lost; the larger file is kept.
  - **D** the group head among files of equal reach ranked every KEEP
    first, including one kept only for agreeing under `REVIEW` with the
    rest (an edit too far to be a copy): a copy with a sticker over part of
    the picture (0.88) headed its original and watermarked copy, and
    `--suggest representative` deleted both. Now `suggest::Head`: a kept
    copy, then the file an undecided class would keep (`choose`), then a
    REVIEW, then a file kept for being unlike, then a DELETE. Left: with no
    pair between them strong enough to settle, the original and its
    watermarked copy tie, and the larger (the q95 copy) heads.
  - Docs: featureless files match only byte copies (`same_analysis` passes
    them over); `--last` writes paths absolute; README's DELETE no longer
    says "apart perhaps from a thin strip". `suggest_score.py` also prints
    losses with exact regions (its `holds` had the same 2% slack, and was
    blind to A).
  - Measured, base (HEAD) against new, cached, one run each:
    IMGS-ALL: lost 0 / 0 (2% slack / exact regions) both; strict line 75
    -> 54; original or lossless copy 355 -> 356 / 364; faithful 364 both;
    picture only in an edit or composite 4 -> 4; KEEP 8,767 -> 9,004,
    DELETE 19,938 -> 19,757, REVIEW 3,831 -> 3,775 (`rotate_small`,
    `print_scan`, `crop_slight` to DELETE, read as the crops of the
    original they are, were one undecided frame; warps and
    `chromatic_aberration`, whose affine fit puts corners further out than
    a copy's, deleted less). Heads that are the original or a lossless copy
    16 -> 20 of 901 groups (measured before the band's REVIEW rule).
    IMGS-ALL2: lost 0 / 0 both; strict line 68 -> 50; original or lossless
    351 -> 355; picture only in an edit or composite 5 -> 3; KEEP 10,896 ->
    10,894, DELETE 15,401 -> 15,241, REVIEW 5,897 -> 6,059. `pairs.py`
    (now also `lossX`, exact regions): loss 59 -> 27 (lossX 59 -> 27),
    originals deleted 365 -> 304, variants deleted 6,056 -> 6,087, REVIEW
    35,719 -> 35,540. The base figures here (355, 75; pairs loss 59) differ
    from 0.79.0's entry (358, 52; 57); not traced. Not run:
    IMGS-PROVENANCE, cost (ABBA). `Edge` is 4 bytes larger.
- **0.81.0, the keypoint-size bar** (`verify::SIZE_AGREEMENT`, one
  octave; cache format IMGFPC21; `out/v56-seed`, RESULTS sections 6-9). Fresh seeds
  first: INRIA Holidays (1,491 photos, 500 scenes) split by scene into
  **HOLI-DEV** (`~/Documents/HOLI-DEV`, 814 photos, 263 scenes) and
  **HOLI-TEST** (677, 237; held out, read once below), 12 generated variants
  a photo (`~/Documents/FRESH/generate.sh`); `families.py --corpus hdev |
  htest` reads a scene as a family and counts siblings and unrelated joins
  apart. On HOLI-DEV, photos alone, 0.80.0 joined 3 pairs of different
  scenes, each one photo shrunk 3-15x into 1-3% of the other with inliers
  2.0-3.2 octaves off the transform's scale (see *How it works*). The bar:
  HOLI-DEV different-scene joins 3 -> 0, siblings 968.3 -> 968.7; IMGS-ALL,
  IMGS5 small families identical, IMGS-ALL2 57.0 -> 56.7. Cached, against
  0.80.0: IMGS-ALL F1 0.9721 both (TP -143, FP +1), IMGS-ALL2 0.9343 ->
  0.9342 (TP +2, FP +39 traps), cross-family and merges identical; keep rule
  0 lost both, 356 / 364 and 355 / 364 unchanged, DELETE 19,757 -> 19,757 and
  15,241 -> 15,243; `pairs.py` identical but `contact_sheet` REVIEW 301 ->
  299. Cost, cached IMGS-ALL2 (ABBA, 60 s apart, no temperature wait): 0.80.0
  663 / 634 CPU-s, new 656 / 660. **HOLI-TEST, once**: 0.80.0 joins 1 pair of
  different scenes on photos alone (a harbour and an alpine valley, mirrored,
  37 points, scale 7.6, 0.756), the bar 0; siblings 713.3 -> 711.0 (pair
  precision n1 52.55 -> 52.82%, n2 60.85 -> 60.84%). The released build
  reproduces the measured switch-on runs byte for byte (IMGS-ALL, IMGS-ALL2).
  **Every bar's step** (cliffs, cached, `run11.sh`, `cliff_table.out`): no
  merge added anywhere; one gone (IMGS-ALL2 red_rocks_sign1/2, 318-350, at
  correlation 0.5-0.3); points, overlap and `-k` steps F1 within 0.0001; at
  low correlation FP -1,154 (IMGS-ALL, 0.2) / -2,006 (IMGS-ALL2, 0.2), TP
  +2,090 / +3,039. `--detail` 1024 / 2048 on IMGS-ALL2: F1 0.9439 / 0.9445
  both builds; containment from one cache 512 -> 1024 loses 1 true pair
  (`DSC48489.png` | `received_48203.jpg`, not looked at), adds 6,338; 1024
  -> 2048 0. **DISC21 shard 0** (50k dev queries against `references_0`,
  529 true copies, bar 0.5 run with `--dump`, `disc/score.py`): no true copy
  lost at any correlation step 0.2-0.9; false 424 -> 329 at 0.5 (µAP 64.3 ->
  65.7, R@P90 49.5 -> 52.9), 85 -> 81 at 0.7 (56.4 both), 11,447 -> 8,748
  at 0.2; dumped verdicts 493 k -> 299 k. Proxy at 0.5: false 80 -> 47, µAP
  72.7 -> 73.3; at 0.7 identical. Cold cost ABBA on IMGS-ALL (3,132 / 3,415
  base, 3,428 / 3,529 new) not readable: idle 84.5 C, the base runs 283
  apart; cached matching (above) level.
  **Cache format bumped to IMGFPC21 for the analysis, not the bar**: this
  session's baseline read IMGS-ALL F1 0.9721 / IMGS-ALL2 0.9343 on the
  `DISC21/fixtest` caches (filled by 0.76.0), not 0.9748 / 0.9415 as
  recorded. The same build analysing IMGS-ALL2 cold gives 0.9414 (recall
  90.06%, 88.78% cached), ~43,600 true pairs apart (30,689 cold only,
  12,960 cached only). The analysis changed after 0.76.0 under one format
  (0.77.0's "byte-identical at 512" was proved on cached corpora, which
  cannot see it); which build changed it was not traced. Every cached A/B
  of this session (cliffs included) read those caches on both sides, so
  its differences stand, but its absolute figures are the old records'.
  **Measure on caches the measured build filled.**
  Also measured and not shipped: a pair's verdict made independent of the
  folder by seeding correspondences from a fixed hash of each descriptor
  instead of the scan's vocabulary (`IMGFP_SEED`, 14 bits x 3 tables): direct
  verdicts the same alone and among 9,285 found photos (HOLI-DEV 240 / 249
  identical evidence, 146 / 219 with words), recall +0.1 on IMGS-ALL2, +50%
  cached matching CPU; and counting inliers one to one (rejected, *How it
  works*). Scaffolding in `out/v56-seed/scaffolding.patch`.
- **The cache by content** (after 0.81.0, uncommitted; schema
  `IMGFPD01`; see *The analysis and the cache*; scratch runs under the
  session scratchpad `runs/`). User's decisions: SQLite for the index, the
  analysis version computed from its sources, BLAKE3 equality as "identical",
  a 30-day grace for gone files; and the new cache as fast or faster to read
  and write and no larger. Done: `thumb.rs` (`Thumb` and its builder out of
  `verify.rs`), the analysis's constants into `analysis.rs`, `IMGFP_ANALYSIS`
  and the boundary test; `decode::Decoded::made_by` (libheif's version) and
  `hash`; `cache.rs` rewritten (index + zstd packs); `same.rs`'s byte pass is
  `content_hashes` + `exact_groups` by hash (sample hash, byte compare,
  `most_open`, `open_file_limit` gone); `.same` and `.heads` gone;
  `matched::Fingerprint` by hash. Code lines (no comments, blanks or tests)
  in the files touched 6,278 -> 5,603 (`cache.rs` 1,609 -> 1,121).
  Proved: reports identical to HEAD (less `runtime_seconds`) on Desktop
  (cold without a cache, cold filling one, the rescan taking the matching,
  a rescan reading every record at another bar), the found corpus (the
  same four) and IMGS-ALL (the last three); all 90 `pairs.py` reports;
  `--against` IMGS-AGAINST cold and cached (JSON, text, `--last --list`);
  `--dump` cold and cached (CSV byte-identical); `--last` txt / csv / json /
  `--suggest correlation`. Size: Desktop 33.50 -> 32.64 MB, found 642.9 ->
  625.6 MB, IMGS-ALL 1.939 -> 1.896 GB (-2.2 to -2.7%). Speed: found
  corpus cold 1,209 -> 1,142 CPU-s (zstd packs 3.2x faster than deflate);
  a rescan reading every record 9.8-12.0 -> 4.8-5.1 CPU-s whole run;
  IMGS-ALL cache read, three ABBA rounds of a rescan reading every record
  (warm, cold, warm): base 31.0 / 27.1, 52.1 / 32.1, 50.6 / 39.6 s, new
  32.8 / 28.9, 37.9 / 32.9, 43.0 / 45.0 s (means 38.8 / 36.8; the stage is
  mostly writing the analyses into the scratch file, `spill.rs`, the same in
  both), whole rescan 100-102 -> 79-80 CPU-s; identical files on a rescan
  0.28-1.17 -> 0.04 s (hashes from the index, nothing read). glibc floor of
  the CLI built in manylinux_2_28 still 2.28 (SQLite and zstd included); its
  report identical. Binary 9.2 -> 12.0 MB (SQLite, zstd). Seen on the way, not this change's:
  rustc 1.99 warns on `lib.rs`'s `check_cpu` `write` declaration (`*const
  u8` for `*const c_void`). Found on the way and fixed: a
  zstd context made per stream (the read was slower than base until each
  thread kept one), a 1 MB buffer zeroed per file hashed, the index read a
  page at a time through SQLite's 2 MB cache (`COUNT(*)` 127 ms -> 35 ms),
  a cold run hashing every file up front (each file read twice once the
  corpus outgrew memory; now only files that may be copies or the index
  may know by size are, the rest hashed by their decode), a fold copying
  the main pack into a side pack, and a commit per 64 rows that was an
  fsync (rollback journal) made under the append lock: found corpus cold
  ABBA 144.5 / 166.9 / 167.1 / 161.8 s (base / new / new / base), the
  commits 7.9 s and workers waiting behind them 32 thread-s; with a
  write-ahead log and the commit after the lock 3.4 s and 0.5 s, and cold
  ABBA 162.0 / 161.2 / 159.3 / 163.0 s wall, 1,104 / 1,097 / 1,093 / 1,115
  CPU-s. Behaviour, end to end: a renamed folder
  of 636 analyses 0; chmod + `touch` of 50 files analyses 0 and takes the
  kept matching; six same-size BMPs `touch`ed together, scanned as `sub1`
  then `sub2`, then together: no pair (0.69.0 reported "same pixels");
  `kill -9` mid-analysis keeps all but the uncommitted batch (576 / 635;
  the old cache kept every appended record: a SIGKILL lost up to 63
  analyses, Ctrl-C and SIGTERM none).
  **Then, the costs taken back** (same session): (a) the analysis version
  hashes only the crates the analysis calls and their dependencies, the
  target and its CPU features (was the whole `Cargo.lock`, `rustc -vV` and
  every flag: a dependency bump anywhere or a new stable compiler would have
  analysed every user's library again); rustc 1.92 and 1.99 builds write
  Desktop's packs byte for byte alike at `-t 1`. (b) A row is committed as
  soon as the index is free (`Db::commit_waiting`), not 64 at a time: Desktop
  `kill -9`ed after 250 described kept 259; found corpus cold, eight runs
  BAAB + ABBA, wall 154.0 / 160.0 / 162.1 / 159.7 s base, 159.0 / 160.6 /
  158.3 / 159.1 new (means 158.95 / 159.25), CPU 1,089 / 1,077 mean. (c) A
  damaged index is replaced (at open; later, by `finish` when alone), was a
  failure every run. (d) On a share: no mapping, and the mode is tested
  (`a_cache_on_a_share_keeps_and_reads_as_a_local_one`, forced); not run on
  a real share (no sshfs here). (e) A file whose size is shared is sampled
  (16 KB first, middle, last) and read whole only when its sample is shared
  too: identical-files stage on a cold found corpus 0.62 -> 0.17 s (base
  0.17), each of IMGS-ALL's 3,552 size-sharing files (0.67 GB) was read twice.
  (f) Cross checks build no C: `LIBSQLITE3_SYS_USE_PKG_CONFIG=1
  ZSTD_SYS_USE_PKG_CONFIG=1 PKG_CONFIG_ALLOW_CROSS=1 cargo check --target
  aarch64-unknown-linux-gnu --all-targets`, and for `aarch64-apple-darwin`
  also `CARGO_FEATURE_PURE=1` (blake3's NEON C); both clean. Reports of
  Desktop (cold, cold filling, rescan, other bar) and the found corpus cold
  identical to HEAD's; renamed folder 0 analysed, chmod + `touch` 0 analysed
  and the matching taken. Remeasured after (a)-(f), base = HEAD: IMGS-ALL
  reports identical (cold filling, rescan, other bar), cache 1.939 -> 1.896
  GB; IMGS-ALL cold, six runs base / every-commit / 64 MB checkpoint
  interval / same / every-commit / base: 625.0 (features 2,250 thread-s, the
  machine slow) / 476.4 / 473.6 / 486.4 / 477.5 / 473.4 s, describing 341-353
  s each but the first base's 475: a lone pair once read 487 / 587 s, which
  was the machine, and the checkpoint interval changes nothing (kept at
  SQLite's default). IMGS-ALL cache read, three ABBA rounds (warm, cold,
  warm): base 45.7 / 66.2, 46.5 / 49.8, 40.1 / 40.5 s, new 38.4 / 48.0,
  35.7 / 41.7, 54.6 / 38.8 (means 48.1 / 42.9), whole run 100 -> 77 CPU-s.
  Found corpus rescans, ABBA x4: wall 4.16 / 3.94 s mean, CPU 11.7 / 7.0,
  read stage 2.95 / 3.31 s (a new run right after a base one read slower,
  4.4-4.8 s, the second of a pair 2.2-2.4; the two caches do not fit in
  memory together). Left: a cache of 0.81.0 is not read (analysed
  again once), and the binary is 2.8 MB larger.
- **0.83.0, the geometric look** (`retrieve::Index::deeper`, see *How it
  works*; `out/v58-reach`, RESULTS sections 1-7). Point (3) of the user's
  brief: in a large folder a true copy ranks past `-k` and is never verified
  (DISC21 shard 0: 91 of 529 true copies, ranks 150-82,000). A file the
  second look re-asks reads its ranking to `GEOMETRIC_DEPTH` (1,000) and
  verifies, best first, the candidates whose weak-geometric-consistency peak
  beats the best peak among its own first `-k` and is at least 3. No budget
  constant (the null is the file's own); the depth swept on two DISC21
  shards (gain flat from 1,000 to 3,000; half of it at ranks 150-300).
  Point (1), seeds made independent of the folder, was tried as fixed
  seeding and rejected (*Tried and rejected*). Measured, the shipped build
  against HEAD (0.82.0) on caches HEAD's build filled: DISC21 shard 0 true
  copies at 0.5 / 0.7 374 / 312 -> **382 / 318** (µAP 65.86 -> 66.89, 56.66
  -> 57.62; false 380 -> 393, 87 -> 88), shard 1 318 / 280 -> **323 / 283**
  (µAP 66.49 -> 67.37, 60.28 -> 60.86; false 396 -> 403, 93 -> 95), **none
  lost** at either bar; 95-96 k deeper candidates a shard. IMGS-ALL and
  IMGS-ALL2: F1, precision, recall, pairs and merges identical (1,349,971
  and 1,282,289 pairs; 405 and 363 deeper candidates verified, from 1,354
  and 2,024 re-askers); keep rule identical on both; `pairs.py` identical
  (loss 26); small families identical but HOLI-DEV photos alone, one more
  sibling pair (968.7 -> 969.7, no different-scene join); HOLI-TEST
  identical (711.0); IMGS-AGAINST identical; `--detail` 512 / 1024 on IMGS
  identical to HEAD (0 lost 512 -> 1024). Cost: found corpus, cached, cooled
  ABBA **+16.5% CPU** (317 / 326 -> 367 / 382; nearly every file there is
  re-asked, and scoring 850 candidates a file is the cost, ~6 µs each,
  memory-bound); DISC21 cached single runs +9% / +7% (2,623 -> 2,856, 2,686
  -> 2,883 CPU-s), peak RSS 3.35 -> 3.45 GB (each file's `Shape`); IMGS-ALL
  / IMGS-ALL2 cached single runs 940 -> 886, 770 -> 758 CPU-s (within the
  runs' noise: few files there are re-asked). The `Shape`s are made only
  when the second look runs (not under `--skip-mirrored`), and not for
  `--detail`'s finer steps, which ask no second look.
- **0.84.0, the family's representative** (`Scope::re_askers`, see *How
  it works*; `out/v59-reask`, scaffolding `IMGFP_REASK` removed). Point (4) of the
  brief: the second look was asked only by files with fewer than two
  anchors, so whether a picture's mirror was looked for depended on the
  folder. **MIRFAM** (`~/Documents/MIRFAM`, truth `MIRFAM-truth.csv`,
  `make_mirfam.py`): each IMGS-ALL seed as o, o_q80, o_s70 and a turned
  family m, m_q78, m_s60 (flip_h / flip_v / rot180 by hash; rot180 is
  found directly). Turned families linked to their seed, of 364: HEAD 187
  (99 of 276 real mirrors), every matched file asking 364, the library
  rule (one asker per clean-anchor component, any answer) 363, shipped
  (component of any anchors, representative keeps clean verdicts) 358.
  Against HEAD (0.83.0), cached, the library rule / shipped: IMGS-ALL F1
  0.9748 -> 0.9734 / **0.9749** (recall 96.17 -> 96.30 / 96.18%; library
  rule bird1/2 851 -> 6,839); IMGS-ALL2 0.9414 -> 0.9401 / **0.9417**
  (90.06 -> 90.24 / 90.11%; library rule Mural1/2 187 -> 6,775); found
  corpus pairs +474 / -166 (231 new pairs, photos of one butterfly
  species) / identical. Cost, cooled ABBA: library rule found +2.6%,
  IMGS-ALL +26%, IMGS-ALL2 +37% (askers 1,354 -> 5,923, 2,024 -> 10,005);
  shipped found 362 / 346 vs 348 / 349, IMGS-ALL 950 / 818 vs 819 / 828,
  IMGS-ALL2 781 / 700 vs 713 / 709 CPU-s: level (askers +162, +229, +86).
  Keep rule 0 lost both corpora; originals kept IMGS-ALL 358 / 364 all,
  IMGS-ALL2 350 -> 349 / 351; `pairs.py` identical (26); small families
  identical; HOLI-DEV 969.7 -> 973.0 / 969.7; HOLI-TEST 711.0 -> 716.7 /
  712.3 (no different-scene join either way); `--detail` 512 -> 1024
  containment library rule 1 true pair lost, shipped 0. Against a library
  the shipped build is HEAD's rule: IMGS-AGAINST and DISC21 shard 0
  reports identical to HEAD's; per-family there lost one mirrored true copy
  a shard (0.51, 0.67 correlation; false 393 -> 383, 403 -> 385), hence
  the split. Cliffs (every step, IMGS-ALL and IMGS-ALL2, `bat/cliffs`):
  shipped as HEAD at every points, overlap and `-k` step but IMGS-ALL2
  overlap 0.5 (Acueducto3/4 236 -> 306), and at IMGS-ALL2 correlation
  0.6-0.2 Segovia1/2 merges (334-455), Henares1/2 from 0.4 (324-337),
  Mural1/2 out at 0.6; library rule Mural1/2 at points 6-3, game2/3 at 0.6,
  **bust2 | Classroom1 at 0.3-0.2** (402, 4,557). `-k` 10 recall IMGS-ALL2
  83.5 -> 87.4%. The shipped build reproduces the scaffolding's per-family
  runs byte for byte (MIRFAM, found, IMGS-ALL, IMGS-ALL2, `--detail` 512 /
  1024 on IMGS); tests pass. After 0.84.0, the bar's bound on the second
  look (`Settling`) leaves out each family's representative: at the end of
  verification it is the askers to the file (IMGS 313, IMGS3 403) and over
  their descriptors by 3% / 1.5% (it keeps each family's largest file);
  0.84.0 counted the representatives as settled, one asker a family low.
- **`--dump` changed the pairs, 0.73.0 to 0.76.0** (fixed in 0.77.0): `-k`
  counting rejections credited a window with every verdict in `all_direct` (`stood`), and a dumping run keeps verdicts the
  run's bars reject (three points, overlap 0.2), so it asked other windows.
  IMGS-ALL2 cached, with and without `--dump`: 2,087 / 2,028 of 1,275,506
  pairs differed at 512, 12,779 / 12,547 at 1024. Now only verdicts that
  stand are credited: with and without `--dump` identical on IMGS-ALL and
  IMGS-ALL2 at 512 and IMGS-ALL2 at 1024, and plain runs identical to
  before (so released reports were never affected). Every `--dump` figure
  from 0.73.0 on is a dumping run's own: the DISC21 shards above,
  replays off a dump.
- **Squashed transforms are no claim** (0.77.0;
  `verify::Policy::frames`, `verify::axis_scales`): `MAX_SCALE` bounds each
  direction of a transform (its singular values), not their mean. Before,
  only √|det| was bounded, and a fit through nearly collinear inliers (a
  wave crest, a horizon) is free across their line: DISC21 R007131 (a
  yacht on calm sea) was put into R030945 (surf) as a sliver forty times
  longer than wide (directions 0.44 and 0.0105, mean 0.068), mirrored and
  inverted, 61 aligned points, overlap 1.00, correlation 0.775 (its blocks
  mostly flat, abstaining). That scan's four slivers had one direction at
  0.006-0.011 or 87; every other transform was within 1/16-16 in each
  direction. No new constant: the bound's own reason (past sixteen-fold a
  part is ~30 px of the analysis) holds per direction. IMGS-ALL at 0.8:
  2 pairs of 1,287,146 gone (same seed: `scale_small`|`upscale`,
  `crop_micro`|`scale_small`), 9 added, F1 0.9531 -> 0.9532; IMGS-ALL2:
  F1 0.8971 both, one cross-family pair fewer. Tests
  `axis_scales_are_the_directions_least_and_most`, `a_sliver_is_no_claim`.
- **The vocabulary's leaves are copies** (0.77.0;
  `index::Vocabulary::levels`): they were pointers into the analysis's
  scratch file (`spill.rs`), saving 120 bytes a leaf, but the sample is
  spread over every picture, so the descent read the whole mapping at
  random. DISC21's dev queries against one reference zip (100,000 files,
  45 M descriptors, 5.8 GB on a 5.8 GB machine) sat in quantising for 90
  minutes at ~80% I/O wait and was killed. Copies are at most `SAMPLE_CAP`
  x 128 = 164 MB. IMGS-ALL report byte-identical; quantising 21.6 -> 17.1 s.
  The same 100,000 files, cached: quantised in 130 s. At 50,000 (2.9 GB of
  descriptors, which fit) both builds quantise in 72 s. On DISC21's 50,000
  references with the sliver bound as well: the four slivers gone, 94 pairs
  in 78 groups, 18.5 min.

## Cliffs before 0.74.0 (from *Parameters*)

Cliffs at 0.73.0's defaults: see its *Status* entry (IMGS-ALL merges docks1/2
at overlap 0.5 and below, Acueducto3/4 at 0.25, red_rocks_sign1/2 at
correlation 0.3 and below, Henares1/2 at 0.2; none elsewhere).
Cliffs at 0.72.0's defaults (the one-sided flat block counted), every step
of every bar, IMGS-ALL and IMGS5 (`out/v44-flat-contrast`; 0.71.0 at the
same steps in `out/v42-flat-cliffs`): IMGS-ALL merges nothing at points
10-3, overlap 0.85-0.25 (Antoniego1/2 165-170 false pairs at 0.5 and
below), correlation 0.9-0.2 (cross-family 461 at 0.2, largest 78; 43 at
0.8, largest 39) or `-k` 150-10. IMGS5 merges 5 at the default and points
8-5, 6 at points 4-3 (Mural1/2 back), 7 at overlap 0.75-0.3 (Puerto3/4,
Macizo5/6 back), 8 at 0.25, 5 at every correlation 0.9-0.2, 5 at `-k` 100,
4 at 50 and 25, 2 at 10. The table below is from before 0.72.0, on the four corpora and
IMGS-ALL: the 512 column at 0.54.0's default (correlation 0.7; session
scratch, not in `out/`), the 640 column at 0.51.0 with correlation 0.6
(`out/v36-cliffs/cliffs.txt`; 0.44.0's in `out/v31-cost-cliff`). At 0.7,
512: points 3 is at most 66 cross-family pairs (IMGS3 `docks`, 62); overlap
0.5 merges `docks` on IMGS-ALL (5,702), IMGS3 alone reaches 168
(`Antoniego`) and IMGS, IMGS2, IMGS4 stay at 0-1 down to 0.4; correlation
below 0.7 is the 0.51.0 sweep (no merge 0.2-0.6).

| option | 512 (default) | 640 |
|---|---|---|
| `--min-aligned-points` 10 | none down to 3 | none down to 3 (0.44.0: `bust` at 7) |
| `--min-frame-overlap` 0.85 | 0.35 (`docks` at 0.5) | 0.25 (`docks` at 0.6) |
| `--min-pixel-correlation` 0.7 | none 0.2-0.8 | 0.3 below (`game` at 0.4, IMGS3) |
| `-k` 150 | none from 10 up | none |

`bust` no longer merges at 640 on any corpus, and `Henares` no longer joins
`docks` at 512 overlap 0.3. Nearest misses (no seed pair >= 300): IMGS 640
correlation 0.2, `product-white-bg`|`sunflowers` 261; IMGS-ALL 640 overlap
<= 0.5, `Antoniego` 258-262. `-k` below 25 measures the propagation budget
(clusters get starred), not the candidate list. Every remaining merge is two
photographs of one scene.

## Facts moved from *Status*

- Shipped `--min-pixel-correlation` is 0.7 (since 0.54.0; kept at 0.72.0,
  when a block flat in one picture only began to count: see its entry), chosen for
  what users want grouped, not for F1: 0.5 grouped merely similar photos,
  and 0.6 paired five of IMGS5's sixty seeds with their near-miss sibling
  (direct, 0.63-0.70: GanyutingStore, Kaper-12/13, Lizard, Los Gigantes,
  Macizo1/2). Cost at 512: recall -1.3 to -1.7 points on every corpus
  (IMGS-ALL 96.90 -> 95.45%, F1 0.9822 -> 0.9748; mostly `tiled_watermark`
  286 -> 216 of 304, warps), precision 99.57 -> 99.60%, cross-family 7 -> 4,
  keep rule unchanged (0 lost, 297 / 304, 304 / 304; `pairs.py` loss 24,
  REVIEW +25). It does not stop IMGS5's merges (11 at 0.6-0.75, 10 at 0.8):
  those go through clean anchors, which agree at 0.9 a block.
- The working size is 512 (`Args::work_size`; 256 under `--fast`), one
  step below the knee at 640. `--work-size` is gone (see *`--detail`*
  above): `--detail 1024 | 2048 | 4096` adds finer levels above the analysis at
  512 and matches them in steps, stating everything the default states;
  `--fast` is a separate, cheaper analysis outside that knob.
- Precision: on IMGS every false pair is a rearrangement trap. On the
  near-miss corpora stray single files join a sibling photograph's family
  (the clean-anchor rule's lone-file allowance). **Watch merges, not pairs.**
- Open: no downscaled JPEG decode path (worth ~4%); nothing tested above
  ~30k images; on the found corpus about half of sampled reported pairs are
  different specimens of one species, leaking through the propagated tier;
  5 of the 7 families not keeping their original are grey or near-grey
  originals beaten by a tinted copy (`temperature`) on "colour over grey".
