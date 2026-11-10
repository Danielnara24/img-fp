# v61: where the acceptance stack's errors enter (2026-11-09, after 0.84.1)

Offline, on IMGS-ALL2 at 512 (`--dump`, build 4d56d07; runs in the session
scratchpad) and its small families (`families.py --corpus all2`, each subset
dumped). Scripts here: `where.py`, `negative.py`, `oneway.py`, `sim.py`,
`famwhere.py`; outputs `*.out`.

1. **Full corpus: the graph does the precision.** Direct anchors are 6.8%
   cross-family (45,983 of 673k; all near-miss siblings: Mural, Fountain3/5,
   bird, Henares, game, bust...); the report keeps 7,799 of them. Corroborating
   29,347 -> 1,032. `where-all2.out`.
2. **Negative evidence does not separate.** Verdicts whose geometry stands
   (10 points, centred, overlap) and whose pixels disagree (blk < 0.5): sibling
   sets 2-40% of their positives, real families 2.2% (89% of those true pairs
   the pixels missed: documents, manholes, UI). Floriana1/2 2.3%, sea_photo
   1.8%. `negative-all2.out`.
3. **Two-way clean links are nearly pure one by one** (5 cross of 121k,
   0.004%); one-way anchors are 9.5% cross. `oneway-all2.out`.
4. **But "same picture" as their closure fails**: the 5 errors merge 15
   classes; with parts attached P 67.8%, counting only in-class pairs and
   direct links P 95.41% / R 94.08% / F1 0.9474 and 28 merges, against the
   shipped 98.61% / 90.11% / 0.9417 and 3. A closure multiplies one error by a
   family; the shipped rules (bridge, cover, cycle, shared parts) are what hold
   that back. `sim-all2.out`.
5. **Small families: the single verdict decides, and every false join is an
   anchor that is not clean.** Clean direct 1,608 true / 3 cross; anchor
   two-way 1,038 / 72, one-way 780 / 239 (originals alone: bust, Henares,
   Segovia, Fountain3/5, Mural, bird, game, Lizard, Puerto1/2, sea_photo...).
   Their blk 0.70-0.97, blk_min mostly 0.005-0.35 (bird 0.73, Segovia 0.65,
   Fountain 0.54). Anchors that are not clean carry ~45% of small-family recall
   (warps, photo_of_screen, vignette, stickers), so a bar on them is a recall
   trade, as CLAUDE.md already records for blk_min. `famwhere-all2.out`,
   `families-all2.out` (total joined 63.0).

