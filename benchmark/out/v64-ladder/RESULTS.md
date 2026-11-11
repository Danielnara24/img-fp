# v64: --fast against the default, and 256 as the ladder's base

Builds: head (b36361f); lad2 = head + `ladder.patch` (scaffolding, not
shipped): every analysis is the one at 256 (`analysis::BASE`), then finer
levels up to `--detail` (default 512). A level's plane is what an analysis at
its size would describe, small pictures doubled as `sift::enlargement` does
(`Finer::up`). A level keeps the scales below where the level beneath
starts detecting (1.6 k of its pixels, not 1.6: at 1.6 the 3.2 px scale at 512
is detected by neither). The 512 level holds up to `FEATURES` of its own, and
levels past 512 have today's caps counted from 512. Steps 256, 512, ... as
`match_all` has them; the 512 step's window divisor, second look and veto
are switches (`IMGFP_W512`, `IMGFP_SL512`, `IMGFP_VETO512`).
Scripts `fast.sh`, `lad.sh`; log `lad.log.txt`, `fast.log`.

## 1. Does the default state what --fast states? No

| | --fast true | default true | --fast's true pairs the default misses |
|---|---|---|---|
| IMGS-ALL2 (0.7) | 1,157,202 (R 83.6%, F1 0.905) | 1,247,528 (R 90.1%, F1 0.942) | 13,304 (1.1%) |
| DISC21 shard 0, 0.5 | 327 (µAP 59.00) | 382 (µAP 66.89) | 9 |
| DISC21 shard 0, 0.7 | 279 (µAP 51.16) | 317 (µAP 57.48) | 11 |

## 2. Probe: how today's 512 analysis splits (600 IMGS-ALL2 files, 200 found)

375 / 527 keypoints a file (the 600 cap rarely binds), 61% / 62% finer than
the 256 plane's first octave; the 256 analysis holds 126 / 165. So a 256 base
plus one fine octave at 512 holds about what 512 holds without a budget cut.

## 3. The ladder on IMGS-ALL2 (cached on a cache lad2 filled; CPU-s user)

| | TP | FP | R | F1 | traps / cross | merges | past 300 | matching CPU-s |
|---|---|---|---|---|---|---|---|---|
| head default | 1,247,528 | 17,621 | 90.11 | 0.942 | 4,128 / 13,493 | 69 | 3 | 828 |
| ladder, 512 step as finer (w15) | 1,237,240 | 18,648 | 89.4 | 0.937 | | | | 885 |
| ladder, windows -k / 3 | 1,247,009 | 19,323 | 90.1 | 0.941 | 4,127 / 15,196 | 83 | 4 (Mural1/2 994) | 963 |
| ladder, full windows | 1,248,418 | 19,456 | 90.2 | 0.941 | 4,186 / 15,270 | 86 | 4 (Mural1/2 998) | 1,122 |
| + second look at 512 | 1,250,296 | 19,676 | 90.3 | 0.942 | | | | 1,243 |
| + no veto | 1,252,387 | 28,032 | 90.5 | 0.940 | | | | 1,300 |
| `--detail 256` | = today's --fast, report identical | | | | | | | 449 (head --fast 385: the cache's records hold the 512 level and are cut) |

Cold analysis + matching (one run, not ABBA): ladder 2,853 CPU-s against
~2,683 (v62's ABBA mean for head).

## 4. Why it cannot meet "nothing extra, nothing lost"

A ladder states every coarser step's claims, so the 512 run states the 256
step's false pairs as well as its own. --fast's false pairs are other pairs
than the default's (siblings its coarse features join), so the union has
more: +1,700-1,800 cross-family, merges 69 -> 83-86, Mural1/2 past 300. That
is structural; only letting the 512 step drop the 256 step's claims removes
it, and then it is no ladder. Recall needs the 512 step's full windows,
whose matching comes on top of the 256 step's: +16% (-k / 3) to +35% (full)
matching CPU. The analysis itself is about level.

Not shipped; source reverted. The no-cost way to one knob is `--detail 256`
as today's --fast analysis and >= 512 as today's (no containment 256 -> 512,
which --fast does not have today either).
