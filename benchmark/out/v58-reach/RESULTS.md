# v58-reach: what the scan's size keeps from being found (2026-11-08)

User's brief: (1) a verdict's seeds thinned by the run's vocabulary and (3) `-k`
making the scan lazier on bigger corpora are the two limits to remake; recall may
not fall, precision may where it is correctness, CPU "a bit, under control".

Base = HEAD (0.82.0). DISC21 shard 0 = all 50,000 dev queries `--against`
`references_0` (50,000), 529 true copies, `--min-pixel-correlation 0.5`, cached
(`~/Documents/DISC21/cache57`), scored from the report (`dscore.py`: points +
correlation / 2, bars 0.5 and 0.7).

## 1. Where base loses DISC21's true copies (`classify.py`, `why.py`, trace)

Scaffolding `IMGFP_TRACE` (a list of pairs): each pair's rank in each file's whole
ranking, how deep each file asked (`window`), its shared words and verdict.

| of 529 true copies | n | pass when the two files are scanned alone |
|---|---|---|
| stated | 374 | |
| **ranked past the window** (never verified) | **91** | 40 |
| verified, verdict short of the bars | 50 | 9 |
| no shared word at all | 10 | 0 |
| verified, under 3 shared words | 4 | 1 |

So `-k` (point 3) is where the folder costs most. The missed ranks are mostly
150-2,000 and many far beyond (to 82,000 of ~90,000 touched); a missed copy scores
~0.005 against a top of ~0.16 (median), so no absolute score bar can reach them
and a bigger `-k` cannot either. Several would be stated as they are if asked
(Q02160 83 points, Q26536 89, Q17716 70, Q08853 72).

## 2. Hamming embedding over the fixed vocabulary: rejected

Retrieval by the fixed 65,536-leaf vocabulary (`fixed.rs` of v57) with a 64-bit
signature a descriptor (`src/he.rs`: random signs, Walsh-Hadamard, less the
leaf centre's), a posting met within H bits: the matching criterion is then the
pair's own, at one fineness whatever the corpus. IMGS-ALL2 at H 24: recall 90.14%
(base 90.06), precision 98.59% (98.62), but the query read 18.4 G postings (25%
passing) for 33 k files, 93 s wall against base's 5.6. The signature is weak at
this scale: pairs within descriptor distance 143 (the true-correspondence p90,
`WORD_FLOOR`) are within 28 bits at p90 on the found corpus, and so are 26% of
pairs beyond 320. DISC21 shard 0 at H 26: true copies ranked *worse* than base for
417 of 529 (better for 68); of base's 126 ranked past 150, 5 come under 150; query
680 s wall against 42. Stopped.

## 3. Weak geometric consistency reranks them

Each shared word between two files votes for (rotation in 16ths of a turn, scale
ratio in octaves); a candidate's peak is the most of the asker's keypoints in one
bin. Reranking each query's top 3,000 (bag of words) by the peak (trace, fixed
seeding build, retrieval 3 paths): of the 126 true copies base ranks past 150, 32
come into the top 150 (18 of them with a verdict, 18 passing alone); 16 that base
has in its top 150 drop out. So geometry is an *addition* to the window, never a
replacement.

## 4. The geometric look

