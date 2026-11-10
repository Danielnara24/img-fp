# v46-matcher: scoring where siblings meet, and guided matching (2026-10-25)

Base 0.72.0 (`img-fp-0.72.0`). Scaffolding build `img-fp-guided`: guided matching
in `verify::guided`, on when `IMGFP_GUIDE_D` (the descriptor bar) is set; off it
writes all 17 reports byte-identical to 0.72.0 (`off/`). All runs cached
(`cache.bin`: v40-keep's plus the found corpus); `cache.bin.matched` removed
before every timed run.

## 1. Scoring: `benchmark/families.py`

`run BINARY OUT --cache C`, `score OUT [-v]`, `compare OUT...`. Per corpus
(IMGS-ALL, IMGS5): seeds alone (n0), seeds + 1 / 2 random variants (three draws
each), seeds among the 9,285 found photos (f), and on request the whole corpus
(`--only full`). Columns: P / R on the subset's truth, joined (seed pairs with any
false pair between their files), orig (their two originals paired directly),
keepX (DELETEs kept by another photograph's file). The headline is **total
joined** = n0 + mean n1 + mean n2 + f. 0.72.0: **IMGS-ALL 27.0, IMGS5 36.7**
(the v45-k figures, same draws).

## 2. What the verification says with no vocabulary (`probe/`)

`src/probe.rs` (test-only scaffolding): 1,478 pairs (`probe/pairs.tsv`: 386
within-set sibling originals, 728 original + SAME/CROP variant, 364 unrelated
originals), every keypoint of A against every keypoint of B (`brute.tsv`).
Passing the default bars: siblings 7 / 300 IMGS-ALL and 8 / 86 IMGS5 (the
seeds-alone answer), unrelated 0, true 94.7% / 90.8%. **Seeds alone is what the
verification itself says; the full corpus hid it.**

## 3. Guided matching

The vocabulary only proposes: the transform `best_transform` finds among its
shared-word matches (>= 3 agreeing) is used to find the correspondences again,
each A keypoint matched to the nearest-descriptor B keypoint within the inlier
tolerance of where it lands, if nearer than D; least-squares refit, twice. So a
verdict no longer depends on how fine the vocabulary is. Full IMGS5, sibling
originals the full run meets: n_in Puerto1/2 60 (brute 75; 0.72.0 n_match 19),
Mural1/2 250 (257), bird1/2 346 (347), Kaper-12/13 47 (60). Not met at all in the
full run (retrieval, not verification): Seagull, Shenzhen, Cemetery3/4, Lizard,
Macizo1/2 ... (`g250/i5-full-dump.csv`). In the report the cluster rules then
refuse most of what is met, which is why "full" still shows 2-3 orig.

Descriptor bar D, total joined (IMGS-ALL / IMGS5), recall n1 / n2:

| D | IMGS-ALL joined | R n1 / n2 | IMGS5 joined | R n1 / n2 | IMGS5 full R |
|---|---|---|---|---|---|
| 0.72.0 | 27.0 | 95.44 / 94.41 | 36.7 | 97.14 / 96.48 | 95.79 |
| 100 | 17.3 | 94.07 / 93.91 | 18.3 | 95.43 / 95.32 | 95.26 |
| 125 | 20.7 | 94.98 / 94.45 | 23.0 | 95.43 / 96.10 | 95.79 |
| 150 | 23.0 | 95.21 / 94.88 | 29.3 | 96.57 / 96.48 | 96.23 |
| 175 | 24.7 | 95.55 / 94.84 | 31.0 | 96.57 / 96.68 | 96.29 |
| 200 | 24.3 | 95.77 / 94.84 | 32.0 | 97.14 / 96.68 | 96.29 |
| 250 | 27.3 | 96.46 / 94.84 | 39.0 | 97.14 / 96.09 | 96.46 |
| 350 | 26.0 | 96.58 / 95.49 | 40.3 | 98.27 / 96.87 | 96.34 |

A ramp, no cliff. 250 is the coincidence bar (p10 of an unrelated keypoint's
nearest neighbour over a whole picture, 248-288); below it the bar separates,
because a sibling's aligned keypoints are far apart in descriptor (median over a
pair 110-171, Mural1/2 84, bird1/2 59) and a copy's are not (median 50, p95 114).

