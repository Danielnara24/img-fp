# v56-seed: verdicts independent of the vocabulary, and fresh seeds (2026-11-04/05)

Scaffolding, not shipped: `IMGFP_SEED=bits,tables` (`src/seed.rs`, `Pass::seeds`),
`IMGFP_SEED_H`, `src/probe.rs`, the seed stats line. Off, the build is HEAD to the byte
(`derived/Desktop`: 2,649 pairs, groups identical).

## 1. What changes

A verdict's first correspondences were the keypoint pairs sharing a vocabulary word, and
the vocabulary is trained on whatever is scanned. Now: each descriptor gets a code of
`bits x tables` signs of fixed random +-1 hyperplanes through its own mean (integer,
exact on every build, constant seed, no training). Two keypoints sharing a table's bucket
are compared if their codes differ in at most H bits (p99 of a correspondence at D 320,
derived from k and l), and each A keypoint keeps its nearest within D (`GUIDE_D2`).
Retrieval keeps its vocabulary: it only proposes candidates. Codes: 12 bytes a keypoint,
in the spill file.

Measured per-bit agreement (`probe_bits`, 1,478 v46 probe pairs): unrelated 0.549; true
correspondences 0.988 (<50), 0.943 (50-100), 0.903 (100-150), 0.863, 0.823, 0.781,
0.752 (300-350).

## 2. Probe (v46's 1,478 pairs, against every keypoint vs every keypoint)

Every hashed setting tried (sign, +-1, p-stable; 10-18 bits, 4-16 tables) passes within
1-4 pairs of brute force: siblings 13-15 / 14, true 678-683 / 683, unrelated 0-1 / 0, at
0.4-9k candidate pairs a verdict instead of ~250k. The probe cannot choose among them.

## 3. Corpora, cached (`fixtest/all.bin`, `all2.bin`)

| | IMGS-ALL F1 / R / cross-family | IMGS-ALL2 F1 / R / cross-family | IMGS-ALL2 CPU-s |
|---|---|---|---|
| words | 0.9721 / 96.04 / 18,139 | 0.9343 / 88.78 / 13,657 | 649 / 635 / 667 / 695 |
| 14 x 2 | 0.9720 / 96.02 / 18,254 | 0.9327 / 88.51 / 13,657 | 942 |
| 14 x 3 | 0.9721 / 96.04 / 18,110 | 0.9350 / 88.90 / 13,599 | 971 |
| 14 x 5 | 0.9722 / 96.06 / 18,254 | 0.9365 / 89.19 / 13,906 | 1,087 |

