# v73: the whole DISC21 dev set at the new defaults (2026-11-15)

Build: `img-fp` here (v72's `img-fp-new`: correlation 0.65, aligned points 5).
50,000 dev queries `--against` each of the 20 reference zips in turn
(`loop.sh`: `-t 8 -v --no-last --cache cache/c.db --prune-cache --format json`),
~25-31 min a shard, 10.5 h with downloads. `score.py` merges the 20 reports
(best evidence per query-reference pair) against the dev ground truth;
`pershard.py` scores each shard alone.

The first pass wrote text reports (`-o shardN.json.part` chose the layout by
extension), which state one match per query and cannot be scored; they were
deleted 2026-11-16 (`loop-text.log` is the pass's log). Every shard was rerun
with `--format json`.

## Whole dev set (1,000,000 references, 10,000 true copies)

| ranking | bar | true | false | µAP | recall | R@P90 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| aligned points | 0.65 (run) | 6,657 | 3,062 | **63.64** | 66.57 | 55.33 |
| aligned points | 0.7 read off | 6,276 | 2,040 | 60.55 | 62.76 | 54.60 |
| points + corr/2 | 0.65 (run) | 6,657 | 3,062 | 63.61 | 66.57 | 55.21 |
| points + corr/2 | 0.7 read off | 6,276 | 2,040 | 60.53 | 62.76 | 54.58 |

"0.7 read off" drops pairs under 0.7 correlation from the same runs; its
points bar is still 5, so it is not the old default (0.7 / 10).

Earlier whole-dev figures (0.76.0, ranked by points): 65.3 at bar 0.5, 59.9
at 0.7 / 10. SSCD as published: 61.5 (ResNet50), 63.7 (ResNeXt-101);
72.5 / 75.3 with score normalization.

## Per shard (points + corr/2, bar 0.65)

µAP 60.17-66.64, median ~63.4; shard 0 (62.49, 348 / 125) reproduces v72's
run exactly and sits slightly below the middle. See `pershard.out`.

## Shard 0 at bar 0.5, same build (`b05/`, 2026-11-15)

`b05/run.sh` (as `loop.sh`, `--min-pixel-correlation 0.5`, a fresh copy of
cache57), `b05/compare.py` -> `b05/compare.out`. Ranked by aligned points:

| run | read at | true | false | µAP | R@P90 |
| --- | --- | ---: | ---: | ---: | ---: |
| 0.5 | 0.5 | 386 | 458 | **67.29** | 53.12 |
| 0.5 | 0.65 | 348 | 135 | 62.45 | 50.09 |
| 0.65 | 0.65 | 348 | 125 | **62.52** | 50.09 |
| 0.5 | 0.7 | 322 | 95 | 58.24 | 49.53 |
| 0.65 | 0.7 | 322 | 89 | 58.30 | 49.53 |

Points + corr/2 is within 0.07 of each. The 0.5 run states every true copy
the 0.65 run does and 38 more; read at 0.65 it has 10 more false pairs (the
cluster slack reaches lower), otherwise the same. Earlier shard 0 at 0.5
(older build): 66.69. CPU 3,354 s against the 0.65 run's 6,535 s
(single runs, not A/B; not explained here).

## Shard 0 at bar 0.3 (`b03/`, 2026-11-15)

`b03/run.sh` (as b05, `--min-pixel-correlation 0.3`, on b05's cache),
`b03/compare.py` -> `b03/compare.out`, ranked by aligned points:

| run | read at | true | false | µAP | R@P90 |
| --- | --- | ---: | ---: | ---: | ---: |
| 0.3 | 0.3 | 393 | 37,528 | **49.32** | 32.89 |
| 0.3 | 0.4 | 390 | 6,272 | 63.22 | 46.69 |
| 0.3 | 0.5 | 385 | 983 | 67.17 | 52.93 |
| 0.3 | 0.6 | 366 | 234 | 65.48 | 52.36 |
| 0.5 | 0.5 | 386 | 458 | **67.29** | 53.12 |
| 0.5 | 0.6 | 366 | 186 | 65.44 | 52.36 |
| 0.65 | 0.65 | 348 | 125 | **62.52** | 50.09 |

0.3 states 7 true copies 0.5 does not (393 vs 386) for 37,070 more false
pairs; µAP falls 18 points. By bar on shard 0: 0.3 49.3, 0.4 ~63.2
(read off), 0.5 67.3, 0.6 ~65.5 (read off), 0.65 62.5, 0.7 58.3 (read off).
Read-offs carry the lower run's cluster slack (more false pairs than a run
at that bar: 983 vs 458 at 0.5) but nearly the same µAP. CPU 3,452 s.

## Shard 0 at bar 0.5, `--detail 1024` (`b05d/`, 2026-11-15)

`b05d/run.sh` (as b05 plus `--detail 1024`, on b05's cache, so the 1024
level analysed cold), `b05d/compare.py` -> `b05d/compare.out`, by points:

| detail | read at | true | false | µAP | R@P90 |
| --- | --- | ---: | ---: | ---: | ---: |
| 1024 | 0.5 | 388 | 471 | **67.52** | 53.12 |
| 512 | 0.5 | 386 | 458 | **67.29** | 53.12 |
| 1024 | 0.65 | 350 | 135 | 62.76 | 50.09 |
| 512 | 0.65 | 348 | 135 | 62.45 | 50.09 |

+2 true copies (none lost), +13 false, µAP +0.23. CPU 15,943 s, 49 min,
against 3,354 s, 14 min (includes describing the 1024 level for 100,000
files; a cached rerun was not measured).

## Where shard 0's missed copies are lost (`why/`, 2026-11-15)

The 143 true copies the 0.5 / 512 run misses (`why/missed.txt`). `why/dump.sh`:
the same run with `--dump` (verdicts of >= 3 points and >= 0.2 correlation);
`why/alone.sh`: each pair scanned by itself; `why/classify.py` -> `classify.out`
(bars read approximately: points 5, correlation 0.5, overlap 0.85 on the
larger side, centre; slack and cluster rules not replayed).

| in the full scan | n | stated alone |
| --- | ---: | ---: |
| no verdict (never verified, or under 3 points / 0.2) | 115 | 50 |
| verified, fails overlap | 15 | 0 |
| verified, fails correlation | 8 | 0 |
| verified, fails centre | 2 | 1 |
| verified, fails correlation + centre / + overlap | 3 | 0 |

51 of 143 are stated alone, 50 of them never verified in the full scan
(retrieval does not reach them: e.g. Q08853 72 points / 0.80 alone, Q35225
16 / 0.95). 92 are not stated even alone; 39 of those have no verdict of 3
points alone. `why/examples.jpg`: 16 drawn at random (seed 1).
