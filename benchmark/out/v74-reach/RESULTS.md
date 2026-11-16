# v74: reaching DISC21's missed copies in the full scan (2026-11-15/16)

Brief (user): of shard 0's 143 missed true copies (bar 0.5, 512), 51 are
stated when the two files are scanned alone; reach them in the full scan,
cost not up much, tested on IMGS-ALL 1-3 too. Base = `img-fp-base` (0.85.0 +
the 0.65 / 5 defaults; v72's `img-fp-new`, v73's binary). Builds carry
scaffolding switched by environment variables (`IMGFP_SEEDS`,
`IMGFP_FULLSEEDS`, `IMGFP_VOTE`, `IMGFP_PF`, `IMGFP_TRACE`, ...).

## 1. Where the 143 are lost (`trace.sh`, `IMGFP_TRACE`)

Each pair's rank in the query's and the reference's whole ranking, and at
each level of the vocabulary tree (100,000 files: depth 6, 1,191,405 leaves,
near the sample cap, so the leaves are about one sample descriptor each):
seeds (keypoint pairs sharing a word), mutual nearest seeds, the geometric
look's peak, and the verdict verification gives from those seeds.
`trace1.out` (ranks), `levels.out`, `verdicts.out`, `agree.out`.

- Of the 51: best rank either way < 150: 13, < 1,000: 24, < 5,000: 37,
  >= 5,000: 14. All 51 queries are re-asked (`askers`).
- The geometric look's peak (rotation x scale bins of shared words) does
  not separate them at any level: it beats the query's null for 3-6 of 51.
- Verification is asymmetric: from the query's side and the reference's
  side the same pair gives different verdicts (Q28214 passes one way, fails
  the centre test the other). Seeds from coarser tree levels make 4 -> 11
  of the 13 near ones pass the bars (`verdicts.out`), but those were
  verified, and several also fail centre / overlap in the run.
- Verification's first step on the leaf seeds (best transform's agreeing
  count) separates them: for the median query nothing ranked 150-5,000
  beats the best count among its first 150, and the true reference does
  for 8 more of the 51 to depth 1,000, 14 more to 5,000 (`agree.out`).
  As traced (descriptors read from the analysis) 1 ms a candidate.

## 2. The geometric look scored by seed agreement (`retrieve::agree`)

`Index::deeper` with each candidate past `-k` scored by `agree`: its seeds
(`index::shared` of the two files' word lists), each read as a similarity
from its keypoints' place, angle and scale (`Shape::pts`, 8 bytes a
keypoint, held with the index), the most distinct asker keypoints another
seed puts within verification's inlier tolerance; verified when above the
null (the most among the first `-k`) and >= 3, as before. Depth 1,000.

DISC21 shard 0, bar 0.5, cached (`disc.sh`, `dscore.py`, `d-*.score`):

| scoring | true | false | µAP | at 0.65 | deeper candidates | gained / lost |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| base (peak) | 386 | 458 | 67.29 | 62.45 | 95,485 | |
| seeds, full word lists | **393** | 481 | **68.06** | 63.22 | 65,325 | +7 / 0 |
| seeds, candidate's best word only | 389 | 467 | 67.69 | 62.95 | 34,982 | +3 / 0 |
| vote (cells), full lists | 387 | 457 | 67.40 | 62.30 | 80,647 | +4 / -3 |
| vote as a filter, then seeds | 393 | 481 | 68.06 | 63.22 | 62,232 | +7 / 0 |

Gained: Q02160, Q03218, Q12458, Q27556, Q29419, Q35225, Q37489.

Cost, found corpus cached, `.matched` removed each run, alternating
(CPU-s user; `cost-v3.out`, `cost-v4.out`):

| | runs | vs base |
| --- | --- | ---: |
| base | 350, 348; 351, 352 | |
| seeds, best word only | 382, 378 | +9% |
| seeds, full lists | 462, 463; 457, 459 | +31% |
| vote filter + seeds | 518, 529 | +49% |