6. **A: the block map holds no separator** (scaffolding `scaffolding.patch`:
   `--dump` writes each verdict's 36 block correlations and kinds, re-run
   from the thumbnails; reproduces `blk` / `blk_min` on direct verdicts, 13 of
   3,630 off by ~0.001; turned verdicts are checked against the turned
   picture and do not reproduce, so left out). IMGS-ALL2 small families,
   direct non-clean anchors: 1,776 true, 293 cross. The hypothesis was that
   a sibling fails in one compact region and a warp diffusely; it is the
   other way round: siblings fail in more blocks and more separate regions
   (median low0.5 4 vs 1, ncomp0.5 2 vs 1). Every feature, cross removed with
   95% / 99% of true kept: `blk` 32% / 11% (best), `med` 25 / 4, `q25` 24 / 2,
   `frac0.7` 27 / 9, `ncomp0.5` 14 / 4, `blk_min` 6 / 1, largest low region
   7 / 0, its compactness 9 / 2. Two at once (blk plus each) no better: 31% /
   10% at most. So what separates is the mean itself, i.e. a stricter
   `--min-pixel-correlation`, which was measured and reverted (0.8: recall
   95.98 -> 91.91% on IMGS-ALL, user's decision). The true anchors that look
   most like siblings are the truth's local edits: `qr_sticker`,
   `redaction_bars`, `blur_patch` (median 4-6 connected low blocks, blk_min
   0.04-0.08), and warps (`pincushion`, `swirl`, `barrel_distort`,
   `photo_of_screen`). `blocks.py`, `blocks-fam-all2.out`.

7. **Decision (user, 2026-11-09): sibling joins are acceptable as long as a
   higher `--min-pixel-correlation` removes them.** `score.py` now counts
   false pairs between two seeds of one near-miss set apart (`sibFP`) and
   gives precision without them (`P-unrel`); F1 unchanged. IMGS-ALL2 at
   0.7: 17,621 FP, 13,425 of them siblings, P-unrel 99.66%.
8. **Does a higher bar remove every sibling join? Yes, but some only near
   1.** Shipped build (4d56d07), IMGS-ALL2 (`sweep-all2.out`): small
   families (7 subsets) sibling sets 29 / 18 / 15 / 11 / 8 / 6 / 2 / 0 at
   0.7 / 0.75 / 0.8 / 0.85 / 0.9 / 0.95 / 0.98 / 0.99 (recall 91.8 -> 32.2%);
   full corpus 27 / 23 / 20 / 11 / 6 / 3 / 1 at 0.7 ... 0.99 / 0.995 (recall
   90.1 -> 9.0%). Most sets go by 0.75-0.95. Last: Floriana1/2 and 3/4 at
   0.99 / 0.995 (a crop of one is pixel for pixel part of the other, same
   spot: one-way clean links at 0.98-0.991), Fountain3/5 at 0.99, bird1/2
   one pair at 0.995 (two `blur_fill` copies' blurred backgrounds, 0.998).
   A join between clusters always faces the full bar (clean or anchor at
   `blk >= bar`); it is the pairs *inside* a joined cluster that do not:
   corroboration takes `bar - CLUSTER_SLACK_CORRELATION` (0.1). So at 0.98
   Fountain3/5 kept 2,185 false pairs at median correlation 0.944, made
   inside a cluster 24 one-way clean links at 0.981-0.991 had joined
   (`swd`, dump at 0.98).
9. **The bar is not a floor on what is stated.** Shipped, 0.7: 39,792 of
   1,283,132 stated pairs (3.1%; 36,831 true) correlate below it; 0.98:
   212,810 of 570,121 (37%). With the slack at 0 (`sweep-noslack-all2.out`)
   none does, and the full corpus's sibling FP at 0.98 is 160 (shipped
   2,499); but recall at 0.7 is 90.11 -> 87.44% (TP 1,247,528 -> 1,210,558;
   precision 98.6 -> 98.7%), small families 91.76 -> 91.40%. CLAUDE.md
   had the slack at "~0.5 recall"; on IMGS-ALL2 now it is 2.7 points. Not
   changed: user's decision.

10. **The hard floor, built** (`floor.patch`: `CLUSTER_SLACK_CORRELATION`
    gone, a corroborating verdict needs `blk >= bar` like every other; test
    `nothing_stands_below_the_correlation_bar`). Base = 4d56d07's build, new
    = the patch, IMGS-ALL cold into the scratch cache then both from it.
    IMGS-ALL2 identical to the scratch no-slack build (pairs and groups).
    - **Floor holds**: pairs stated below 0.7, IMGS-ALL 29,562 of 1,350,087
      -> 0 of 1,320,402; IMGS-ALL2 39,792 -> 0.
    - **IMGS-ALL**: F1 0.9749 -> 0.9650, precision 98.83 -> 98.92%, recall
      96.18 -> 94.20% (TP -27,108), traps 3,207 -> 2,383, cross-family 12,334
      -> 11,659, precision without sibling pairs 99.75 -> 99.81%, merges 59
      -> 59 (Fountain3/5 6,793 -> 6,635, Floriana1/2 3,042 -> 2,928, bird1/2
      853 -> 673, Mural1/2 252 -> 180), perfect transforms 53 -> 50. Losses:
      `tiled_watermark` 241 -> 169 / 364, `barrel_distort` 349 -> 283,
      `keystone_side` 347 -> 313, `perspective_top` 349 -> 318,
      `redaction_bars` 356 -> 328, `wave_vertical` 312 -> 292,
      `photo_of_screen` 359 -> 349; no transform gains.
    - **IMGS-ALL2**: F1 0.942 -> 0.927, precision 98.6 -> 98.7%, recall
      90.11 -> 87.44%.
    - **Small families** (`families.py compare`): unrelated joins 0 both;
      IMGS-ALL joined 65.3 both (n1 P 89.22 -> 90.26%, R 95.63 -> 95.40%),
      IMGS-ALL2 63.0 both (n2 P 89.81 -> 90.59%, R 91.30 -> 90.82%), HOLI-DEV
      969.7 -> 926.3 (n1 P 50.74 -> 52.66%, R 97.46 -> 97.42%).
    - **Keep rule**: identical. IMGS-ALL 0 lost, 358 / 364, 364 / 364, strict
      line 64, DELETE +5; IMGS-ALL2 351 / 364, 364 / 364, the one
      `phone_screenshot` loss in both; `pairs.py` identical to the total (loss
      26, originals deleted 344, REVIEW 35,229).
    - Not run: cliffs at other bars than the sweep's, IMGS-PROVENANCE, DISC21,
      cost (no stage changes; fewer pairs stated).
