# v35: audit fixes (A-H), IMGS-ALL, --work-size 512, plain x86-64, one run each (accuracy only)

Baseline: out/v34-spill/all-1 (0.46.1). Runs cached in this folder's cache.bin (analysis unchanged).

| | F1 | P | R | trap FP | cross-family | perfect | DELETE | lost | originals kept |
|---|---|---|---|---|---|---|---|---|---|
| 0.46.1 | 0.9821 | 99.57 | 96.90 | 4,814 | 7 | 47 | 17,293 | 0 | 276 |
| new: grey twins matched apart | 0.9818 | 99.57 | 96.83 | 4,816 | 7 | 41 | - | - | - |
| new2: twins matched as one, stated apart; framed any turn | 0.9824 | 99.57 | 96.95 | 4,816 | 7 | 48 | 17,302 | 1 (tile_shuffle) | 276 |
| new3: framed = resize only (shipped) | 0.9823 | 99.57 | 96.93 | 4,814 | 7 | 48 | 17,299 | 0 | 276 |

Halves new3: 0.9802 / 0.9830 (0.46.1: 0.9799 / 0.9829). Same seven cross-family pairs as 0.46.1.
same_pixels pairs 2,081 -> 1,466: the generator's `greyscale` JPEGs share their original's Y plane,
and are now stated as perfect direct matches, so the keep rule chooses colour over grey.
Cold CPU (new, not alternated): 2,844 user-s, 450 s wall; v33 runs 2,697-2,814. Not an A/B.