Shared words alone (no counting, `IMGFP_NOAGREE`): full lists 396 / 405,
best word 362 / 362; 23 seeds a candidate against 10. Prefetching the
next candidates' lists: 515, 499 against 523, 519 (a later session, the
machine slower). The found corpus re-asks 97% of its files, so it is the
worst case for this stage.

## 3. IMGS-ALL 1-3 and small families (full lists; `imgs.sh`, `run-full/`)

Against v72-defaults (the same base binary), cached on v71's cache:

| | F1 base | F1 new | traps | CPU-s base / new (single runs) |
| --- | --- | --- | --- | --- |
| IMGS-ALL | 0.9802 | 0.9802 | 3,977 / 3,977 | 941 / 955 |
| IMGS-ALL2 | 0.9566 | 0.9566 | 4,886 / 4,888 | 865 / 868 |
| IMGS-ALL3 | 0.9377 | 0.9378 | 5,706 / 5,707 | 1,024 / 1,060 |

Same seed pairs past 300. `families.py` (`fam-full.compare`): IMGS-ALL,
-ALL2, -ALL3, IMGS5 total joined unchanged (86.0 / 85.7 / 85.0 / 51.7);
HOLI-DEV 1,114.0 -> 1,116.0 (n0 +1, f +1, both siblings; different-scene
joins unchanged).

## 4. Not reached

44 of the 51 remain: 14 rank past 5,000 both ways, others have too few
agreeing seeds against their null within 1,000 (`agree.out`).

## 5. Can any ranking reach the 51? (`oracle.sh`, `oracle.py`, `thumbrank.py`)

Brief (user): all 51, at most +10% CPU on any corpus. Before building,
an oracle (`Index::oracle`, `IMGFP_ORACLE`; `oracle.out`, `oracle1.err`):
for each missed pair's query, every file of the shard scored with the
descriptors themselves, which no index could afford (1,100-2,600 CPU-s for
143 queries), and the true reference's place.

- Query keypoints with a descriptor within T (200-400) in the same cell,
  at the leaf and 1-3 levels up (level 3: 4,096 cells, 1/4,096 of all
  descriptors compared): the reference is in the first 150 for at most 11
  of 51 at any level and T, first for at most 5. Its count is a third of
  the best file's (leaf, 320: 3 against 9; three up: 22 against 60).
- Those matches scored by `agree` (the similarity most of them fit): first
  150 for at most 16 of 51, first for at most 6; first under any of the
  twelve scores 9, first ten under any 15.
- The pair's own seeds, verified: leaf words pass 10 of 51, one / two /
  three levels up within 320: 12 / 17 / 27, every keypoint pair within
  320 / 400 (no tree): 47 / 48. Alone, the two-file vocabulary seeds
  each keypoint with its nearest in the other file whatever the distance.
- A whole-frame thumbnail (16 x 16 grey, the query and its mirror against
  the 50,000 references by correlation): first for 6, first 150 for 11;
  half the 51 are a picture shrunk 2-6x into another (`alone/*/out.json`).

So what tells these pairs is the fitted transform and the pixel check on
all-pairs seeds, which is a comparison of the two files (~3 ms: 600 x 600
descriptors), and no score read off an index of these features puts the
reference where that comparison would be made: +10% is about one such
comparison a file on this shard, and the best oracle ranks 9 of 51 first.

## 6. Outcome (2026-11-16)

Nothing shipped (user's decision): the bar was all 51 at +10%, and section 5
says no ranking over these features reaches them. `scaffolding.patch` is
everything above against the tree (`src/retrieve.rs`, `index.rs`,
`verify.rs`, `pipeline.rs`; switches `IMGFP_SEEDS`, `IMGFP_FULLSEEDS`,
`IMGFP_VOTE`, `IMGFP_PF`, `IMGFP_DEPTH`, `IMGFP_NOAGREE`, `IMGFP_TRACE`,
`IMGFP_ORACLE`); the sources are back to the base. Untried: seeding a
retrieved candidate by its descriptors, for the 13 ranked inside `-k`.