Full IMGS-ALL (0.72.0 / D 150 / D 250): F1 0.9710 / 0.9740 / 0.9766, precision
99.78 / 99.75 / 99.71%, recall 94.56 / 95.15 / 95.70%, cross-family 10 / 102 /
491 (no seed pair >= 300; at 250 Segovia 87, bust 77, Acueducto1/2 75, game2/3
72, docks 66, Acueducto3/4 65: the siblings the vocabulary hid, now as small
families show them; at 150 Segovia 86, the rest single pairs), perfect
transforms 47 / 53 / 56. Keep rule at 150: 0 lost, 296 / 304 (297), 304 / 304,
strict 55 (56), picture only in an edit 4 (3: `turn2` added). IMGS5 keepX
unchanged (2 a subset, 123 full; 128 at 250).

Cost, found mix cached (ABBA, D 150): 0.72.0 297.4 / 301.2 CPU-s, guided
327.1 / 332.9: **+10%**, not yet optimised (B's keypoint grid is rebuilt per
verdict; three rounds whatever the count).

## 4. Separation: what does not do it alone

Over passing pairs (`probe/stats2.tsv`, guided at 250): siblings vs true copies
(mirror variants left out, the probe asks unturned only).
- inlier descriptor median: siblings 59-171, true p50 50 / p95 114; true above
  110: wave_vertical, halftone, pincushion, swirl, scale_nearest, wall_poster.
- residual median / diagonal: siblings 0.0004-0.006, true p95 0.0053;
  photo_of_screen, wave, swirl, keystone, pincushion, barrel, perspective.
- two-way keypoint coverage: siblings 0.06-0.69, true p10 0.43;
  picture_in_picture, wave, keystone, pdf_page, magazine_spread, halftone.
A sibling a step away is a non-affine warp plus changed content; the generated
warps look like it on all three. blk_min: 20.6% of true below 0.7.

## 5. D derived, not fitted (second round)

D's job is "same point or coincidence", not "sibling or copy". Its bar is where a
pair expects one coincidental correspondence: the 1/C quantile of the pair's own
null (distances from A's keypoints to B's keypoints more than 3 tolerances from
where they land), C being the window trials guided matching makes (B keypoints
within the tolerance of a landing A keypoint). `probe/null.tsv` (`PROBE_NULL`):

| pairs | C p50 | D_pair p10 / p50 / p90 | best in-window distance p10 / p50 |
|---|---|---|---|
| unrelated (364) | 511 | 283 / **320** / 351 | 369 / 415 |
| siblings (386) | 554 | 267 / 313 / 351 | |
| true (728) | 842 | 215 / 286 / 328 (repetitive pictures lower) | |

So **D = 320**, the median over unrelated pairs. No score took part. Checked
against guided matches from the brute-force transform (`guided_timing`): unrelated
pairs get 5.6 a pair at 320 and 314 with no bar at D 1000, against a true copy's 289 and 390,
which is why D cannot simply be removed. The points bar and `encloses_centre`
read the inliers, and with no bar every landing keypoint is an inlier.

The sweep extended to the cliff, as total joined (IMGS-ALL / IMGS5), R n1 / n2:

| D | IMGS-ALL | R n1 / n2 | IMGS5 | R n1 / n2 | IMGS5 full R |
|---|---|---|---|---|---|
| 0.72.0 | 27.0 | 95.44 / 94.41 | 36.7 | 97.14 / 96.48 | 95.79 |
| 150 | 23.0 | 95.21 / 94.88 | 29.3 | 96.57 / 96.48 | 96.23 |
| 250 | 27.3 | 96.46 / 94.84 | 39.0 | 97.14 / 96.09 | 96.46 |
| 300 | 28.3 | 96.35 / 95.07 | 39.7 | 98.27 / 96.87 | 96.37 |
| **320** | 26.0 | 96.58 / 95.15 | 40.3 | 98.27 / 96.87 | 96.29 |
| 350 | 26.0 | 96.58 / 95.49 | 40.3 | 98.27 / 96.87 | 96.34 |
| 400 | 25.7 | 96.69 / 95.49 | 42.0 | 98.27 / 96.48 | 96.37 |
| 450 | 26.0 | 96.81 / 95.46 | 40.7 | 98.27 / 96.87 | 96.28 |
| 550 | 29.0 | 96.92 / 95.57 | 39.7 | 97.70 / 96.68 | 96.13 |
| 1000 (no bar) | 29.3 | 96.92 / 95.73 | 40.0 | 97.70 / 96.68 | 96.01 |

