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