`IMGFP_GEO=M:W` (direct pass, every file's first window) and then `IMGFP_GEO2=M:W`
(inside the second look, so only `Scope::re_askers`, files the direct pass anchored
fewer than twice): past the first `-k` of its ranking, down to depth M, the
candidates with a peak of at least 3 (what a transform hypothesis needs) are
verified as they are, best peak first (then the better ranked), W of them.

| DISC21 shard 0 | true / false 0.5 | µAP 0.5 | true / false 0.7 | µAP 0.7 | CPU-s |
|---|---|---|---|---|---|
| base (HEAD) | 374 / 380 | 65.86 | 312 / 87 | 56.66 | 2,623 |
| fixed seeding, retrieval 3 paths | 377 / 548 | 65.43 | 315 / 89 | 57.14 | 3,250 |
| + direct look 1000:150 | 390 / 920 | 66.16 | 324 / 100 | 58.52 | 3,927 |
| + second look 1000:150 | 390 / 880 | 66.16 | 324 / 96 | 58.52 | 3,922 |
| fixed, retrieval 1 path + direct look | 352 / 471 | 61.86 | 291 / 65 | 53.35 | 2,068 |
| **HEAD + second look 1000:150** | **392** / 546 | **67.42** | 323 / 98 | 58.40 | 3,143 |
| HEAD + second look 1000:10 | 386 / 425 | 67.13 | 320 / 89 | 57.94 | 3,192 |

The look adds and never takes away: against the same build without it, 0 true
copies lost at 0.7, +9. HEAD's own seeding does as well as the fixed seeding here:
the copies the look reaches are verdicts base already makes when asked. The
fixed seeding's 5 losses at 0.7 (all at correlation 0.70-0.78) are its own.

Where the look's true copies come from (`geo2log.txt`, HEAD, 1000:150): 19 new,
at positions 0 (eight), 3, 3, 3, 6, 8, 20, 23, 42, 44, 47, 70 of the geometric
order; peaks 3-9. 10,232 extras stood, 9,537 of them query-to-query (frames of one
video and the like, which DISC21 does not score).

IMGS-ALL2 (cached, single runs, fixed seeding build): retrieval 3 paths 90.27% /
98.61%; direct look 90.29% / 98.60%, +36% CPU (every file asks; on a corpus of
copies the window is full of rejected look-alikes); second look 90.29% / 98.61%,
CPU within the runs' noise (2,019 re-askers, 126 k geometric candidates).

## 5. Cost of the look, and (1)

(1), the seeds: the only seeding found that is the pair's own is v57's fixed
vocabulary. On DISC21 it nets +3 true copies (+8, -5 at 0.7) and needs retrieval
at 3 paths (at 1 path, even with the look, 352 against base 374); with it, +24%
CPU on DISC21 (3,250 against 2,623), +23% on IMGS-ALL2 (v57 section 6). The
copies the look reaches are verdicts base's own seeding already makes when asked.

## 5b. Cost of the look

Found corpus (9,643 files, 8,881 re-askers), cached, prof: the second look
291 thread-s; the geometric look added 95 (scoring 80, the deeper query 8,
verifying 6). Scoring was `index::shared` and a sort a candidate (11 µs); a scorer
of its own (`Asker::peak`: the asker's words as a bitset, the candidate's list
walked forward) against each candidate's best word a keypoint only (`quantise_two`,
from the same descent; the asker keeps its three paths) takes it to 44.

DISC21 shard 0, HEAD + the look in the second look, W 10, the fast scorer
(single cached runs; base 2,623):

| depth : W | true / false 0.5 | µAP 0.5 | true / false 0.7 | µAP 0.7 | CPU-s |
|---|---|---|---|---|---|
| base | 374 / 380 | 65.86 | 312 / 87 | 56.66 | 2,623 |
| 500 : 10 | 381 / 402 | 66.66 | 317 / 87 | 57.48 | 2,951 (+12.5%) |
| 1000 : 10 | 383 / 410 | 66.84 | 319 / 87 | 57.79 | 3,047 (+16%) |
| 1000 : 10, `index::shared` scorer | 386 / 425 | 67.13 | 320 / 89 | 57.94 | 3,192 |
| 1000 : 150, `index::shared` scorer | 392 / 546 | 67.42 | 323 / 98 | 58.40 | 3,143 |

The main corpora, HEAD against HEAD + 500 : 10 (cached, single runs): IMGS-ALL2
recall 90.06 -> 90.07%, precision 98.62% both, 770 / 797 CPU-s (2,024 re-askers,
12,468 geometric candidates); IMGS-ALL TP +2, FP identical (15,541), same 59
merges, 940 / 998 CPU-s (1,354 re-askers). Not run: small families, HOLI,
`pairs.py`, cliffs, ABBA cost.

Scaffolding: `geo-scaffolding.patch` (on HEAD; `IMGFP_GEO2=M:W`, `IMGFP_GEO2_LOG`);
`fixed-he-scaffolding.patch` (v57's fixed seeding with `he.rs`, `IMGFP_GEO`,
`IMGFP_LAZY`, the trace; needs `../v57-fixed/fixed.vocab.zst` as `src/`).

