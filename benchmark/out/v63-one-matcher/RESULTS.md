# v63: one matcher (the default is the one-step case of `--detail`'s loop)

Builds: base `img-fp-head` (4d56d07; source differs from 3741610 by comments
only), new = `match_all` as the step loop, `match_by_detail` gone, a finer
step's state one `Finer` on the `Pass`, and step 0's verdicts taken as the
default takes them (`new2`: the finer step's `known` map first-wins too).
Cached runs, `--dump`, `-t 8`. Scripts `run.sh` (Desktop and IMGS-ALL 512;
its IMGS-ALL 1024 runs were stopped: not in the cache, ~1 h cold),
`run3.sh` (IMGS-ALL2, Desktop with new2), `cmp.sh`, `contain.py`.

## Default (one step): byte-identical

Report (but `runtime_seconds`) and dump identical: IMGS-ALL 512 (1.83 M
dump rows), IMGS-ALL2 512, `derived/Desktop` 512.

## `--detail`: not identical, on purpose

The second look can verify one pair under one variant from both ends. The
default keeps both verdicts and the claim graph takes the first met
(`claims::unique_pairs`). The old `match_by_detail` passed step 0 through
`merge_best`, which kept the better: its first step was not the default's
matching, as its comment said it was.

- Desktop 1024 / 2048: same 63 groups, same pair set; 6 pairs' evidence
  differs (points, correlation of the other duplicate verdict).
- IMGS-ALL2 1024: base 1,253,843 TP / 17,898 FP, new2 1,253,812 / 17,898
  (F1 0.944 both; 33 pairs only base, 1 only new2).
- **Containment 512 -> 1024 (IMGS-ALL2): base loses 1 true pair (and 1
  pair) the default states; new2 loses 0.** The promise "a finer detail
  states everything the default states" now holds by construction.

Peak RSS and wall level (IMGS-ALL 512 4:20 / 4:24, IMGS-ALL2 1024 3:09 /
3:11, 512 2:38 / 2:42; one run each, not a cost measurement).
