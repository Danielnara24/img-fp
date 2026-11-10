# v33: fitted transform, filtered pyramid, clean bar 0.9, whole-frame grid 64 (0.46 work)

IMGS-ALL, --work-size 512, plain x86-64 build, bench.py (cold, cooled, alternated).

## Probe: 48 seeds x 6 exact lossless transforms (288 pairs), copies under the clean bar (blk_min < 0.85)
| | fitted transform | exact transform |
|---|---|---|
| HEAD | 110 | 12 |
| LS fit kept | 12 | 12 |
| + [1 3 3 1] pyramid | 2 | 2 |
(B-spline taps instead of the pyramid filter: 6.)

## Clean bar sweep (fit + pyramid), F1 / cross-family false pairs
0.85: 0.9793 / 6,940 (Segovia1-Segovia2 merge, 6,933) · 0.875: 0.9823 / 7 · 0.90: 0.9821 / 7 ·
0.925: 0.9816 / 92 · 0.95: 0.9806 / 93 · HEAD: 0.9805 / 95

## Keep rule (suggest_score.py), whole-frame grid sweep at clean 0.9
| grid | DELETE | losses | originals kept | faithful copy |
|---|---|---|---|---|
| 32 | 18,183 | 3 | 276 | 304 |
| 48 | 17,759 | 1 | 276 | 304 |
| 64 | 17,293 | 0 | 276 | 304 |
| 96 | 16,002 | 0 | 276 | 304 |
| HEAD | 17,986 | 1 | 272 | 299 |
WHOLE_RESIDUAL 8 at grid 32: 17,735 / 1 loss. suggest::CLEAN 0.85/0.9/0.95: same 3 losses at grid 32.
Grid 64 costs ~0.6 s of a cached IMGS run (stage 1.4 -> 2.1 s).

## Final A/B (final-*)
| | HEAD 1 | new 1 | new 2 | HEAD 2 |
|---|---|---|---|---|
| CPU-s | 2,804 | 2,697 | 2,804 | 2,814 |
| wall s | 453 | 431 | 448 | 452 |
| peak PSS MB | 2,310 | 2,327 | 2,336 | 2,327 |
F1 0.9805 -> 0.9821, P 99.56 -> 99.57, R 96.59 -> 96.90, cross-family 95 -> 7, perfect transforms 46 -> 47,
halves 0.9788/0.9808 -> 0.9799/0.9829. Keep rule: DELETE 17,986 -> 17,293, losses 1 -> 0, originals 272 -> 276,
faithful 299 -> 304.