Rejected on the way: asker words made when asked (`IMGFP_LAZY`, to save the fixed
seeding's 0.9 GB on DISC21): remade every round, IMGS-ALL2 1,007 -> 1,554 CPU-s.

## 6. Depth and budget, swept (2026-11-08)

One run a shard with the look at depth 3,000, *every* candidate with a peak >= 3
verified and logged (rank, peak, standing, correlation; `IMGFP_GEO2_LOG`), and
the first window's peaks (the file's null); any (depth, rule) replayed offline
(`replay2.py`, `replay3.py`, `replay4.py`). The replay reproduces the real runs
(1000:10 predicted +9 / +7, measured 383 / 319 against 374 / 312; null rule at
750 predicted +8 / +6 and 70,856 verified, measured 382 / 318 and 70,856).
Shard 1 = the dev queries against `references_1` (downloaded for this), base
318 / 280 true, 396 / 93 false.

**The budget is derived**: a deeper candidate is verified when its peak beats the
highest peak among the file's own first window (`-k` best by words, which held
nothing that stood: what an unrelated picture reaches *for this file*), and has
the 3 a hypothesis needs. No constant. Against fixed budgets it verifies a tenth
to a hundredth as many and states the fewest false library pairs; it misses the
true copies of files whose own window already peaks high (repetitive texture:
nulls of 5-273), which only a large fixed budget reaches, with 4-20x the false
pairs.

**The depth is measured**, the null rule, gain at bars 0.5 / 0.7 (true copies
base does not state):

| depth | shard 0 | shard 1 | both | found corpus CPU (cooled ABBA) |
|---|---|---|---|---|
| 300 | 3 / 2 | 4 / 2 | 7 / 4 | +6.0% (318 / 318 -> 337 / 337) |
| 500 | 6 / 4 | 4 / 2 | 10 / 6 | |
| 700 | 8 / 6 | 4 / 2 | 12 / 8 | |
| 750 | 8 / 6 | 4 / 2 | 12 / 8 | +14% (319 / 325 -> 370 / 365) |
| 1,000 | 8 / 6 | 5 / 3 | 13 / 9 | +16.5% (317 / 326 -> 367 / 382) |
| 1,500-3,000 | 8 / 6 | 5 / 3 | 13 / 9 | |

Base on both shards: 692 true at 0.5, 592 at 0.7. Nothing comes past 1,000 on
either shard; ranks 150-300 hold half of it. Adaptive depth (read the next block
of 150 while the last held a candidate beating the null) fails: true copies come
alone, not in runs (shard 0: 3 / 2 at +1.2%, 8 / 6 only with two empty blocks
allowed, at the cost of a fixed 750).

**Where the cost is**: scoring, not verifying. Each lonely file scores
depth - 150 candidates plus its 150 for the null, ~6 µs each on the found corpus
(memory-bound: each is another file's list; an L1 filter for the asker's words
changed nothing, 319 / 325 -> 370 / 365 both builds); verifying a candidate is
~90 µs but the null rule verifies few. The cost is close to proportional to the
depth. Every reachable true copy (any rule, depth 3,000): shard 0 14 / 11, shard
1 16 / 10.

## 7. Shipped (0.83.0): depth 1,000, the null rule, clean build (`img-fp-ship`)

Against HEAD (0.82.0) on caches HEAD's build filled; `battery.sh`, outputs in `bat/`.

| | HEAD | 0.83.0 |
|---|---|---|
| DISC21 shard 0, true / false at 0.5 | 374 / 380 (µAP 65.86) | **382** / 393 (66.89) |
| DISC21 shard 0, true / false at 0.7 | 312 / 87 (56.66) | **318** / 88 (57.62) |
| DISC21 shard 1, true / false at 0.5 | 318 / 396 (66.49) | **323** / 403 (67.37) |
| DISC21 shard 1, true / false at 0.7 | 280 / 93 (60.28) | **283** / 95 (60.86) |
| true copies lost (either shard, either bar) | | **0** |
| DISC21 CPU-s, cached single runs | 2,623 / 2,686 | 2,856 / 2,883 (+9% / +7%) |
| DISC21 peak RSS | 3.35 GB | 3.45 GB |

As the replay of section 6 predicted (+8 / +6, +5 / +3). IMGS-ALL and IMGS-ALL2:
F1, precision, recall, pairs, merges identical (405 / 363 deeper candidates
verified); keep rule identical (`keep.out`); `pairs.py` identical (loss 26);
`families.py` identical but HOLI-DEV n0 248 -> 249 sibling joins; HOLI-TEST 711.0
both; IMGS-AGAINST identical; `--detail` 512 / 1024 on IMGS identical to HEAD's,
0 lost 512 -> 1024. Found corpus cost (section 6, cooled ABBA): +16.5% CPU.
The found corpus's report is the scaffolding's to the pair.