The words baseline here (0.9721 / 0.9343) is below the figures recorded for 0.79.0-0.80.0
(0.9748 / 0.9415) on the same caches; not traced, every row above is this session's build.
Cost by stage (IMGS-ALL2, prof, 14 x 5): seeding 78 -> 520 CPU-s, guided 207 -> 373,
transform search 49 -> 132, second look 44 -> 118: more correspondences reach more
hypotheses. Tried for cost: bucket index per asker (same output), chained buckets,
occupancy bitmap, code-distance bound, 10-16 bits: the floor is about +45-50%.
Wider than 14 bits is dearer (the asker's table leaves cache), narrower too (coincidences).

## 4. Small families (`families.py`, fresh corpus added)

Total seed pairs joined (n0 + n1 + n2 + f):

| | words | 14 x 3 | 14 x 5 |
|---|---|---|---|
| IMGS-ALL | 61.0 | 64.7 | 66.3 |
| IMGS5 | 40.7 | 43.3 | 45.0 |
| IMGS-ALL2 | 57.0 | 58.7 | 64.0 |
| HOLI-DEV | 971.3 | 987.7 | 998.7 |

Hashing joins a little more: siblings the vocabulary hid in the large setting (`f`) are
now judged on their own (HOLI-DEV `f` 231 -> 253, against 251 / 252 on seeds alone).

## 5. The promise: one pair, one verdict (`invariance.py`)

Seed-original pairs stated alone (n0) and among 9,285 found photos (f):

| | both | same evidence | in one only |
|---|---|---|---|
| words, HOLI-DEV | 219 | 146 | 32 + 12 |
| 14 x 3, HOLI-DEV | 249 | 240 | 3 + 4 |
| 14 x 3, IMGS-ALL / ALL2 / IMGS5 | 15 / 15 / 11 | all | 1 / 1 / 0 |

Every remaining difference is a propagated pair (0 aligned points: composed through a
third file) or a pair met in one setting only (retrieval's `-k` among 814 files, or the
claim graph). No direct verdict met in both differs.

## 6. HOLI-DEV / HOLI-TEST (fresh seeds)

INRIA Holidays (1,491 photos, 500 scene groups), groups split by sha256 parity: dev 263
groups / 814 photos, test 237 / 677 (held out, not yet run). 12 transforms a seed
(`jpeg_mid, scale_small, crop_off_centre, crop_half, crop_slight, rotate_small, flip_h,
greyscale, watermark_corner, embed_small, collage_cell, phone_screenshot`);
`~/Documents/FRESH/generate.sh`. Words: 248 of 1,249 dev sibling pairs joined on seeds
alone (20%), pair precision 50.8% (n1) / 58.4% (n2), recall 97.5 / 95.7%. By eye (8 at
random): about half near-repeat shots (seconds apart), one a zoom, two different views
(San Marco from two places; two sunsets). UKBench download stopped at 1.18 / 1.65 GB.

## 7. Different scenes joined on Holidays, and the keypoint-size bar (2026-11-05)

HOLI-DEV seeds alone (words) join 3 different-scene pairs, and among the found photos
(hashed) 1. All four the same way (`probe_at`, from the dumped transforms): one picture
shrunk 3-15x (axes 1/3-1/8 and 1/10-1/15) into 1-3% of the other, two as a mirrored
negative; the inlier tolerance is in the larger frame (~10 px, ~100 px in the shrunk one),
so 27-67 "aligned points" land on 6-13 keypoints of the spot; the spot is ~25x10 px of the
128-px thumbnail, where the shrunk picture is a two-tone layout (correlation 0.70-0.76).
Their inliers' sizes are 2.0-3.2 octaves (median) off the transform's scale.

One-to-one inlier count (`IMGFP_ONE2ONE`, `run8.sh`): HOLI-DEV 3 -> 1 (Grenoble / crater at
13); IMGS-ALL TP -817 / FP -102 (`embed_tiny` 347 -> 341, `crop_micro` 305 -> 301), IMGS-ALL2
TP -2,213 / FP -158 (`crop_micro` 273 -> 263, `embed_tiny` 336 -> 326). Rejected.

Size offsets on true verdicts (`probe_sizes`, 80 IMGS-ALL2 seeds x 14 transforms, brute force):
per-verdict median within +-0.3 octave on all but one seed (`panoramic3`: up to +1.18 on a
passing verdict, +2.09 on a failing one); of 1,008 passing true verdicts 1 over 1 octave, 3
over 0.5. Pooled inliers p05 -0.15, p95 +0.37.

Median bar at one octave (`IMGFP_SIZE_BAR=1`, `run9.sh`): HOLI-DEV different-scene joins 3 -> 0,
siblings 968.3 -> 968.7; IMGS-ALL, IMGS5 identical; IMGS-ALL2 57.0 -> 56.7 (unrelated 1 -> 0.7).
Full IMGS-ALL F1 0.9721 -> 0.9721 (TP -143, FP +1, recall 96.04 -> 96.03%, `contact_sheet`
354 -> 353, `mirror_crop` 361 -> 360), IMGS-ALL2 0.9343 -> 0.9342 (TP +2, FP +39 traps,
`contact_sheet` 351 -> 350); cross-family and merges identical on both.

## 8. Shipped (0.81.0), and HOLI-TEST read once

`verify::SIZE_AGREEMENT` (1 octave) in `judge`, the scaffolding reverted (`scaffolding.patch`).
`img-fp-ship` writes `size-all.json` and `size-all2.json` byte for byte (less runtime).
Keep rule: `suggest_score.py` IMGS-ALL 0 lost, 356 / 364, 364 / 364, actions 1 KEEP / REVIEW;
IMGS-ALL2 0 lost, 355 / 364, DELETE +2; `pairs.py` identical but `contact_sheet` REVIEW
301 -> 299. Cost, cached IMGS-ALL2, ABBA (60 s between, no temperature wait): head 663 / 634
CPU-s, ship 656 / 660. HOLI-TEST (`fam-test-head`, `fam-test-ship`): different-scene joins
1 -> 0 (116703 | 113200, mirrored, 37 points, scale 7.6, 0.756), siblings 713.3 -> 711.0.

## 9. The checks not run first, and the cache (2026-11-05/06)

`run11.sh`, `run12*.sh`, `score11.sh`, `trace/`. Cliffs (`cliff_table.out`): no merge added at
any step, red_rocks_sign1/2 gone at IMGS-ALL2 correlation 0.5-0.3, F1 within 0.0001 at points,
overlap and `-k`; at correlation 0.2 FP -1,154 / -2,006, TP +2,090 / +3,039. `--detail`
(`detail/*.txt`): F1 0.9439 / 0.9445 at 1024 / 2048, both builds. DISC21 shard 0
(`disc/score.out`): true copies identical at every step 0.2-0.9, false 424 -> 329 at 0.5
(µAP 64.26 -> 65.71), 85 -> 81 at 0.7. Proxy (`proxy/`): 0.5 false 80 -> 47, µAP 72.7 -> 73.3.
Cold ABBA (`cold/`): 3,132 / 3,415 head, 3,428 / 3,529 ship, unreadable (idle 84.5 C).

`detail_contain.py` first read "11,136 lost 512 -> 1024": its 512 run read `fixtest/all2.bin`.
From one fresh cache (`trace/`): 512 cold F1 0.9414 (recall 90.06%), 1024 0.9439, 1 true pair
lost, 6,338 added, 1024 report identical to `detail/ship-1024.json`. The fixtest cache's records
give 0.9343 with the same build: the analysis changed after 0.76.0 under format 20. Shipped with
the format bumped to 21 (0.81.0).
