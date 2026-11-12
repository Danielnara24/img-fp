# v65: 256 as the base of `--detail`, two more designs (not shipped)

2026-11-11/12, after v64. The brief: make 256 the base of the detail ladder
so `--fast` goes and `--detail 256 | 512 | ... | 4096` is the one knob, at
no extra cost and nothing lost. Then (user, 11-11 evening): the 256 run
shares the cache with the others, every step states what the step below
states, and where accuracy and cost collide, cost gives way. Then (11-12):
+40% is too much. Reverted to HEAD (3df498c); user's decision.

HEAD = 3df498c, plain x86-64 build. All runs `-t 8`, IMGS-ALL2 / IMGS-ALL
cached on a cache the measured build filled unless "cold" (`--no-cache`,
evicted, rested). CPU-s are user time.

## The two designs

Both: `analysis::BASE` = 256, one reduction to the 512 plane, the 256 plane
that plane fitted down (`reduce::fitted`), then levels at 512, 1024, ... as
v64 made them; matching steps 256, 512, ....

**A, the 512 step leans on 256** (`g8`; `snapshots/pipeline.g5.rs` is its
pipeline with the switches still in, before one vocabulary replaced two).
Built for cost parity:
- the 256 and 512 levels share one `FEATURES` (600) budget;
- one vocabulary, trained on the 512 step's features, for every step;
- the 512 step asks in windows of `-k` / 3, has no second look, verifies
  afresh only each file's 3 best candidates the 256 step already judged
  (the rest keep the 256 verdict), and its claims overrule: a clean 256
  verdict between files the 512 step's own clusters do not join is demoted,
  and a 256 claim joining two of the 512 step's families is not stated;
