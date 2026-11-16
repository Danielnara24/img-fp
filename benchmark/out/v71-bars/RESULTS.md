# v71: the three user bars, each swept alone (2026-11-14)

img-fp 0.85.0 (plain build, `img-fp-head`), cached real runs (no dump replay):
IMGS-ALL, IMGS-ALL2, IMGS-ALL3 on a copy of `v67-keep`'s cache, DISC21 shard 0
on `cache57`. Each bar moved alone; the other two at their defaults
(correlation 0.7, overlap 0.85, points 10). `batch.sh`, `table.py` ->
`table.out` (full table), `dscore.py` (DISC21 from every stated pair, ranked
by points + correlation / 2). Defaults reproduce the baselines to the pair
(0.9749 / 0.9417 / 0.9213; DISC21 317 / 81 / 57.48 = the real 0.7 run).

Mean F1 is over the three corpora; DISC21 is true / false copies, µAP.

| setting | mean F1 | ALL | ALL2 | ALL3 | DISC21 | seed pairs past 300 (ALL/2/3) |
|---|---|---|---|---|---|---|
| corr 0.60 | 0.9622 | 0.9817 | 0.9604 | 0.9446 | 362 / 158 / 64.92 | 4 / 4 / 5 |
| corr 0.65 | 0.9565 | 0.9793 | 0.9531 | 0.9372 | 344 / 114 / 61.92 | 4 / 3 / 4 |
| **corr 0.70** | 0.9460 | 0.9749 | 0.9417 | 0.9213 | 317 / 81 / 57.48 | 4 / 3 / 4 |
| corr 0.75 | 0.9312 | 0.9680 | 0.9248 | 0.9008 | 300 / 59 / 54.77 | 4 / 3 / 5 |
| corr 0.80 | 0.9076 | 0.9554 | 0.9010 | 0.8663 | 266 / 40 / 48.90 | 4 / 4 / 3 |
| overlap 0.75 | 0.9421 | 0.9672 | 0.9368 | 0.9223 | 326 / 88 / 59.09 | 5 / 4 / 6 |
| overlap 0.80 | 0.9459 | 0.9742 | 0.9399 | 0.9237 | 322 / 87 / 58.36 | 4 / 4 / 4 |
| **overlap 0.85** | 0.9460 | | | | | |
| overlap 0.90 | 0.9415 | 0.9734 | 0.9383 | 0.9127 | 316 / 74 / 57.53 | 3 / 2 / 3 |
| points 3 | 0.9477 | 0.9758 | 0.9453 | 0.9219 | 321 / 87 / 58.08 | 4 / 3 / 4 |
| points 5 | 0.9476 | 0.9758 | 0.9451 | 0.9219 | 321 / 87 / 58.08 | 4 / 3 / 4 |
| points 7 | 0.9472 | 0.9757 | 0.9441 | 0.9217 | 319 / 87 / 57.78 | 4 / 3 / 4 |
| **points 10** | 0.9460 | | | | | |
| points 14 | 0.9439 | 0.9736 | 0.9377 | 0.9204 | 314 / 77 / 57.02 | 5 / 4 / 4 |

Readings:
- **Correlation**: monotone, lower is better on every corpus and on DISC21
  down to 0.6, with no new merge past 300. The cost is false pairs: ALL3 at
  0.6 has traps 3,625 -> 9,208 and cross-family 13,487 -> 21,538 (game2/3
  becomes a 6,633-pair merge, Mural1/2 6,993). The bar sits where it is for
  users (CLAUDE.md, *Defaults*), not F1, and these corpora cannot see
  siblings meeting; no change without `families.py`.
- **Overlap**: 0.85 is the peak (0.80 level in mean F1, +0.9 µAP, but
  cross-family 13,493 -> 21,615 on ALL2 and Floriana3/4 491 -> 3,771 on
  ALL). 0.90 drops the Fountain3/5 merge on IMGS-ALL (cross 12,334 -> 3,064)
  for -1 point of recall.
- **Points**: monotone and nearly flat; 3-5 are one point (mean F1 +0.0017,
  DISC21 +4 true / +6 false, µAP +0.60), traps and cross-family level
  (ALL2 traps +186), merges unchanged, perfect transforms up (ALL2 12 -> 17).
  The bar barely binds, as the null-model attempt found. The one candidate
  change; its precision has not been measured where siblings meet.

## Small families (`fam/`, `families.py`, 2026-11-14)

Total joined (seed pairs reported as one picture; n0 + mean n1 + mean n2 +
f), every join a sibling (same scene) on every corpus. `fam/compare.out`,
`fam/*.score` (`-v`: which pairs).

| | base | points 5 | points 3 | corr 0.65 |
|---|---|---|---|---|
| IMGS-ALL | 65.3 | 65.3 | 65.3 | 86.0 |
| IMGS-ALL2 | 63.0 | 63.3 | 63.3 | 85.3 |
| IMGS-ALL3 | 63.0 | 63.0 | 63.0 | 85.0 |
| IMGS5 | 40.7 | 40.7 | 40.7 | 51.7 |
| HOLI-DEV | 969.7 | 975.7 | 977.0 | 1,106.3 |

- **Correlation 0.65** joins a third more sibling photographs on every
  IMGS corpus (+21-22) and 14% more on HOLI-DEV (+137), for +0.7 points of
  small-family recall at best. 0.7 stays.
- **Points 5 / 3**: level on IMGS-ALL, IMGS-ALL3, IMGS5; IMGS-ALL2 +0.3
  (Macizo3/4 joined in one of three n2 draws); HOLI-DEV +6.0 / +7.3 (of
  ~970), mostly one n2 draw where Holidays group 1325, a burst of eleven
  views, joins whole. Small-family recall +0.1 to +0.9 points. No
  different-scene join anywhere.