**There is no cliff**: seeds pair with no found photo at any D, and found photos
among themselves give 1,703 pairs (0.72.0), 1,355 (150), 2,813 (250), 2,689 (320),
2,583-2,698 (400-1000). Above ~250 the pixel check alone decides. D below 250 is
a descriptor-similarity cut that takes siblings, warps and recall down together
(the ramp in section 3), so it would be fitted. 320 is on the plateau.

Full IMGS-ALL at 320: F1 **0.9773**, precision 99.72%, recall 95.82%, cross-family
412 (Segovia 87, Acueducto1/2 75, game2/3 75, docks 65, Acueducto3/4 62, the rest
single pairs; none >= 300), perfect transforms 58 / 87 (0.72.0 47). Keep rule: 0
lost, **298** / 304 (297), 304 / 304, strict 55 (56), picture only in an edit 3
(3), DELETE 16,656, REVIEW 3,152.

## 6. Cost

Profile (`--features prof`, slots `guided` and `#guided:*`), found mix, D 150:
282,076 calls, 1.3 rounds a call (66,004 of the 84,428 later rounds found the same
matches as the round before). In the heap, single thread (`guided_parts`), one
round on an unrelated pair costs ~13 µs for B's grid, ~40 µs for walking 3x3 cells
for ~460 landing keypoints (1,600-1,800 keypoints looked at), ~15 µs of distances.
Rewritten: the three cells of a row are one run of the row-major grid, positions
are stored in cell order, nothing is allocated, and the rounds stop when a round
finds the matches of the one before (its fit is the transform they were found
at, so the result is the same). 17 reports byte-identical to the first version
(`g150b`). End to end it gains little (ABBA, CPU-s, old / new / new / old: 332.0 /
328.4 / 326.2 / 333.4).

At D 320 (ABBA, off / on / on / off): **305.8 / 356.2 / 357.0 / 303.7, +17%**. The
profile shows `guided` at 68 s (49 at 150) and `pixel_check` at 1.3 -> 12 s:
coincidence-level hypotheses with 3-9 vocabulary agreements are refined and
reach the pixel check. About two thirds of the added cost is in the second look.

**Gated** (`IMGFP_GUIDE_GATED`): refine only a hypothesis that already clears the
points it will be held to (10, or 3 when frame-on-frame), i.e. what 0.72.0
would read at all. ABBA: off 302.1 / 304.2, gated 312.8 / 313.9, **+3.4%**.
Small families: joined identical (26.0 / 40.3), recall lower (IMGS-ALL n1 96.58 ->
95.66, n2 95.15 -> 94.72; IMGS5 n1 98.27 -> 97.71, full 96.29 -> 95.86). Full
IMGS-ALL: F1 0.9722, precision 99.75%, recall 94.81%, cross-family 345, keep rule
the same as ungated. The cost: a hypothesis the vocabulary has thinned below 10
is not refined, so the corpus dependence returns at the gate (it showed
nowhere in the small families).

**The stack against 0.72.0** (`cost5.sh`, `cost5.out`; guided D 320 + `-k`
counting rejections + shared-part rule v3, cached, kept matching removed,
ABBA, CPU-s user+sys): found mix 287.8 / 284.3 base, 330.3 / 331.5 stack,
**+16%** (wall 41.8 / 39.5 -> 45.5 / 49.4); full IMGS5 78.0 / 77.7 base,
131.1 / 133.4 stack, **+70%** (wall 12.3 / 12.1 -> 19.5 / 19.4). Clocks
2,040-2,100 MHz but one stack run at 2,500 and one at 1,750, same CPU-s.
Matching only: a cold run adds the analysis, the same in both.

**Where IMGS5's +70% goes** (`prof5.sh`, `prof5/`; `--features prof`
build of HEAD, layers switched on one at a time, cached, thread-time):
CPU-s 81.8 / 81.0 all off, 126.5 guided, 140.0 guided + `-k`, 139.7 /
138.6 the stack. Guided **+45 s**, all in its own slot (`guided` 45.5 s,
`verify:direct` 37 -> 84); `pixel_check` only 16-17 -> 18 (the found mix's
1.3 -> 12 is not repeated: IMGS5's candidates are copies, already
pixel-checked). `-k` **+13.5 s**: candidates verified 487,481 -> 865,649
(8,762 windows more in 5 rounds), `query` 1.5 -> 6.9, `shared` 6.3 -> 9.1,
`verify:direct` +7, `cand:rank` +1. The shared-part rule: none measurable.
Why the share is larger than on the found mix: IMGS5's base is small, its
second look nearly idle (257 files re-asked; on the found mix nearly every
file is, `variant:pass` 329 s of thread-time), so 45 s of refinement over
487 k direct candidates (~93 us each) is half again its matching.

