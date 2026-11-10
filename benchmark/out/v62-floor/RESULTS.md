# v62: the hard correlation floor against HEAD, every bar

Builds: `img-fp-head` (4d56d07), `img-fp-floor` (HEAD + `v61-acceptance/floor.patch`:
`CLUSTER_SLACK_CORRELATION` gone, a corroborating verdict needs blk >= the bar).
Scripts: `batch.sh` (one run at a time, temperature wait), `table.py` -> `table.out`,
`dscore.py` (DISC21). Every `.an` of the sweep and cliffs in `analyses.txt`,
provenance scores in `prov.txt`, timings in `batch.log.txt`. Accuracy runs share one
scratch cache that the head build filled (the floor changes no analysis).

## 1. Cost (cold, `--no-cache`, ABBA, idle 74 C)

| | head | floor |
|---|---|---|
| found corpus CPU-s | 783 / 902 (mean 843) | 763 / 881 (mean 822) |
| IMGS-ALL2 CPU-s | 2,711 / 2,656 (mean 2,683) | 2,707 / 2,746 (mean 2,726) |

Level: both differences are inside the runs' own spread.

## 2. DISC21 shard 0 (50k dev queries `--against` references_0, 529 true copies)

Run at 0.4 with `--dump`, each bar read off it (`dscore.py`), plus real 0.7 runs.

| bar | head true / false / µAP | floor |
|---|---|---|
| 0.4 | 386 / 1,236 / 62.62 | 386 / 1,233 / 62.62 |
| 0.5 | 381 / 387 / 66.69 (R@P90 52.74) | identical |
| 0.6 | 362 / 159 / 64.87 | 362 / 158 / 64.87 |
| 0.7 | 318 / 86 / 57.63 | identical |
| 0.8, 0.9 | 264 / 39, 181 / 15 | identical |
| real 0.7 run | 317 / 81 / 57.48 | identical |

Against a library a copy and its reference rarely share a cluster with a third
file to vouch for them, so corroboration does almost nothing there. 0.4 is
not better than 0.5 for either build: +5 true copies for ~850 false.

## 3. Correlation sweep 0.50-1.00 (`table.out`)

At the same bar the floor states fewer pairs: lower recall, higher precision
(IMGS-ALL at 0.7: F1 0.9749 -> 0.9650, P 98.83 -> 98.92%, R 96.18 -> 94.20%;
IMGS-ALL2 0.9417 -> 0.9272, R 90.11 -> 87.44%). The fair question is the
**curve**: at each head point's recall, the floor's precision interpolated between
its own bars.

| IMGS-ALL, head bar (recall) | P head | P floor (its bar) | traps head / floor | cross head / floor |
|---|---|---|---|---|
| 0.60 (97.64) | 98.714 | 98.707 (0.534) | 4,453 / 4,582 | 12,954 / 12,935 |
| 0.65 (97.10) | 98.772 | 98.764 (0.583) | 3,896 / 3,858 | 12,627 / 12,773 |
| **0.70 (96.18)** | 98.833 | 98.839 (0.635) | 3,207 / 3,139 | 12,334 / 12,333 |
| 0.75 (94.78) | 98.912 | 98.902 (0.683) | 2,488 / 2,575 | 11,786 / 11,834 |
| 0.80 (92.34) | 98.973 | 99.024 (0.739) | 2,021 / 1,682 | 11,091 / 10,782 |
| 0.85 (88.99) | 99.064 | 99.102 (0.793) | 1,396 / 1,188 | 10,120 / 9,851 |
| 0.90 (85.03) | 99.152 | 99.157 (0.840) | 1,069 / 1,049 | 8,882 / 8,850 |
| 0.95 (78.41) | 99.259 | 99.250 (0.901) | 825 / 947 | 7,190 / 7,176 |

| IMGS-ALL2, head bar (recall) | P head | P floor (its bar) | traps head / floor | cross head / floor |
|---|---|---|---|---|
| 0.60 (93.76) | 98.434 | 98.437 (0.560) | 5,170 / 5,209 | 15,485 / 15,398 |
| 0.65 (92.28) | 98.546 | 98.521 (0.608) | 4,630 / 4,398 | 14,226 / 14,780 |
| **0.70 (90.11)** | 98.607 | 98.629 (0.656) | 4,128 / 3,807 | 13,493 / 13,537 |
| 0.75 (87.05) | 98.641 | 98.696 (0.705) | 3,627 / 3,334 | 12,981 / 12,587 |
| 0.80 (82.87) | 98.703 | 98.746 (0.752) | 3,209 / 2,746 | 11,870 / 11,833 |
| 0.85 (77.56) | 98.805 | 98.835 (0.798) | 2,565 / 2,283 | 10,417 / 10,375 |
| 0.90 (70.16) | 98.891 | 98.930 (0.850) | 2,043 / 1,865 | 8,847 / 8,640 |
| 0.95 (59.00) | 99.054 | 99.041 (0.906) | 1,546 / 1,600 | 6,257 / 6,386 |

(Below 0.6 the interpolation is not readable: IMGS-ALL2's cross-family jumps
15,980 -> 23,119 between 0.55 and 0.5, so a straight line between bars
misstates it. Above 0.95 neither build states anything but byte-level copies.)

**Reading.** Mostly the same curve, moved along it: the floor at about bar −0.05
is HEAD at the bar. Precision at equal recall differs by −0.01 to +0.05
points, cross-family pairs within ±4% either way. The one steady gain is
**traps** (same-seed rearrangements) from 0.7 to 0.9: −2% / −8% at 0.7, −17%
/ −14% at 0.8, −15% / −11% at 0.85 (IMGS-ALL / IMGS-ALL2). Perfect transforms at
equal recall level (53 / 51.6, 12 / 12.8 at 0.7).

**Merges** (seed pairs analyse.py counts) are the same for both builds at every
bar, and so are the seed pairs past 300 except IMGS-ALL 0.9 (4 -> 3), 0.95 (3 -> 2) and
IMGS-ALL2 0.8 (4 -> 3). But merges depend on the bar, not on recall: at the
floor's equivalent of 0.7 (~0.65) they are those of 0.65, IMGS-ALL 62 against
HEAD's 59 at 0.7, IMGS-ALL2 74 against 69. So "move the default to keep the
recall" would cost a few merges.

## 4. Cliffs (other bars, correlation 0.7 except the correlation steps)

Merges identical between builds at every step on both corpora. Seed pairs past
300: IMGS-ALL `-k` 10 4 -> 5 (floor), IMGS-ALL2 `-k` 50 4 -> 3, `-k` 10 2 -> 4,
overlap 0.6 6 -> 5, overlap 0.5 9 -> 7. Recall at each step falls by what it
falls at the default (IMGS-ALL ~2 points, IMGS-ALL2 ~2.7-3.1). At correlation
0.4-0.2 the two builds are nearly identical (the floor's bar there is below where
corroboration had anything left to add).

## 5. IMGS-PROVENANCE

`dated` and `same-time`: identical scores (126 / 266 originals kept, 266 / 266
faithful, 0 lost, 2,368 DELETE); 12,570 -> 12,569 pairs.

## 6. Conclusion

The floor does what it says: every stated pair is at or above the bar, and
cost is level. On the generated corpora it is mostly a move along the
precision-recall curve (about 0.05 of bar), with a small real gain in
rearrangement traps between 0.7 and 0.9 and no gain in cross-family pairs or
merges. DISC21, cost and provenance do not move.
