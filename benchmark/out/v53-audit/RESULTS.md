# v53-audit: audit A (vocabulary twins) and B (damaged files)

Run 2026-10-28 by `run.sh` and `cost.sh`, one shared analysis cache (`cache.bin`).
Binaries in `bin/`. `base` is 0.74.0. `new` is the working tree with B, C and two
switches for A:

- `off`: `new` with no switch (B and C only).
- `floor`: `IMGFP_WORD_R=143`. Leaves of one parent within r of each other are one word.
  This is the v46 word floor brought back; r = 143 is the value derived then.
- `sample`: `IMGFP_SAMPLE_R=143`. At the deepest level of the tree, k-means is not
  trained on a member within r of one already kept, so no leaf is made between two
  observations of one point.

Every wrapper deletes `cache.bin.matched` first. The kept matching's fingerprint does
not see the switches, so without that a variant could take another variant's matching.

## A: the defect

`logo.jpg` with a pixel-identical PNG copy, or with a q95 JPEG copy, among 15, 30 and
120 random found photos, five draws each:

| | base | floor | sample |
|---|---|---|---|
| copies missed, of 30 | 28 | 0 | 0 |

**Cause.** In `index_all`, an image with fewer descriptors than `per` puts all of them
into the vocabulary sample, and so does its copy. Two copies of a descriptor are then
the closest members of their deepest-level group, and k-means gives each one a leaf of
its own. In a 9-file folder each of the logo's 42 words was the neighbouring leaf of
its copy's word (17/18, 44/45, …), so the two files shared no word at all: the copy
was never a candidate, and guided matching (which needs 3 matching points to start)
never ran.

## Corpora

| | base | off | floor | sample |
|---|---|---|---|---|
| IMGS-ALL F1 | 0.9735 | = base | 0.9739 | 0.9738 |
| precision / recall | 98.88 / 95.86 | | 98.85 / 95.98 | 98.85 / 95.95 |
| cross-family pairs | 11,932 | | 12,227 | 12,166 |
| seed pairs with a false pair | 66 | | 72 | 79 |
| merges of 300+ pairs | 5 | | 5 (Floriana3/4 474 -> 554) | 5 (docks1/2 75 -> 154, under 300) |
| perfect transforms | 51 | | 54 | 52 |
| IMGS-ALL2 F1 | 0.9356 | = base | 0.9390 | 0.9393 |
| precision / recall | 98.59 / 89.01 | | 98.63 / 89.60 | 98.62 / 89.66 |
| cross-family pairs | 13,515 | | 13,091 (Henares1/2 200 -> 0, Floriana1/2 483 -> 179) | 13,203 |
| perfect transforms | 7 | | 11 | 11 |

`off` writes IMGS-ALL and IMGS-ALL2 reports byte-identical to `base`, less
`runtime_seconds`. Each variant's two IMGS-ALL runs are byte-identical too.

Keep rule (`suggest_score.py`). No content lost anywhere.

| | base | floor | sample |
|---|---|---|---|
| IMGS-ALL originals kept / faithful copy left | 358 / 364 | 358 / 364 | 358 / 364 |
| IMGS-ALL strict line | 55 | 55 | 55 |
| IMGS-ALL2 originals kept / faithful copy left | 352 / 364 | 351 / 364 | 351 / 364 |
| IMGS-ALL2 strict line | 38 | 38 | 38 |

The IMGS-ALL2 originals change by churn, not by one family:

- floor loses beach, Teatr_na_wodzie_(1) and UnionCity, and gains airplane_wing2 and
  Wignacourt Tower1.
- sample loses 12.webp, galaxy, Teatr and UnionCity, and gains airplane_wing2,
  manhole2 and Wignacourt Tower1.
- In nearly every lost family the one survivor of the original's lossless copies was
  `webp_lossless`, now deleted too (`keeporig.py`).

## Small families (`families.py`)

`fam-*/score.out` holds the full tables. `base` reproduces `out/v51-families` exactly.

| total joined (lower is better) | base | floor | sample |
|---|---|---|---|
| IMGS-ALL | 66.0 | 65.0 | 67.0 |
| IMGS-ALL2 | 63.7 | 63.0 | 65.7 |

| recall, mean of three draws | base | floor | sample |
|---|---|---|---|
| IMGS-ALL n1 / n2 | 96.20 / 95.46 | 96.77 / 95.59 | 96.77 / 95.59 |
| IMGS-ALL2 n1 / n2 | 92.58 / 90.92 | 93.15 / 91.18 | 93.34 / 91.40 |

Sets joined differently from base:

- floor: + Acueducto1/2 (IMGS-ALL2 n2-d1); − Floriana2/4 among the found photos, on
  both corpora.
- sample: + boat_photo1/2 joined directly as originals (n0, both corpora), + Parish
  Church1/2, Shenzhen1/2, Acueducto1/2 in single draws, and two *unrelated* seeds
  joined (galaxy | sea_photo4, IMGS-ALL2 n2-d2); − field2/3, red_rocks_sign1/2 and
  Wignacourt Tower1/2 in single draws.

## Cost

User CPU-seconds, cached analysis, `-t 8`, a 60 s cooldown and a wait for idle + 3 C
between runs. Order: base, floor, sample, sample, floor, base. About 5 GB of swap was
in use throughout.

| | base | floor | sample |
|---|---|---|---|
| IMGS-ALL matching | 923, 918 | 995, 990 (+7.9%) | 937, 926 (+1.1%) |
| found mix (364 originals among 9,285 found photos) | 347, 347 | 375, 361 (+6.0%) | 360, 358 (+3.4%) |
| found mix pairs (unlabelled) | 2,339 | 2,194 | 2,604 |

## B: damaged files (synthetic, base vs new)

`kitchen.jpg` with a copy cut off, and `dog2.jpeg` with a copy missing only its EOI marker:

- **EOI missing (same pixels):** base keeps the damaged file and deletes the intact
  one under all three rules. new keeps the intact one and deletes the damaged one.
- **Cut at 85%, with the intact original and a q90 copy:** base makes the damaged
  file the group's head and KEEP, and `--suggest representative` deletes both intact
  files. In new an intact file is the head, and `representative` deletes the damaged
  one and the q90 copy.
- **Cut at 95% or 85%, intact original alone:** unchanged; the intact file was the
  head already.
- **Left as it was:** in the 85% case with three files, the content rule still gives
  the damaged file KEEP. Its best match, 0.89, is under `REVIEW`, and its correlation
  is low because of the damage itself.