- `--detail 256` reduces straight to 256 (today's `--fast` to the byte) and
  keeps records of its own.

**B, the 512 step is a matching of its own** (`h3`; `full-512-step.patch`
is the whole diff against HEAD, `snapshots/pipeline.h3.rs`). Built for the
properties:
- `--detail 256` makes the 256 plane as every run does, so its record is
  the leading level of any finer record (one cache);
- the 512 level has `FEATURES` of its own; a vocabulary a step for 256 and
  512; the 512 step is a plain matching (full windows, second look, nothing
  taken as decided), the better verdict of the two steps kept;
- every 256 claim is stated (the veto families are both steps' claims).

`h1` is B with the 512 step still skipping pairs the 256 step judged and no
second look there; `h1` + `IMGFP_SL512` (`h2`) adds the second look.

## Results

| | IMGS-ALL2 P / R, TP, FP | IMGS-ALL P / R, TP, FP | DISC21 shard 0, µAP 0.5 / 0.7, true at 0.5 |
|---|---|---|---|
| HEAD | 98.61 / 90.11, 1,247,528, 17,621 | 98.83 / 96.18, 1,316,691, 15,541 | 66.89 / 57.48, 382 |
| A (`g8`) | 98.50 / 90.24, 1,249,383, 19,047 | 99.52 / 96.29, 1,318,265, 6,314 | 62.50 / 53.32, 355 |
| `h1` | 97.97 / 90.73, 1,256,128, 26,070 | 98.37 / 96.53, 1,321,481, 21,941 | 63.68 / 55.48, 371 |
| `h2` | 97.97 / 90.77, 1,256,682, 26,095 | | 63.24 / 56.73, 380 |
| B (`h3`) | 98.46 / 90.85, 1,257,803, 19,728 | 98.37 / 96.57, 1,322,066, 21,897 | 66.48 / 57.40 (the 0.5 run read at 0.7), 381 |

`--detail 256`: A is `--fast`'s pair list (IMGS-ALL2, found corpus). B on
shared records: IMGS-ALL2 98.60 / 83.50, 1,156,019, 16,450 (`--fast` 98.58
/ 83.58, 1,157,202, 16,660); IMGS-ALL 98.48 / 91.56, 1,253,502, 19,305
(`--fast` 98.93 / 91.37, 1,250,912, 13,475: bird1/2 847 -> 6,586 and
Floriana1/2 2,849 in, on planes that differ by rounding).

Cost against HEAD:

| | A | B |
|---|---|---|
| IMGS-ALL2 cold (5 min rests) | 2,243-2,263 / HEAD 2,327-2,372; wall 316-320 s / 323-329 | 2,822 / 2,338 (+21%), one pair |
| IMGS-ALL cold | 2,890, 2,942 / 3,053, 3,091 | |
| found corpus cold | 616, 636 / 804, 908 (pairs 2,657 / 2,954, unlabelled) | |
| DISC21 `r10k` cold | 738, 733 / 963, 1,006 | |
| Desktop cold | 44-45 / 49 | |
| IMGS-ALL2 cached | 733, 729 / 818, 831 | 1,203 / ~820 (+47%) |
| IMGS-ALL cached | 770 / 925 | 1,317 / 925 (+42%) |
| DISC21 shard 0 cached | 1,992 / 3,340 | 4,893 / 3,340 (+46%) |
| `--detail 256` cold, IMGS-ALL2 | 998, 996 / `--fast` 936, 993 | 1,096 / 991 (+11%) |
| `--detail 1024` cold, IMGS-ALL2 | | 4,512 / 3,640 (+24%); 98.37 / 91.44 against 98.59 / 90.56 |

Containment (pairs of the step below not stated; `val-g8/lad-*.contain`,
`h3/*.contain`):
- A: 512 -> 1024 -> 2048 -> 4096 lose 0 on both corpora. 256 -> 512 loses
  16,688 pairs on IMGS-ALL2 (8,950 true, 7,182 false) and 13,257 on IMGS-ALL
  (3,269 true, 9,791 false). HEAD's `--fast` -> default loses 13,304 true on
  IMGS-ALL2.
- B: 256 -> 512 loses 0 on both.

Small families (`families.py`, total joined, lower is better): A 60.3 /
58.0 on IMGS-ALL / IMGS-ALL2, HEAD 65.3 / 63.0. B not run.

## Why neither ships

A costs nothing and loses DISC21: 33 of HEAD's true copies gone, 6 gained.
Its 512 step verifies 2.4 M candidates where HEAD's verifies 7.2 M and has
no deep second look. Full windows bring back 15 (`disc/log`, `w1`); a second
look there none while a picture whose 256 level is full has no 512 feature
at all (the shared budget). And 256 is not contained, nor its cache shared.

B has the properties and HEAD's accuracy, and is the 256 scan plus a whole
512 scan. Each cheaper step between them lost something measured:
- skipping at 512 what 256 judged (`h1`, `h2`): the copies are found (380
  of 382 with the second look) but stated with the 256 verdict's evidence,
  median 75 aligned points against HEAD's 118 on the same true copies, so
  µAP stays at 63.2;
- no second look at 512 (`h1`): 21 of the 22 copies HEAD has and `h1` lacks
  were never verified (`h1/disc-dump`).

Also tried in A and dropped: ending the 256 step's propagation early when a
512 step follows (no wall gain, -353 / -1,820 true pairs); the 512 step's
words added to a 256-trained vocabulary (-1,210 true pairs for 27 CPU-s);
the 256 step's clustering beside the 512 step's index build (no gain, the
machine is saturated).

## What it says about the knob

A scan at one size is not a superset of the scan below it, so containment
has to be built by keeping the coarser analysis and adding to it. That is
cheap above the default, where a finer step may trust what the default
decided, and its cost falls on whoever asks for it. Below the default the
step on top *is* the default: either it trusts 256's verdicts and delivers
less than today's default (A), or it is a whole scan on top of another (B).
The 2026-11-01 work-size ladder met the same wall ("buys the knob by
sacrificing the default"). Sizes as scans of their own with a cache each is
`--work-size`, which found less than 512 at 1024 and 2048 for about twice
the cost (HISTORY.md, *`--detail`*, the cold ABBA table).

## Files

`scripts/` (`val4.sh` the validation of A, `go11.sh` a first look at a
build, `go13.sh` the cold steps, `disc.sh`, `ab.sh`, `contain.py`,
`dscore.py`), `val-g8/` (A), `h1/`, `h3/`, `steps/` (cold, `h3` against
HEAD at 512, 256, 1024; stopped there), `disc/` (A's switches on DISC21),
`run.log.txt`, `baab.log.txt`, `ab-*.txt`. Reports, pair lists and binaries
were not kept.