## 7. Siblings against warps (second round): still no separator

Over pairs passing the default bars (guided at 320; 14 siblings, 683 true):
- **Local alignment** (`PROBE_LOCAL`, `verify::pixel_check_field`): a smooth
  correction on a 12x12 lattice, each node's affine from matches within a sixth
  of the side, grown three times, then the pixel check through it (checked:
  through the plain affine it gives the shipped `blk_min` exactly). Warps gain
  (pincushion worst block 0.24 -> 0.74, swirl 0.77 -> 0.92, wave mean 0.73 ->
  0.82), composites lose (the lattice extrapolates past the embedded picture:
  pdf_page 0.66 -> 0.32, slide_deck, browser_window), siblings do not move
  (bird 0.73, Segovia 0.65 -> 0.72). Growth is bounded: guided matches exist
  only within the tolerance of the global transform.
- **Leave-one-out consistency** (`PROBE_LOO`): each match against an affine
  fitted to its 8 nearest matches, residual over its keypoint's scale. True
  p50 0.07, p90 0.23; siblings 0.09 (bird) to 1.52 (field). The camera-moved
  siblings stand out (field, sea_photo 0.61, Henares 0.57, Seagull 0.56), but
  `wave_vertical` reads 0.57, and the nearest siblings read like copies: bird 0.09,
  Mural 0.15, Cemetery 0.15, Fountain 0.18.
- Mean block agreement still separates best: siblings 0.71-0.97 (median 0.84),
  true p10 0.90. That is the correlation bar the user set at 0.7.

The nearest siblings are a static camera and changed content, which is what
`qr_sticker`, `redaction_bars`, `blur_patch` and `caption_bar` are, and the
generated truth calls those the same picture. Only the camera-moved siblings
have a signal of their own (parallax), and a vertical wave also displaces along
parallel lines, so an epipolar test would not exclude it either.

## 8. Where a sibling verdict depends on the corpus (third round)

`IMGFP_WATCH=watch/sib.tsv` (scaffolding, `pipeline::watch`) logs every
retrieval score and verification of the 386 sibling-original pairs, followed
through the file matched for each (a byte copy or same-pixels twin asks in an
original's place). `watch/why.py` sorts each pair into retrieval (no shared word
/ ranked out of `-k`), start (< 3 shared or agreeing), verdict rejected, claim
graph (stands, not stated), stated; `watch/cross.py` crosses originals alone with
the full corpus, `watch/cmp.py` compares runs. All at D 320.

Pairs that stand or are stated on the originals alone (12 IMGS-ALL, 13 IMGS5),
in the full corpus:
- **Verdict: corpus-independent where the pair is met.** n_in alone / full:
  Acueducto1/2 71 / 73, Segovia 79 / 79, game2/3 226 / 225, KoronasdaruLeeds
  94 / 94, Mural 254 / 260, Puerto 78 / 76, bird 347 / 347, Fountain 101 / 101.
- **Retrieval: most are not met.** 6 / 12 and 8 / 13 ranked out (rank 176 to
  4,114) or share no word (Cemetery3/4). The ones met rank 99-143, inside 150
  by a little.
- **Claim graph: most of what is met is refused** (5 / 6, 4 / 6); stated in the
  full corpus only bird, Fountain.
All 300 / 86 sibling pairs: on IMGS-ALL alone 235 are rejected by the verdict,
40 ranked out (303 files, the 150 cut), 13 have < 3 agreeing, 12 stand. In the
full corpus 177 share no word, 109 ranked out, 9 rejected, 5 stand.

The rank falls because the **score** falls: a sibling's containment score in the
full corpus is 1-19% of its score alone (`watch/scores.py`). On the originals
alone retrieval does not filter at all: 59 or 303 files are inside `-k` 150, and
an IMGS5 original's median score against everything is 0.22 (3,098 words). In the
full corpus, 90 copies of each picture make the vocabulary finest around each
seed (occupancy 3 descriptors a word), so its words are smaller than two
photographs of one point lie apart. `-k 300` alone meets 1 more pair of 13.

