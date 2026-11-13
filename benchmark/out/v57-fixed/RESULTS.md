# v57-fixed: a verdict's correspondences from a fixed vocabulary (2026-11-07)

Goal (user's): one pair, one verdict, whatever else is scanned, for at most ~5% more CPU
(v56's hashed seeding, `IMGFP_SEED`, got there for +45-50%).

Scaffolding: `scaffolding.patch` (env switches `IMGFP_SEEDVOCAB`, `IMGFP_SEED_PATHS`,
`_STRONG`, `_R`, `_MUTUAL`, `_GATE`, `_FUSED`, `_ASYM`, `_CANON`, `IMGFP_RETRIEVE_PATHS`,
`IMGFP_RETRIEVE_FIXED`, `IMGFP_TRAIN_SEED`; `seeds.rs.scaffolding`). Runs in the session
scratchpad (`v57/`); every A/B below reads one cache filled by the measured build (HEAD's
analysis, `c.db`), never the old `DISC21/fixtest` caches.

## 1. The idea

Seed correspondences from words of a vocabulary trained **once**, on photographs no corpus
holds (DISC21 references R000000-R009999, 10,000 pictures, ~6 M descriptors), shipped with
the program. Then which keypoints of two files are compared depends on the two files alone.
The run's own vocabulary still proposes candidates (retrieval); it no longer touches a
verdict. Tree: 16 x 16 x 16 x 16 = 65,536 leaves, sample 1.5 M, 12 iterations, 9.2 MB
(6.9 MB zstd), trained in 21 s.

## 2. What it took to make it cheap (IMGS-ALL2 cached unless said; base 878 CPU-s, recall 90.06%)

| step | recall | CPU-s | note |
|---|---|---|---|
| 3 paths, bound D 320 (`f65k`) | - | found: verify 6 -> 27 s | 255 shared pairs / verdict, 3x as many 3-point hypotheses |
| only correspondences <= 143 propose (`p2s143`) | 89.84 | 1,057 | 143 = `WORD_FLOOR`; warps lose |
| 1 path, all <= 320 propose (`p1all`) | 89.40 | 821 | no new constant; losses: pinch, aspect, shear |
| 1 path unbounded (`p1unb`) | 89.40 | 993 | the bound is not the loss: path boundaries are |
| 2 paths (`p2all`) | **90.29** | 1,167 | all recovered, +33% |
| guided gated at the bar (`p2g`) | 88.91 | 1,067 | `crop_micro` -30, `embed_tiny` -18: rejected |
| fused table pass (`fus`) | = | = | identical pairs to the merge; not the bottleneck |
| + mutual nearest (`p2m`) | **90.34** | 1,134 | no constant; fewer coincidental hypotheses |
| asymmetric: asker 2 paths, candidate 1 (`a21m`) | 89.99 | 977 | verify at base's 71 s |
| canonical orientation, asker soft over 2 orientations (`ca21`) | 89.91 | 1,029 | mirrored / inverted words free |
| + retrieval 2 paths (`ca21r2`) | 89.88 | 954 | |
| + retrieval 1 path (`ca21r1`) | 89.73 | 889 | **chosen** |

Canonical hard on both sides (`cp2`, `cp1`): 89.48 / 88.16 (a random key flips close pairs'
orientation); soft on the asker side recovers it. Retrieval on the fixed words themselves was
not run: 65 k words over 11.5 M descriptors is ~180 a word, the postings a query reads grow
~50x.

Found corpus (9,643 files, unlabelled, full second look; single runs): base 416 CPU-s,
`ca21r2` 385, `ca21r1` **304**: nearly every file is re-asked mirrored there, and the fixed
words seen turned cost no descent.

## 3. The chosen design (`ca21r1`), single runs before the clean build

- IMGS-ALL (cached): F1 0.9748 -> 0.9749, precision 98.83 -> 98.85%, recall 96.17 -> 96.17%,
  cross-family 12,332 -> 12,235, seed pairs with false pairs 83 -> 50, merges 59 -> 46.
- IMGS-ALL2: F1 0.9414 -> 0.9394, recall 90.06 -> 89.73%, precision 98.62 -> 98.56%.
- Small families (`families.py`, total joined, lower is better): IMGS-ALL 65.3 -> 60.3,
  IMGS5 40.7 -> 38.0, IMGS-ALL2 63.0 -> 56.3, HOLI-DEV 968.7 -> 952.0; recall within
  +-0.6 (IMGS-ALL2 n2 91.30 -> 90.79).
- One pair, one verdict (`invariance.py`, originals alone against among 9,285 found
  photos): IMGS-ALL 14 / 14 same evidence (base 10 of 14 met in both), IMGS-ALL2 14 / 14
  (10 / 14), HOLI-DEV 220 / 240 (146 / 219), IMGS5 7 / 8 (5 / 9). Every remaining
  difference is propagated or met in one setting only (retrieval, claim graph).

## 4. The clean build (`src/fixed.rs`, no switches)

Differences from `ca21r1`: a tie between two orientations reading alike along the key
goes to the lesser oriented bytes (it went to the lower orientation number, which is not
mirror-invariant: `turned_words_are_the_words_of_the_turned_features` failed on it); the
vocabulary retrained with that rule by `fixed::tests::train_fixed_vocabulary`, which
reproduces `src/fixed.vocab.zst` byte for byte; the asker's words found in the candidate's
by walking forward (both are in word order) instead of a word -> slot table (1 MB, read at
random: `seed` 162 -> 103 thread-s on IMGS-ALL2, output identical); the orientation
permutations read off `Features::seen_as`, so `sift.rs` and the analysis version are
untouched (a `pub(crate)` there changed `IMGFP_ANALYSIS` and analysed every file again).

**Cost, cached, cooled ABBA** (`abba.sh`: k10temp idle + 3 C, >= 60 s apart, kept
matching removed before each run; base = HEAD):

| | base CPU-s | new CPU-s | |
|---|---|---|---|
| IMGS-ALL2 (BAABBA) | 777 / 760 / 783 | 689 / 717 / 757 | **-6.8%** |
| found corpus, 9,643 files, full second look (ABBAAB; the build before the forward walk) | 332 / 332 / 336 | 251 / 251 / 253 | **-24.5%** |

(First IMGS-ALL2 round, before the forward walk: 771 / 781 against 827 / 827 / 855, +7.8%.)
Profile, IMGS-ALL2 thread-s, base -> new: quantise 140 -> 58, the second look's quantise
39 -> 17, query 30 -> 15, cand:rank 11 -> 4, guided 242 -> 207, best_transform 63 -> 50,
seeding (shared + correspond -> `seed`) 89 -> 103, fixed words 0 -> 15; verify:direct
559 -> 515. Binary 12.0 -> 19.0 MB (the vocabulary); crate 7.2 MB packed.

**Accuracy, cached against the base's cached runs:**

- IMGS-ALL2: F1 0.9414 -> 0.9399, precision 98.62 -> 98.57%, recall 90.06 -> 89.82%,
  perfect transforms 12 -> 13; worst pinch -12, shear -5, aspect_distort -4, crop_micro
  -4; best video_call_frame +3. Merges past 300: Fountain3/5, bird1/2 as before,
  Floriana3/4 372 -> 219 (under), Mural1/2 187 -> 308 (over; 0.73.0 called 327 fine by
  eye).
- IMGS-ALL (cold, the base cached): F1 0.9748 -> 0.9730, precision 98.83 -> 98.40%,
  recall 96.17 -> 96.22%. The whole precision difference is IMGS5 bird1/2, two frames of
  one burst (CLAUDE.md: "too near and may merge"), 851 -> 6,849 false pairs; without it
  cross-family 11,481 -> 11,394. The scaffolding's `ca21r1` had 864 there: the burst is
  on its edge.
- Keep rule (`suggest_score.py`): content lost 0 -> 0 (IMGS-ALL), 1 -> 1 (IMGS-ALL2, the
  same `phone_screenshot`); original or lossless copy kept 358 -> 356 / 350 -> 356; faithful
  copy 364 / 364 both; strict line 64 -> 83 / 39 -> 23; picture only in an edit or
  composite 4 -> 5 / 5 -> 3. Not traced. `pairs.py` not run.
- Small families (total joined, lower is better): IMGS-ALL 65.3 -> 62.3, IMGS5 40.7 ->
  38.7, IMGS-ALL2 63.0 -> 58.3, HOLI-DEV 968.7 -> 953.7; recall -0.7 to +0.3.
- One pair, one verdict (`invariance.py`): IMGS-ALL 15 / 15 stated alone and among the
  found photos with identical evidence (base 10 of 14 met in both), IMGS-ALL2 15 / 15
  (10 / 14), HOLI-DEV 215 / 233 (146 / 219), IMGS5 7 / 8 (5 / 9).
- `--detail` containment, IMGS (5,641 files), 512 -> 1024: 0 true pairs lost, 1,404 added.

Not run: HOLI-TEST, DISC21, cliffs, `pairs.py`, IMGS-AGAINST, cold-run cost.

## 5. What is still folder-dependent

Retrieval: which pairs are *asked* still depends on the run's vocabulary and on `-k`
(the remaining "met in one setting only" rows above). Among pairs asked, the direct verdict
is the pair's own. The claim graph's admission and propagation read other files by design.

## 6. Against the per-folder vocabulary on every measure (2026-11-07, later)

User's bar: recall and precision level or better everywhere. Scaffolding (uncommitted, in
the tree): `IMGFP_AP` asker paths (2), `IMGFP_OP` candidate paths (1), `IMGFP_RP` retrieval
paths (1), `IMGFP_RLAST` retrieval soft at the last level only. Cached, `c.db`, single runs.

| IMGS-ALL2 | recall | precision | cross-family |
|---|---|---|---|
| base | 90.06 | 98.62 | 13,354 |
| clean build (AP 2, RP 1) | 89.82 | 98.57 | 13,898 |
| RP 2 / RP 3 | 89.99 / 89.98 | 98.61 / 98.60 | |
| AP 3, RP 3 / RP 2 | 90.27 / 90.22 | 98.61 / 98.61 | 13,515 / 13,511 |
| AP 4, RP 2 | 90.38 | 98.59 | 13,751 |
| OP 2, RP 3 | 90.20 | 98.60 | |
| AP 3, RP 1 (**a3r1**) | **90.08** | 98.57 | 13,991 |
| RLAST 2 / AP 3 + RLAST 2 / 3 | 89.72 / 89.94 / 89.77 | | soft last level does not help |

IMGS-ALL: base 96.17 / 98.83; AP 3 RP 2 96.31 / 98.40; AP 4 RP 2 96.34 / 98.39; a3r1
**96.29** / 98.40 (bird1/2 6,872; without it FP 14,577 against base 14,690).

Cost, cooled ABBA against base (HEAD), CPU-s: AP 3 RP 2: IMGS-ALL2 818 / 664 (**+23%**:
retrieval at 2 paths, quantise +45 thread-s, query +9, 5% more pairs verified), found
313 / 316. a3r1: IMGS-ALL2 716 / 691 / 703 against 728 / 649 / 673 (**+3.0%**), found
243 / 244 / 243 against 320 / 318 / 316 (**-23%**).

Small families (a3r1 against base): joined IMGS-ALL 65.3 / 65.3, IMGS5 38.0 / 40.7,
IMGS-ALL2 61.0 / 63.0, HOLI-DEV 955.0 / 968.7; recall IMGS-ALL n1 +0.19 n2 -0.04, IMGS5 n1
-0.56 (one pair, Shenzhen1) n2 +0.39, IMGS-ALL2 +0.38 / +0.19, HOLI-DEV -0.08 / -0.06.
One pair, one verdict: IMGS-ALL 16 / 16, IMGS-ALL2 16 / 16, HOLI-DEV 210 / 233, IMGS5 7 / 8.

**Where the full-corpus precision goes.** Every sibling pair the fixed seeding adds on the
full corpora (bird1/2, Mural1/2, game2/3, Henares1/2, Seagull1/2) is one base itself joins
when the folder is not ninety copies a seed: base pairs bird1 | bird2 alone at 347 points,
0.971, and joins each of the five in the seeds-alone run and in 2-9 pairs of every one-
and two-copy draw, the same counts as the fixed build. Base's full-corpus figure is its
vocabulary hiding those pairs (the shield of `out/v45-k`); a pair-intrinsic verdict cannot
keep it.

## 7. The battery (2026-11-07 evening; `battery/`, `battery.sh`)

Base = HEAD (`battery/img-fp-head`), new = fixed seeding with the asker at 3 paths, retrieval
1 path (`battery/img-fp-ship`, the tree's switches at their defaults). Cached, `c.db`.

- **pairs.py** (`pairs-compare.out`): loss 26 -> 27, originals deleted 344 -> 346
  (`slide_deck` Shenzhen1, `magazine_spread` KoronasdaruLeeds1: a third file in the folder,
  Shenzhen2's original at 25 points, 0.783, which base pairs too alone at 29, 0.803; and two
  magazine spreads matched through their generated layout at 14 points, 0.732).
- **HOLI-TEST** (`htest-compare.out`): siblings joined 711.0 -> 691.7, pair precision n1
  52.82 -> 54.03%, n2 60.84 -> 62.19%, recall 98.67 -> 98.72%, 96.87 -> 96.91%; different
  scenes 0 both.
- **IMGS-AGAINST** (`against/score.out`): found 4,849 both, content lost 0 both, false library
  claims 71 -> 68, repeat 3,317 -> 3,314.
- **`--detail`** on IMGS (`detail/contain.out`): 512 -> 1024 loses 0 true pairs in both
  builds (base +1,408, new +1,540).
- **Keep rule** (`keep.out`): content lost 0 / 0 and 1 / 1; originals 358 -> 356, 350 -> 357;
  strict line 64 -> 83 (IMGS-ALL), 39 -> 39. All +19 is IMGS `document2`: its original and
  lossless copies deleted for `motion_blur`, which the new build fits at scale 1.005 and
  0.995 where base fitted 1.042 and read 0.906; base already read the blur against the
  lossless WebP at 0.991. A keep-rule weakness the better fit exposes.
- **Cliffs** (`cliff_table.out`): recall lower than base at IMGS-ALL2 points 6-3 (-0.06 to
  -0.07), correlation 0.9 / 0.8 (-0.20 / -0.08), overlap 0.3 (-0.03), `-k` 100 / 50; IMGS-ALL
  correlation 0.9 / 0.2, `-k` 50; higher at every other step (IMGS-ALL `-k` 10: 91.18 ->
  92.81%, FP -9,155). Merges past 300 only in the new build: Mural1/2 (IMGS-ALL2 default and
  points, IMGS-ALL points 6-3), Parish Church1/2 (IMGS-ALL2 overlap 0.5-0.25), game2/3,
  Acueducto1/2, Henares1/2, morminte1/2 (IMGS-ALL correlation 0.4-0.2), png3 | baseball5
  (IMGS-ALL2 correlation 0.3-0.2, two unrelated seeds); only in base: Cemetery3/4,
  ibira1/2, red_rocks_sign1/2, Floriana pairs at `-k` 10-50.
- **DISC21 shard 0** (`disc/score.out`, 529 true copies; base reproduces 0.81.0's shard
  exactly): new at retrieval 1 path true 374 -> **339** at 0.5 (µAP 65.86 -> 60.23), 312 ->
  282 at 0.7. Of the 40 lost, 39 have no verdict at all, and eight run alone give both builds
  the same verdict (Q03233 | R023861 212 points, 0.865): retrieval's loss, not seeding's.
  Retrieval at 2 paths 360 / 492 false, µAP 63.03; at 3 paths 377 / 548 false, µAP 65.43
  (0.7: 57.14 against 56.66). CPU, one cached run each: base 2,623, new 1 path 1,769, 2 paths
  2,489, 3 paths 3,250 (+24%). Proxy at 0.5: 391 -> 374 true (1 path).

**Verdict.** The fixed seeding judges a pair as base does alone (every pair tried) and better
in the small-family and held-out measures, but the CPU it saved came from retrieval at one
path, which DISC21 shows costs recall on heavy edits (-9% of true copies). With retrieval back
at three paths DISC21 recall is level (+3) but false pairs at 0.5 rise 380 -> 548 and CPU is
+24% there (+16-23% on IMGS-ALL2). Neither version meets "no recall or precision lost, <= 5%
CPU".

## 8. The DISC21 false pairs, each scanned alone (2026-11-13)

Question left by section 7: with retrieval at three paths the fixed build states 548 false
pairs on shard 0 at 0.5 against base's 380. Are they pairs base states too when the folder
does not hide them (section 6's finding on IMGS-ALL), or pairs the fixed seeding lets in?

`battery/disc/alone.py`: every false Q | R pair of `head/report.json` and `ship-r3/report.json`
(550 and 386 by this count, 175 in both), each run as a scan of its two files, both binaries
(`img-fp-head`, `img-fp-ship`), `--no-cache -t 1 --min-pixel-correlation 0.5`. Result per
pair in `battery/disc/alone.json` (points, correlation; empty when not stated).

| false at 0.5 in the shard run | n | base states it alone | fixed states it alone |
|---|---|---|---|
| only the fixed build | 375 | 68 | 253 |
| only base | 211 | 64 | 12 |
| both | 175 | 153 | 145 |
| all | 761 | 285 | 410 |

Of the 375, 329 are direct (base alone 57, fixed alone 251) and 46 propagated (11, 2); 49 of
base's 211 are propagated.

**Not the folder hiding them.** 201 of the fixed build's own false pairs are stated by it
alone and by base not even alone: the fixed words seed verdicts the per-folder words do not,
whatever else is scanned. Median 17 points, correlation 0.543; 4 of the 201 reach 0.7.

**It is between 0.5 and 0.7.** Read at 0.7 off the same reports the two are level: false 87
(base) and 89 (fixed, three paths), 21 only the fixed build's, of which base states 10 alone
at 0.7 and the fixed build 15. (µAP at 0.7, section 7: 57.14 against 56.66.)

So section 6's reading ("a pair-intrinsic verdict cannot keep base's figure, which is its
vocabulary hiding pairs") holds for the siblings of IMGS-ALL and not for DISC21 at 0.5: there
thorough seeding passes unrelated pairs at 10-25 points and 0.5-0.6 that sparse seeding does
not reach, in a folder of two as in one of 100,000. Part of base's precision below the
default bar is its seeding, not its acceptance. A rework proposed on the first reading
(fixed seeding, retrieval left at three paths, verdicts kept by content pair) was withdrawn
on this.

Not run: the same at a real 0.7 run; base's own false pairs that it does not state alone
(147 of 211) were not traced further.
