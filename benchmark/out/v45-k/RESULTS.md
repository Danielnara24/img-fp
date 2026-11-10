# v45-k: is -k cherry-picking the corpus? (0.72.0, 2026-10-25)

All runs cached (v40-keep/cache.bin; found-corpus mixes on fcache.bin, a copy of
v44's found cache plus the originals). Lists in lists/ (mk.py): every seed's
original + n random variants (draws d1-d3), or + m random found-corpus photos.
Reports r/<list>[@<flag>].json; scored by table2.py (truth = analyse.py's,
restricted to the list). "seedp" = seed pairs with any cross-family pair, "orig"
= seed pairs whose two ORIGINALS are paired directly. bars-i5.txt, bars-all.txt:
every sweep. d/: dumps. sep.py: blk / blk_min of sibling vs true verdicts.

## Answer: -k is not the shield; the vocabulary is

- Full IMGS5 at -k 150 / 300 / 600 / 1500 / 5460: identical from 300 up, and within 41 pairs of 150
  (i5-k*.json). Candidate lists are "files sharing a word"; at 5,460 files they
  are shorter than 300. Small families (n <= 2): flat from -k 50 up on IMGS5,
  flat 10-400 on IMGS-ALL. The "merges fall as -k falls" rows are starvation
  (i5-n30 k10: Fountain3/5 and Mural1/2 vanish). One sign of fit: i5-n30 at
  k250 adds bird1/2 (851 pairs).
- The shield: a sibling pair's pixel agreement is the same in every regime, its
  descriptor matches are not. Puerto1/2 n_match 489 (seeds alone) / 300 (n=2) /
  125 (n=10) / 47 (n=30) / 19 (full), blk 0.71-0.73 throughout; Shenzhen,
  Seagull, Cemetery3/4 not met at all in the full run (not even in --dump).
  Correspondences are sought only within shared words, and a vocabulary trained
  on ~90 copies of each seed is finest exactly around each seed's descriptors.
- Size alone does not do it: originals among m unrelated found photos pair as
  many siblings as alone. IMGS5 m = 0/300/1000/3000/9285: 8 / 9-10 / 9 / 6-7 / 8;
  IMGS-ALL: 6 / 6-8 / 7 / 6-7 / 6. A real library is the seeds-alone regime.

## Sibling originals paired directly, by family size (default bars)

| copies a seed | IMGS5 orig (seedp) | IMGS5 P% | IMGS-ALL orig (seedp) | IMGS-ALL P% |
|---|---|---|---|---|
| 0 (seeds) | 8 (8) | - | 6 (6) | - |
| 1 | 7-8 (9-10) | 64-68 | 6-8 (6-8) | 90.6-93.4 |
| 2 | 7-11 (9-13) | 66-71 | 5-9 (5-11) | 91.1-96.8 |
| 5 | 3-7 (7-11) | 79-86 | 3-6 (4-6) | 96.0-97.4 |
| 10 | 4-5 (7-9) | 84-85 | 2-3 (3-4) | 97.6-98.5 |
| 30 | 2-3 (7-9) | 89-91 | 0 (3-5) | 99.7-99.8 |
| 90 (full) | 2 (12) | 90.84 | 0 (9) | 99.78 |

Recall does not move with family size (IMGS-ALL n=1-2 93.3-96.6%, full 94.56%).
Of ~295 within-set seed pairs in IMGS-ALL (~86 in IMGS5; sets by name), the
default joins 5-11 (7-13) at 1-2 copies a photo.

## Bars where families are small (bars-*.txt)

- Correlation is the only bar that separates siblings. IMGS-ALL seeds alone at
  0.5/0.6/0.65/0.7/0.75/0.8/0.9: 18/13/9/6/5/4/2 sibling pairs; IMGS5
  16/13/12/8/7/6/2. Left at 0.9: Segovia1/2 and game2/3; bird1/2 and Fountain3/5.
- Overlap: IMGS5 seeds alone 8 -> 14 at 0.75 (one step), 15 at 0.6-0.4 (a
  portrait from the landscape's spot is a crop of it); IMGS-ALL 6 -> 7 / 8 / 10.
- Points 10 -> 3: no sibling pair added at n <= 2 on either corpus (siblings
  have 15-330 inliers), IMGS-ALL n=1 recall 95.56 -> 96.93%. The bar guarded
  siblings only where the vocabulary had already thinned their matches.
- blk_min overlaps: 18-20% of true verdicts at n=2 (warps, photo_of_screen,
  vignette, caption_bar...) are at or below Segovia1/2's 0.70.

## Keep rule

No DELETE whose kept copy is another photograph in any IMGS-ALL subset. IMGS5:
2-8 rows at every family size (bird1/2 nearly all; Floriana, Puerto3/4,
Cemetery3/4), 123 rows in the full IMGS5 run, never scored before.
