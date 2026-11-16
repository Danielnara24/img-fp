# v70: IMGS-ALL3, the third catalogue (2026-11-13)

`/home/daniel/Documents/IMGS-ALL3`: the 364 seeds of IMGS-ALL through
`corpus/transforms3.py` (`make_variants.py --catalogue third`; parts and
`generate.sh` in `/home/daniel/Documents/IMGS-v3`). 90 transformations, none
in IMGS-ALL or IMGS-ALL2: 60 single (containers, crops, hosts, occlusions,
tone and colour, warps, degradations, 3 traps, 4 pipelines) and 30 chains of
two of them, each step from a salt of its own. 33,124 files, 1,326,051
positive pairs, no write failures. The amounts were set once and not adjusted
after any tool's result.

Run: `run.sh` (bench.py, each tool cold, alone, evicted, cooled; img-fp
0.85.0 plain build, defaults). The machine was ~4 GB into swap at the start,
so cost reads slow.

| tool | F1 | precision | recall | single (57) | chains (29) | perfect | wall | CPU s | anon+swap |
|---|---|---|---|---|---|---|---|---|---|
| img-fp | 0.921 | 98.5% | 86.5% | 97.9% | 92.0% | 23 / 86 | 537 s | 3,772 | 893 MB |
| sscd | 0.464 | 89.1% | 31.4% | 79.0% | 31.0% | 0 | 12,631 s | 69,676 | 7,186 MB |
| imagededup_phash | 0.370 | 99.1% | 22.8% | 59.0% | 19.6% | 0 | 424 s | 2,910 | 3,199 MB |
| imagededup_cnn | 0.193 | 89.2% | 10.8% | 46.4% | 9.3% | 0 | 1,128 s | 2,001 | 12,280 MB |
| pdq | 0.183 | 100.0% | 10.1% | 46.2% | 3.1% | 0 | 654 s | 3,788 | 1,751 MB |
| czkawka_tuned | 0.089 | 100.0% | 4.7% | 30.5% | 0.1% | 0 | 813 s | 5,988 | 1,083 MB |

"single" and "chains" are seed-to-variant recall (the per-transform table,
`all3/per_transform.csv`); F1 is over all pairs.

img-fp (`all3/analyse.txt`): false pairs 17,112 = traps 3,625 + cross-family
13,487, all near-miss siblings; past 300: Mural1/2 6,366, bird1/2 5,201,
Floriana1/2 413, Floriana3/4 353. Mural1/2 is 325 on IMGS-ALL and not past
300 on IMGS-ALL2. Lowest transforms: `tiled_repeat+solarize` 242,
`triptych+film_grain` 247, `triptych` 259, `proof_stamp+fax` 266,
`crop_band+ghost` 284, `solarize` 286, `crop_corner+social_post` 287.
`triptych` is the one transform a competitor beats it on by a margin
(imagededup_phash 341, sscd 331).

Not matching for want of a decoder or of EXIF, not of a matcher:
`jxl_lossless` (imagededup, pdq: 0), `exif_turned` (imagededup_phash, pdq:
0; score with `--exclude-geometry exif` to leave it out), `to_gif`
(czkawka: 0).

Near-blank outputs left as generated (std under 4 grey levels at 32 px):
`channel_drop` and `crop_band+channel_drop` 12 seeds each, `bloom` and
`ripple+bloom` 2, `solarize` 1, `crop_corner+fade_matte` 1.
