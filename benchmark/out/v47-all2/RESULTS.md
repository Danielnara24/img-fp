# img-fp 0.74.0 on IMGS-ALL2 (baseline), 2026-10-27

`run.sh`: `bench.py --only imgfp --corpus IMGS-ALL2` (cold, `--no-cache`, plain
build, defaults), then `analyse.py --per-transform` (`analyse.out`),
`score.py --per-transform` (`score.out`), `suggest_score.py` (`suggest.out`).

| | IMGS-ALL2 (0.74.0) | 0.73.0: IMGS-ALL / IMGS5, run apart (`v46-matcher/rel`) |
|---|---|---|
| F1 | 0.9681 | 0.9774 / 0.9425 |
| precision | 99.59% | 99.73 / 92.40% |
| recall | 94.18% (FN 80,866) | 95.83 / 96.17% (FN 47,709 + 8,649) |
| perfect transforms | 51 of 88 | 58 (IMGS-ALL) |
| traps / cross-family | 3,566 / 1,817 | 2,527 / 479; 435 / 17,418 |
| seed pairs >= 300 false pairs | bird1/2 788, Floriana1/2 339 | none; bird 6,827, Fountain3/5 6,642, Floriana1/2 2,872, Floriana3/4 473, Mural 327 |
| halves (alt / hash) | 0.9673 / 0.9673 | |
| cost | 427 s wall, 3,043 CPU-s, 656 MB anon+swap, 33,500 files | ~2,500 CPU-s, 27,659 files |

IMGS5's merges are smaller here **not because anything improved**: inside a
33k-file corpus the vocabulary thins siblings' correspondences (CLAUDE.md, top
section). Precision on this corpus is not a safety figure; use `families.py`.

Per transform (seeds whose variant pairs with the original, old = IMGS-ALL +
IMGS5 pooled, old names): perspective 348 -> 281, wave 310 -> 273,
barrel_distort 350 -> 325, crop_micro 304 -> 286, embed_tiny 347 -> 340,
crop_quarter / photo_of_screen / crop_half / crop_panoramic / wall_poster
-3 to -4; pinch (new) 244; up: crop_strip 352 -> 360, perspective_two 345 ->
352, tiled_watermark 237 -> 244, contact_sheet 346 -> 350.

Keep rule: 19,717 DELETE, 4,028 REVIEW, none loses content; 357 / 364
families keep the original or a lossless copy, 364 / 364 a faithful copy;
strict line 52; picture only in a composite / mirror / edit 4 (b&w2,
document1, Acueducto1, red_rocks_sign1).