**Word floor** (`IMGFP_WORD_R`, `Vocabulary::with_floor`): leaves of one parent
whose centres lie within r of each other are one word (single linkage), so a
word is no finer than two observations of one point. r = **143**: the median over
true pairs of each pair's p90 descriptor distance between corresponding
keypoints (`probe/dist.tsv`; p50 54, p99 293). Vocabularies: IMGS5 originals
3,098 -> 3,063 words, full IMGS5 225,617 -> 119,566, full IMGS-ALL 1,034,793 ->
602,469. At r 0 the 17 reports are byte-identical to D 320.
- Siblings standing alone, met in the full corpus: IMGS-ALL 6 -> 9 of 12, IMGS5
  7 -> 10 of 13; scores x1.4-x34 (IMGS5 0.017-0.42 against noise 0.012 at rank
  300, 0.006 at 1,000). Not met: sea_photo1/2 (#233), field1/2 (#224),
  field2/3 (#170), Cemetery3/4 (#620), Seagull (#164), Shenzhen (#1,711).
- Small families: the originals-alone reports identical; r 100 / 143 / 200 / 250
  total joined IMGS-ALL 26.3 / 26.0 / 25.0 / 24.7 (no floor 26.0), IMGS5
  40.7 / 41.0 / 39.3 / 39.3 (40.3); full IMGS5 recall 96.45 / 96.50 / 96.44 /
  96.43 (96.29). No cliff.
- Full IMGS-ALL (list of manifest files; without the floor this is the `-r`
  run to the figure): F1 0.9773 -> **0.9780**, precision 99.72 -> 99.71%,
  recall 95.82 -> 95.96%, cross-family 412 -> 502 (subway1/2 65 added; none
  >= 300), perfect transforms 58 both; keep rule unchanged (0 lost, 298 / 304,
  304 / 304).
- Cost (ABBA, CPU-s, without / with / with / without): full IMGS5 125.7 / 137.2
  / 133.3 / 125.3 (**+8%**: coarser words, longer postings), found mix 342.4 /
  354.3 / 356.6 / 350.1 (+2.6%, about the spread).

What is left of the corpus dependence: (a) the `-k` cut against the copies of
both photographs (met siblings rank 54-164; the asker's own ~90 copies come
first); (b) scores near the noise for the rest; (c) the claim graph, which
refuses most met sibling verdicts in large families by design.

## 9. `-k` counts rejections, and the claim graph's rules (fourth round)

All on guided D 320 + floor r 143. Scaffolding `IMGFP_K_REJECT`
(`pipeline::retrieve_and_verify`): a file's candidates are a window of its
ranking; after verification each file asks for as many more as its window
had verdicts standing, until `-k` have been rejected or the ranking ends.
`IMGFP_CLAIMS_OFF` (`claims::rule_off`): `bridge`, `cover`, `cycle`, `clean`
(every anchor clean). Runs `watch/*-kr`, `c-{none,bridge,cover,cycle,clean}`,
scores `claims_score.out`, cost `cost4.out`.

**`-k` counting rejections.** Verified candidates: full IMGS-ALL 2.59 M ->
4.16 M (30,740 more windows, 6 rounds), full IMGS5 472 k -> 878 k, found mix
+2,211 windows (2 rounds). Sibling pairs met (standing on their originals
alone): IMGS-ALL 9 -> 10 / 12 (sea_photo1/2 from #233), IMGS5 10 -> 11 / 13
(Seagull1/2 from #164); field1/2 and field2/3 now meet but keep < 3 agreeing
points from the vocabulary; Cemetery3/4 (#620) and Shenzhen1/2 (#1711) are
still out. None more is stated: every newly met pair stands and the claim
graph refuses it. Small families identical in every subset (joined 26.0 /
41.0); full IMGS-ALL F1 0.9780 -> 0.9781, recall 95.96 -> 95.98%,
cross-family 502 -> 501; IMGS5 full P 88.44 -> 88.29% (cross-family +425,
all inside its existing merges), R +0.03. Cost (cached, ABBA, CPU-s):
full IMGS5 off 151.6 / 134.5, on 148.4 / 152.5; found mix off 359.4 / 371.4,
on 369.8 / 378.6: within the runs' spread (most of the extra verdicts are
rejected early).

**The claim graph's rules, each off alone** (joined total IMGS-ALL / IMGS5;
full corpora's seed merges >= 300 pairs):

| off | small families | full IMGS-ALL | full IMGS5 |
|---|---|---|---|
| none | 26.0 / 41.0 | F1 0.9781, cross 501, no merge | 6 merges, cross 28,425 |
| bridge | 26.0 / 41.0 | bust1/2 merge (6,039), F1 0.9754 | + Mural, Puerto3/4, Macizo5/6 |
| cover | **31.0 / 48.0** (n1 6.7 -> 9.7, 10 -> 14) | cross 530, no merge | as none |
| cycle | 26.0 / 41.3 | Segovia1/2 merge (6,085) | + Mural, Parish Church |
| clean (all clean) | 27.0 / 44.7 | 229 merges, F1 0.9251 | 51 merges, F1 0.8008 |

Every rule is still load-bearing. The cover test is the one that works
where families are small; the bridge test and the cycle rule guard large
families only. No rule moves n0 or f (joined n0 6 / 9, f 5 / 9): there a
sibling pair is one verdict between two lone files, which no graph rule can
refuse, so only the verdict can. Keep rule unchanged in every run (0 lost,
298 / 304, 304 / 304).

## 10. What merges Macizo1/2 (`mac/`)

Full IMGS5: guided D 320 alone and guided + `-k` counting rejections do not
merge it; the word floor alone (5,611 pairs) and guided + floor (5,813) do.
It is one file: Macizo1's `crop_off_centre` (`scan_31771.jpg`), which holds
only the distant mountain, the same in both photographs. Two Macizo2 copies
match it (`--dump`, n_in / blk / blk_min):

| run | Macizo2 `video_call_frame` | Macizo2 `print_scan` |
|---|---|---|
| no floor, no guided | 27 / 0.963 / **0.9012** clean | 9 / 0.949 / 0.799 (under the points bar) |
| floor | 40 / 0.964 / **0.9120** clean | 24 / 0.965 / **0.9005** clean |
| guided + `-k` | 101 / 0.965 / **0.9067** clean | 80 / 0.964 / 0.8933 anchor |

One clean link is a bridge and is demoted; two clean links into one file are
not a bridge, so both stay clean and the two families join unasked. The floor
does not create the risk, it tips the second link over `CLEAN_WORST_BLOCK`
by 0.0005. Guided + `-k` is 0.0067 from the same merge. The verdicts are not
wrong about pixels: inside the crop the mountain agrees at 0.96 a block.

## 11. The shared-part rule (`IMGFP_PART_RULE`, `part.sh`, `part2.sh`, `hen/`)

On guided D 320 + `-k` counting rejections, no floor (the config to ship).
A clean link holding one way only (one file's frame on the other's at the
overlap bar, not back) says the first is a *part* of the second; a part of
files in two clusters joins neither to the other (section 10: Macizo1's
mountain crop). Clusters are first made from two-way clean links; of the
clusters holding a part's containers its own keeps its links if it holds any,
else the one with most links; links into the others stop being clean.

**As first built (v1)** a file with no two-way clean link was a cluster of
its own. Every composite is such a file, so a part whose own cluster held a
container had its links into its family's own contact sheet, PDF page,
`embed_tiny` cut. Cost found at the cliff sweep: IMGS-ALL correlation 0.4
merges **Henares1/2 (7,055)**, off does not. Traced on a `--dump` with an
offline replay of the claim graph (`hen/replay3.py`; demotions 59,266 vs the
run's 59,242, bridges 50 vs 53, the dump's four decimals): no clean link joins
the families either way; v1 takes Henares1's clean cluster 70 -> 60 files and
Henares2's 74 -> 64 (their composites), and Henares2's `crop_off_centre` has a
weak anchor to all 60 (off: 68 of 70, missing `column_roll`, `embed_tiny`), so
the cycle rule's exception admits the join.

**Fixed (v2, `img-fp-part2`)**: a file with no two-way clean link is of the
cluster most of its one-way clean links reach (then the lowest file). A
family's composite is its family's; Macizo2's `video_call_frame` is
Macizo2's, so its link into Macizo1's crop is still cut. Replayed first on four
dumps (IMGS-ALL correlation 0.4; IMGS5 floor 0 / floor 143 / D320+kr): Henares
gone, Macizo1/2 gone (floor 143), no new seed pair sharing a cluster; then
built (Rust and replay agree: 10,644 / 10,649 demoted, bridges 41 / 39).

Default (no floor):

| | off | v1 | v2 |
|---|---|---|---|
| small families joined IMGS-ALL / IMGS5 | 26.0 / 40.3 | 26.0 / 40.3 | 26.0 / 40.3 |
| full IMGS-ALL F1, P, R | 0.9773, 99.72, 95.83 | 0.9774, 99.73, 95.82 | 0.9772, 99.73, 95.79 |
| full IMGS-ALL perfect transforms | 58 | 58 | 55 |
| full IMGS5 F1, P, R | 0.9340, 90.66, 96.30 | 0.9513, 94.62, 95.64 | **0.9569, 95.74, 95.64** |
| IMGS5 cross-family pairs | 21,939 | 11,830 | 9,189 |
| IMGS5 seed pairs >= 300 | bird 6,827, Fountain3/5 6,642, Floriana1/2 2,873, Floriana3/4 2,665, Tree1/2 2,357 | Fountain 6,644, Floriana1/2 2,871, bird 1,238, Floriana3/4 473, Mural 327 | bird 6,827, Floriana3/4 780, Fountain 549, Floriana1/2 429, **Mural 327** |
| keep rule (IMGS-ALL) | 0 lost, 298 / 304, 304 / 304 | same | same |

v2's three perfect transforms lost are one family each of `crop_half`,
`crop_off_centre`, `qr_sticker` (304 -> 303); recall -0.04 points. Mural1/2
is 248 off, 327 with either version: a person added on a wall, a real merge
by eye, now over the line. bird1/2 (a burst) is whole again under v2 (v1 had
split it).

**Cliffs, v2 against off** (every step of `part.sh`'s sweep, both corpora;
`part2_score.out`): IMGS-ALL's seed pairs >= 300 identical at every step
(docks at overlap 0.5-0.3; red_rocks_sign, Henares 340-346 at correlation
0.2 both ways); F1 within -0.0005 (overlap 0.6: 0.9719 -> 0.9714). IMGS5:
never more pairs >= 300 at a step than off; Tree1/2 gone at every step but
overlap 0.3; Macizo5/6 gone at overlap 0.75 and 0.6; Fountain3/5 and both
Floriana pairs fall from 2,500-7,600 to 420-850 at points 8-3, correlation
0.8-0.2 and `-k` 50 / 25 (unchanged at overlap 0.75 and below); Mural1/2
appears at 319-354 at most steps (off: under 300), Parish Church1/2 at 300 at
overlap 0.75. F1 higher at every step but overlap 0.3 (equal, 0.8645). Macizo1/2 does not
merge in this config with the rule off either (0.0067 under the clean bar,
section 10). With the floor (D 320 + floor, where it merged at 5,813), a run
of v2: no Macizo1/2 pair at all, full IMGS5 F1 0.9684, P 98.34%, R 95.38%
(v1 0.9516; seed pairs >= 300 bird 846, Floriana3/4 782, Fountain 550,
Floriana1/2 429, Mural 329; `mac/part2-d320floor.json`).

**What v2 cost in recall, and v3** (`low/`, `part3.sh`). v2's IMGS-ALL loss
(378 true pairs) is 375 in one family, `low-light.avif`: its `crop_half` and
`crop_off_centre` are two-way to each other, a cluster of 2; the lone
composites `slide_deck`, `wall_poster`, `magazine_spread`, `qr_sticker`
reach it most, so are assigned to it; the part's own cluster held a
container, so own-first kept it and demoted ~50 links into the main cluster
of 57, leaving a fragment of 6 (`low/trace.py`). Every one of v2's 10,653
IMGS-ALL demotions is same-family. **v3**: of the clusters holding the
part's containers keep the most-linked (own on a tie, then lowest file).
Replay (`low/v3.py`) then Rust agree (5,963 / 5,950 demoted, all
same-family; `low-light` whole at 71); Henares (correlation 0.4), Macizo1/2
(floor) and Tree1/2 still stopped.

| | off | v2 | v3 |
|---|---|---|---|
| small families joined IMGS-ALL / IMGS5 | 26.0 / 40.3 | 26.0 / 40.3 | 26.0 / 40.3 |
| full IMGS-ALL F1, P, R | 0.9773, 99.72, 95.83 | 0.9772, 99.73, 95.79 | **0.9774, 99.73, 95.83** |
| full IMGS-ALL perfect transforms | 58 | 55 | **58** |
| IMGS-ALL true pairs against off | | -378 | **-0, +3** |
| full IMGS5 F1, P, R | 0.9340, 90.66, 96.30 | 0.9569, 95.74, 95.64 | 0.9425, 92.40, 96.17 |
| IMGS5 cross-family pairs | 21,939 | 9,189 | 17,418 |
| IMGS5 seed pairs >= 300 | bird 6,827, Fountain 6,642, Floriana1/2 2,873, Floriana3/4 2,665, Tree 2,357 | bird 6,827, Floriana3/4 780, Fountain 549, Floriana1/2 429, Mural 327 | bird 6,827, Fountain 6,642, Floriana1/2 2,872, Floriana3/4 473, Mural 327 |
| keep rule (IMGS-ALL) | 0 lost, 298 / 304, 304 / 304 | same | same |

v3 gives IMGS5 back what v2 took from Fountain3/5 and Floriana1/2, which by
eye are crop-like and may merge (user's verdict), so its lower F1 there is
those pairs counted false. IMGS5 true pairs against off: -591 (Floriana4
292, Floriana3 182, Mural1 70, Parish Church1 40; `crop_micro`,
`crop_quarter`, `crop_off_centre`, `crop_square`, `crop_strip_top`), +287
(Parish Church2 151, Floriana3 80); v2 was -1,790. Cliffs (`part3_score.out`):
IMGS-ALL seed pairs >= 300 identical to off and v2 at every step, F1 equal
to off or up to +0.0002 at every step (v2 down to -0.0005); IMGS5 the same
merge counts as v2 at every step, never more than off; Tree1/2 gone at every
step but overlap 0.3, Floriana3/4 451-514, Fountain3/5 and Floriana1/2 at
their off levels. With the floor (`mac/part3-d320floor.json`): Macizo1/2 3
pairs (off 5,813, v2 0), F1 0.9535 (bird 846, Fountain 6,685, Floriana1/2
2,855, Floriana3/4 474, Mural 329).

**The stack against 0.72.0 at every cliff step** (`cliff6.sh`, `cliff6_score.sh`,
`cliff6_table.out`; 0.72.0 from `v44-flat-contrast`, one step rerun from these
lists identical; page https://claude.ai/artifact/UQpb24tWzdheHiFdKfANx8).
23 steps + default each corpus. IMGS-ALL: recall higher at all 24, F1 at 23
(equal at correlation 0.2), precision lower at all 24 (0.01-0.6 points, 1.0 at
overlap 0.25), cross-family higher everywhere (default 10 -> 479); merges new
at overlap 0.5-0.25 (docks1/2; Acueducto3/4 at 0.25) and correlation 0.3-0.2
(red_rocks_sign1/2; Henares1/2 at 0.2), none in 0.72.0; the same with the rule
off where run. IMGS5: F1 higher at 23 (not overlap 0.3), precision and
cross-family better at 22 (not overlap 0.3, `-k` 10), recall lower at points
6-3 and correlation 0.2; merges fewer at points 4-3 and correlation 0.9-0.8,
more at overlap 0.3 and `-k` 50-10 (0.72.0 starved there: recall 60.4% at 10).

## Open

1. D: derived at 320 (section 5); gated or not is the cost/recall choice (section 6).
2. Retrieval: the word floor (section 8) meets 9 / 12 and 10 / 13, `-k`
   counting rejections 10 / 12 and 11 / 13 at no measurable cost (section 9);
   left: scores near the noise (Cemetery3/4, Shenzhen1/2).
5. The claim graph: every rule still load-bearing (section 9). Precision on
   small families now rests on the verdict alone at n0 / f and on the cover
   test at n1 / n2.
6. The shared-part rule (section 11): v3 stops Macizo1/2, Tree1/2 and
   most of Floriana3/4, adds nothing on IMGS-ALL at any step, loses no
   IMGS-ALL true pair; Mural1/2 crosses 300 (a real merge by eye).
3. Cost: +17% ungated, +3.4% gated (section 6).
4. Siblings against warps: no separator found (section 7); the nearest siblings
   are edits by the truth's own definition.
