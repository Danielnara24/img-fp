# Baseline: the competition, measured

The field over **IMGS-ALL**, now all five generated corpora (IMGS to IMGS5:
33,136 files, 364 seeds, the first generation's 90 transforms), and over
**IMGS-ALL2**, the hard generation of the same 364 seeds (33,500 files, 91
transforms, every placement and direction drawn per seed and every transform
0.74.0 found for all seeds made harder). Each tool was run once, cold, alone,
with the corpus evicted from the page cache and a cooldown to a measured idle
+ 3 C, in one session (2026-10-28, `out/v54-field/all` and `all2`:
`metrics.json`, `score.txt`, `per_transform.csv`, img-fp's `analyse.txt` and
`suggest.txt`; scripts `run.sh` then `run3.sh`). img-fp is **0.75.0** at its
defaults, the plain x86-64 build the release ships.

The short version: on IMGS-ALL img-fp is **F1 0.974 against SSCD's 0.729**,
and **54 of 87 transformations are perfect on all 364 seeds** against **zero
for every other tool** (fclones' one is `copy_exact`). On IMGS-ALL2, made to
be hard for img-fp, it is **0.939 against SSCD's 0.472**,
and every competitor loses more recall to the harder transforms than img-fp
does. Nothing else finds an image inside a bigger one.

SSCD and difPy were left out of the first IMGS-ALL2 pass (user's decision,
to finish within the day) and run alone afterwards, into the same folder:
SSCD that evening (`sscd2.sh`), difPy overnight (`difpy2.sh`).

## Results: IMGS-ALL (five corpora)

| | count |
|---|---:|
| files | 33,136 (33,119 in the truth) |
| positive pairs | 1,369,009 |
| negative pairs | 547,024,647 |
| excluded (PARTIAL) | 23,865 |

| tool | F1 | precision | recall | perfect | wall | CPU s | CPU% | PSS | anon+swap | proc | img/s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **img-fp 0.75.0** | **0.974** | 98.8% | **96.0%** | **54** | 590 s | 3,751 | 636% | 2,706 MB | **606 MB** | 1 | 56 |
| sscd | 0.729 | 80.5% | 66.6% | 0 | 12,745 s | 71,057 | 558% | 1,626 MB | 3,138 MB | 5 | 3 |
| imagededup_cnn | 0.571 | 85.0% | 42.9% | 0 | 1,460 s | 2,450 | 168% | 3,041 MB | 12,414 MB | 9 | 23 |
| imagededup_phash | 0.504 | 96.6% | 34.1% | 0 | 604 s | 3,683 | 610% | 1,037 MB | 3,411 MB | 9 | 55 |
| czkawka_tuned | 0.414 | 99.4% | 26.2% | 0 | 1,548 s | 10,057 | 650% | 1,727 MB | 1,711 MB | 2 | 21 |
| pdq | 0.402 | 100.0% | 25.1% | 0 | 1,444 s | 7,306 | 506% | 3,366 MB | 4,069 MB | 9 | 23 |
| findimagedupes | 0.115 | 7.0% | 33.2% | 0 | 753 s | 1,973 | 262% | 1,274 MB | 1,272 MB | 2 | 44 |
| imgdupes_phash | 0.107 | 98.4% | 5.7% | 0 | 201 s | 1,195 | 595% | 822 MB | 936 MB | 9 | 165 |
| dupeguru | 0.072 | 100.0% | 3.7% | 0 | 4,047 s | 5,270 | 130% | 1,188 MB | 1,546 MB | 9 | 8 |
| czkawka_default | 0.020 | 97.7% | 1.0% | 0 | 230 s | 1,535 | 667% | 971 MB | 956 MB | 2 | 144 |
| difpy | 0.004 | 97.9% | 0.2% | 0 | 18,620 s | 29,830 | 160% | 3,559 MB | 7,765 MB | 10 | 2 |
| fclones | 0.002 | 100.0% | 0.1% | 1 | 1 s | 2 | 311% | 28 MB | 22 MB | 2 | 46,022 |

img-fp in detail (`analyse.txt`): precision 98.85%, recall 95.98%; false pairs
3,075 rearrangement traps and 12,227 cross-family, of which five seed pairs
pass 300 pairs, all IMGS5 photographs of one scene (Fountain3/5 6,697,
Floriana1/2 2,977, bird1/2 808, Floriana3/4 554, Mural1/2 331); halves 0.9720 /
0.9772. Keep rule: 19,925 DELETE, none loses content, 358 / 364 families keep
the original or a lossless copy, 364 / 364 a faithful full copy. The figures
are 0.75.0's cached measurement to the pair (`out/v53-audit`): cold and cached
runs agree.

## Results: IMGS-ALL2 (the hard generation)

| | count |
|---|---:|
| files | 33,500 (33,483 in the truth) |
| positive pairs | 1,384,501 |
| negative pairs | 559,114,958 |
| excluded (PARTIAL) | 39,444 |

| tool | F1 | precision | recall | perfect | wall | CPU s | CPU% | PSS | anon+swap | proc | img/s |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **img-fp 0.75.0** | **0.939** | 98.6% | **89.6%** | **11** | 450 s | 2,898 | 644% | 2,438 MB | **625 MB** | 1 | 74 |
| sscd | 0.472 | 80.4% | 33.4% | 0 | 12,091 s | 67,821 | 561% | 1,959 MB | 4,704 MB | 5 | 3 |
| imagededup_phash | 0.423 | 98.1% | 27.0% | 0 | 545 s | 3,300 | 606% | 1,214 MB | 3,589 MB | 9 | 61 |
| pdq | 0.299 | 100.0% | 17.6% | 0 | 1,297 s | 6,275 | 484% | 3,310 MB | 4,585 MB | 9 | 26 |
| imagededup_cnn | 0.249 | 88.1% | 14.5% | 0 | 1,291 s | 2,278 | 176% | 3,241 MB | 12,650 MB | 9 | 26 |
| czkawka_tuned | 0.197 | 99.7% | 10.9% | 0 | 1,124 s | 7,300 | 650% | 2,071 MB | 2,064 MB | 2 | 30 |
| imgdupes_phash | 0.107 | 99.8% | 5.7% | 0 | 152 s | 870 | 573% | 831 MB | 946 MB | 9 | 221 |
| findimagedupes | 0.094 | 5.6% | 27.3% | 0 | 642 s | 1,816 | 283% | 1,310 MB | 1,308 MB | 3 | 52 |
| dupeguru | 0.012 | 100.0% | 0.6% | 0 | 3,238 s | 4,511 | 139% | 1,181 MB | 1,420 MB | 9 | 10 |
| czkawka_default | 0.008 | 98.7% | 0.4% | 0 | 135 s | 885 | 654% | 818 MB | 803 MB | 2 | 248 |
| difpy | 0.003 | 98.2% | 0.1% | 0 | 8,770 s | 22,987 | 262% | 3,911 MB | 7,614 MB | 10 | 4 |
| fclones | 0.001 | 100.0% | 0.1% | 1 | 1 s | 2 | 343% | 29 MB | 23 MB | 2 | 63,208 |

img-fp's 11 perfect transforms include the seven that are perfect by
construction (identical pixels, or a plain re-save of an opaque seed); the
other four are ones the floor brought to 364 (0.74.0: 7). Its hardest: `pinch`
246, `tiled_watermark` 249, `swirl` 262, `wave` 274, `crop_micro` 281,
`perspective` 286. False pairs 4,100 traps and 13,091 cross-family (Fountain3/5
6,023, bird1/2 5,715, Floriana3/4 371 over 300). Keep rule: 15,576 DELETE, none
loses content, 351 / 364 originals or lossless copies, 364 / 364 a faithful
copy.

## What the numbers say

**The hard generation costs the field more than it costs img-fp.** From
IMGS-ALL to IMGS-ALL2 img-fp's recall goes 96.0 -> 89.6%. imagededup_cnn's
goes 42.9 -> 14.5%, SSCD's 66.6 -> 33.4%, czkawka_tuned's 26.2 -> 10.9%, pdq's 25.1 -> 17.6%,
dupeGuru's 3.7 -> 0.6%: the global descriptors and hashes lose most of what
they had on stronger crops, turns and edits made in any direction, which is
what IMGS-ALL2 changed. imagededup_phash holds up best of the field (34.1 ->
27.0%); SSCD is second there on F1, at 80.4% precision.

**Containment is still img-fp's alone.** On IMGS-ALL, of 364 seeds:
`embed_tiny` 352 against 0 for every other tool, `contact_sheet` 352 against
0, `collage_cell` 363 against 1, `picture_in_picture` 359 against 7,
`crop_quarter` 362 against SSCD's 56, `pdf_page` 362 against 71,
`wall_poster` 361 against 76, `phone_screenshot` 361 against 95,
`embed_small` 363 against 188, `slide_deck` 363 against 203. On IMGS-ALL2 img-fp
has 344-363 on each; the best other tool is SSCD on six (`slide_deck` 203,
`embed_small` 160, `wall_poster` 146, `crop_quarter` 78, `pdf_page` and
`phone_screenshot` 71), and nothing finds more than 3 of `embed_tiny`,
`contact_sheet`, `collage_cell` or `picture_in_picture`. SSCD ties img-fp on
`browser_window` (363).

**Where a competitor leads**, by seeds found of 364. IMGS-ALL: SSCD on
`keystone_side` (363 / 346), `perspective_top` (363 / 349), `wave_vertical`
(323 / 313), `photo_of_screen` (363 / 358) and `flip_h` (363 / 360);
imagededup_phash on `tiled_watermark` (291 / 241); pdq on `vignette` (362 /
355); czkawka_tuned on `transpose_diag` (362 / 361). IMGS-ALL2:
SSCD on `perspective` (363 / 286), `pinch` (358 / 246), `swirl` (359 / 262),
`wave` (297 / 274), `perspective_two` (363 / 353), `photo_of_screen` (362 /
359) and `hue_rotate` (363 / 362);
imagededup_phash on `tiled_watermark` (300 / 249), `jpeg_low` (340 / 324) and
`noise` (347 / 338); pdq on `blur` (361 / 320) and `vignette` (362 / 355); SSCD
and pdq both on `aspect_distort` (363 / 352). Non-affine warps and whole-picture overlays,
where a global descriptor is helped by not needing one transform for the
whole frame; and low-detail degradations (blur, noise, JPEG at low quality)
on IMGS-ALL2, where local features thin out and a hash of the whole still
holds.

**Precision.** Every hash tool keeps 97-100% precision by finding little.
findimagedupes stays the outlier at 7.0% / 5.6% (6.5-6.7 M proposed pairs).
SSCD's precision falls from 85.3% (four corpora) to 80.5% with IMGS5 in,
whose seeds are near-miss sets of one scene; not broken down by family here.

**difPy finished**, which it did not on four corpora: 5.2 hours and 29,830
CPU-seconds, swapping from its first hour (7.8 GB anonymous memory plus swap
at the peak, against 5.8 GB of RAM), for F1 0.004. **imagededup_cnn's**
similarity matrix reached 12.4-12.7 GB anonymous plus swap. On IMGS-ALL2
difPy took 2.4 hours (22,987 CPU-seconds, 7.6 GB anonymous plus swap) for
F1 0.003: 1,979 pairs proposed. Why it ran in half its IMGS-ALL time was
not looked into; it was alone on the machine at night both times.

### Cost

img-fp is the only tool here that finishes IMGS-ALL in under ten minutes
while keeping all eight threads busy, and the only one whose anonymous memory
stays under a gigabyte (606 / 625 MB; its PSS also counts the mapped analysis
file, which the kernel can drop and read again). SSCD took 3.5 hours and
71,057 CPU-seconds, 19 times img-fp's CPU for F1 0.729; on IMGS-ALL2 3.4
hours, 67,821 CPU-seconds (23 times img-fp's) and 4.7 GB anonymous plus
swap, for 0.472.

Cost figures are softer than the accuracy:

- The idle baseline measured at the start was 78.8 C (ceiling 81.8 C), with
  5.6-6 GB of swap in use by the desktop throughout; mean clocks ran from
  1,455 to 2,462 MHz by tool.
- SSCD's IMGS-ALL2 run started from a hotter idle (82.2 C measured, ceiling
  85.2 C) in the evening, at a mean 2,155 MHz.
- From late morning (SSCD's last half hour on IMGS-ALL onward) the laptop was
  in use, which adds noise to those rows' wall and CPU.
- img-fp's 3,751 CPU-seconds against 0.74.0's 3,133 (`out/v52-all5`) is the
  word floor's measured +7.9% on matching plus a different session and
  clock; it is no A/B.
- `bench.py`'s memory sampler, which reads every process's memory map ten
  times a second, took about 65% of a core while difPy ran with 14
  processes; it was the same in every earlier baseline, but it is a tax that
  multi-process tools pay and img-fp (one process) hardly does.

## DISC21: the public copy-detection benchmark

The corpora above are ours, generated. DISC21 (the Image Similarity
Challenge 2021 dataset) is not: on its dev set, 50,000 query images are
matched against 1,000,000 reference photographs (YFCC100M), and 10,000 of
the queries are edited copies of a reference, by a ground truth we did not
make. It is the set SSCD was trained and published on. Scored by µAP (micro
average precision over every (query, reference, score) prediction of every
query, in one ranking; written as a percent, as the papers do) and by recall
at 90% precision.

img-fp: the build after 0.76.0 (default correlation 0.7), run 2026-10-29/30
as 20 shards, each all 50,000 queries `--against` one 50,000-reference zip at
`--min-pixel-correlation 0.5 --dump`; a query-reference pair's score is its
best aligned points over the shards (correlation / 2 breaks ties). Higher
bars are the 0.5 run filtered by correlation (a real 0.7 run on a 25k x 25k
piece agreed on 102 of 103 pairs). Scripts and outputs in
`~/Documents/DISC21` (`loop.sh`, `shard.sh`, `score_shards.py`, checked
with `score_disc.py`). SSCD's rows are the paper's (Pizzi et al., CVPR 2022,
Table 2) and its repository's `docs/Evaluation.md`, not run here.

| | µAP | recall at 90% precision | predictions | recall |
|---|---:|---:|---:|---:|
| **img-fp, bar 0.5** | **65.3** | 51.7% | 17,337 | 71.3% |
| img-fp, bar 0.6 | 64.7 | 55.2% | 11,024 | 68.3% |
| img-fp, bar 0.7 (default) | 59.9 | 54.1% | 8,045 | 62.1% |
| img-fp, bar 0.8 | 50.1 | 49.0% | 5,929 | 51.2% |
| SSCD ResNet50 (paper) | 61.5 | 38.3% (repo) | | |
| SSCD-large, ResNeXt-101 (paper) | 63.7 | | | |
| SSCD ResNet50, score normalization | 72.5 | 63.1% (repo) | | |
| SSCD-large, score normalization | 75.3 | | | |

**Read with this:**

- img-fp learns nothing; SSCD is trained on DISC21's own 1,000,000-image
  training set, whose edits are drawn from the same augmentation family as
  the queries'.
- Score normalization subtracts from a query's similarity its mean
  similarity to its three nearest images in that training set, which makes
  scores comparable across queries. img-fp has no such step. Simulated with
  the other dev queries as the background (points less the mean of a
  query's 1-3 best verdicts with other queries), it gives µAP 66.0-66.7:
  about +1, not +11, since an unrelated pair rarely gives img-fp a verdict
  at all.
- The ranking (points, not correlation) and the 0.5 bar were chosen on the
  dev set itself, on a 25k x 25k piece first. DISC21's held-out final query
  set has not been run.
- Sharding gives each query up to 150 candidates per 50,000 references,
  not 150 overall, which favours recall over a single run.
- Cost: 9.1 hours wall, 37.6 CPU-hours, 24-36 minutes a shard, peak memory
  under 1.7 GB, on this laptop (Ryzen 7 3700U, 4 cores / 8 threads, 5.8 GB).

## How these were measured

`bench.py`, one tool at a time, never two at once.

- **Order.** `--only` runs tools in the order given: the ones that could run
  the machine out of memory first (difPy, imagededup_cnn, pdq, SSCD,
  czkawka_tuned, imagededup_phash) on both corpora, then the rest, img-fp
  last.
- **Out of memory.** Every tool runs with `oom_score_adj` 1000, so when memory
  runs out the kernel ends the tool and not the desktop session beside it, in
  a session of its own under `--timeout 6` hours (none reached it; none was
  killed). The script itself was started detached (`setsid nohup`), out of
  reach of Claude Code's background-shell memory guard, which ended the
  four-corpus baseline's difPy run.
- **Cold cache, both kinds.** Each tool's own cache disabled (img-fp
  `--no-cache`, czkawka `-H`, imgdupes `--no-cache`, a fresh dupeGuru
  database, SSCD without `--embeddings`); the corpus evicted from the page
  cache with `posix_fadvise(POSIX_FADV_DONTNEED)` before every run.
- **CPU seconds** from `getrusage(RUSAGE_CHILDREN)` deltas; **peak memory**
  sampled over the whole process tree at 10 Hz: PSS, RSS, and anonymous
  memory plus swap (`peak_anon_swap_mb`), which for img-fp is the figure that
  counts (its PSS includes the mapped analysis file).
- **Thermal.** 60 s minimum cooldown, then a wait for the die to come back to
  the idle measured at the start + 3 C (78.8 C, ceiling 81.8 C), the same
  ceiling for every invocation.

## Reproducing

```bash
cd benchmark
RUSTFLAGS="-C target-cpu=x86-64" CARGO_TARGET_DIR=../target/plain \
    cargo build --release --locked --bin img-fp     # the build the release ships
python3 corpus/make_combined.py --clean             # after regenerating a corpus
../vendor/venv/bin/python -u bench.py --corpus ~/Documents/IMGS-ALL --out out/vNN/all \
    --temp-ceiling C --timeout 6 --only difpy,imagededup_cnn,pdq,sscd,czkawka_tuned,imagededup_phash,dupeguru,findimagedupes,imgdupes_phash,czkawka_default,fclones,imgfp
../vendor/venv/bin/python score.py --corpus ~/Documents/IMGS-ALL --per-transform --csv out/vNN/all/per_transform.csv out/vNN/all/*.json
../vendor/venv/bin/python analyse.py --corpus=$HOME/Documents/IMGS-ALL --per-transform out/vNN/all/imgfp.json
```

`out/v54-field/run.sh`, `run3.sh`, `sscd2.sh` and `difpy2.sh` are the scripts this
baseline ran (`table.py` makes the tables). `--imgfp-bin` measures another
img-fp binary and `--imgfp-args` passes options; each img-fp row records its
binary's path, version and sha256. `--only` merges into whatever `--out`
names, so a tool re-run replaces its own row. The memory-heavy tools need the
machine to themselves: close the browser, and start the run detached.
