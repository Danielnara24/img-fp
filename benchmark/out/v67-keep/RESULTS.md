# v67-keep: the keep rule's promise, and finer evidence for siblings (2026-11-12)

`src/suggest.rs` only. Builds: `base/img-fp` is HEAD (1312919), `new/img-fp-final` the change, both plain
x86-64; one cache (`cache/`, filled by `fill.sh`). `final.sh` is the battery; `new/` holds its
outputs, made by a build whose answers are the final one's to the file (`n7/`), before two changes
that only cost less (section 6). Unit tests: 239 pass, three new.

## 1. The loss at HEAD

IMGS-ALL2, `IMGS/low-light.avif`: `phone_screenshot` (273x478) DELETE. It was deleted by a file
itself deleted, through a propagated pair worth 5.5 points: `FIT_ERROR / points` = 12.7% of the
frame, more than the phone's bars (8%), so the screenshot was "the frame" of the photograph.
Reproduces on the family alone (`lowlight.list`, 0.8 s).

## 2. What changed (`suggest.rs`)

- **One frame is the corners and the pixels** (`Component::same`, `exact`, `frame_onto_frame`):
  the fit puts each frame in the other (`FIT_ERROR`) and each shows the other over its whole frame
  under the transform their sizes give, corner to corner. Offline on IMGS-ALL2's 531,510 strong
  pairs (`all2.sides`, `samerule.py`), pairs wrongly called one frame: corners alone 151, pixels
  within `FIT_ERROR_MOST` 65, both 5. `sides.py`: laid corner to corner, one-frame faithful pairs
  read p50 4.0 / p99 16.1; a 2-5% crop p10 30.8.
- **Every deletion is a kept file's** (`Ctx::settle`, `Component::through`): a deleted file hands
  on its keeper; the kept file is asked of the next file directly, the transform composed through
  the files between. A file the kept file is itself paired with is not asked again.

Plain builds, cached, defaults (`new/` against `base/`):

| | HEAD | new |
| --- | --- | --- |
| IMGS-ALL2 DELETE / REVIEW | 15,524 / 6,006 | 13,819 / 5,367 |
| content lost, and with regions exact | 1 / 1 | 0 / 0 |
| lost, holders neither composites nor turned copies | 39 | 0 |
| original or lossless copy kept / 364 | 351 | 350 |
| picture left only in a composite, a turned copy or an edit | 5 | 3 |
| IMGS-ALL DELETE / REVIEW | 19,733 / 3,725 | 17,607 / 5,053 |
| content lost / strict | 0 / 64 | 0 / 0 |
| original or lossless copy kept / 364 | 358 | 363 |
| picture left only in a composite, a turned copy or an edit | 4 | 1 |
| `pairs.py` origDEL / varDEL / loss / REVIEW | 344 / 6,304 / 26 / 35,229 | 344 / 6,309 / 26 / 34,629 |
| IMGS-PROVENANCE (both trees) DELETE, originals, lost | 2,368, 126 / 266, 0 | 2,351, 126 / 266, 0 |
| IMGS-AGAINST repeat / new | 7,165 / 2,949 | 6,735 / 3,352 |
| content lost; named keeper does not hold | 0; 443 (scan 203) | 0; 286 (scan 46) |
| held files that are `repeat` | 3,317 / 4,430 | 3,292 / 4,430 |
| false library claims | 71 | 73 |
| found corpus actions | 125 DELETE, 300 REVIEW | the same, no file moved |

Pairs are HEAD's to the byte on IMGS-ALL, IMGS-ALL2 and the found corpus (the rule reads them, it
states none). Against a library the 402 files that are `new` and were `repeat` have no library
copy: they were repeats of another scanned file by a chain.

Deletions given up (`diff2.py`): unedited copies 6.6% (ALL2) / 3.7% (ALL), edits 26.4% / 22.5%,
three in four to REVIEW: the kept file itself does not match them within the bar. The steps
(quick build): pixels-only "one frame" and no relay 13,824 / 18,090 DELETE (`n1`), with the relay
14,185 / 18,129 (`n2`), corners and pixels with the relay 13,786 / 17,611 (`n3`).

## 3. Tried and rejected

- **Pixels alone for "one frame"** (n2): a slightly turned `print_scan` read as its original's
  frame; `pairs.py` 26 -> 28.
- **A file is never deleted for a smaller one** (n4): larger files then win every size class;
  IMGS-ALL2 originals kept 349 -> 313. Reverted.

## 4. Open: a file deleted for a smaller one

24 of `pairs.py`'s 26 losses are an original deleted for a smaller copy that `extra` says it adds
nothing to (18 downscales, 4 `noise`, 2 `halftone`). By a second method (`psnr.py`: the kept copy
enlarged against the original) some are real: `pedestrian` for a thumbnail 4% its size 21 dB,
`logo` 29, `selfie2` 31-33; others harmless (40-47 dB). The statistic cannot prove an enlargement.
Also: `suggest_score.py` reads IMGS-ALL2's `upscale` (made from a small copy) as full detail; no
deletion rests on one alone, HEAD or new (`noupscale.py`).

## 5. D: finer evidence does not separate siblings

`dsep.py`, `droc.py` on original + 2 variants (`v51-families` lists): siblings passing at equal
shares of true pairs kept. A grid of 96 or 192 samples, on thumbnails of 128, 256 and 512 px, is
level with or worse than the 48 grid on 128; inlier residual over keypoint size is far worse
(IMGS-ALL2 at 92% kept: 136 siblings by `blk`, 159 by the 96 grid, 350 by residual). No sibling
is a clean anchor in these subsets. Resolution is not what lets siblings through.

## 6. Cost

The suggestion stage (`-v`, "grouping and suggesting"), cached, 8 threads, plain builds: IMGS-ALL2
HEAD 7.2-7.3 s, new 9.5 s of a 135-150 s run; IMGS-ALL 8.6 s and 9.0 s. It was 13.6 s and 18.2 s:
every pair the corners call one frame was laid corner to corner, each way on its own (290,000
samplings on IMGS-ALL2, 714,000 on IMGS-ALL). Now both ways come of one sampling (`whole_both`;
moved 92 and 21 files' actions, a blur step's rounding) and a third picture of a frame is looked
for only until one is met (`pictures_of_one_frame`; answers identical, `n7/`). A first guess, that
the cost was the class scan's hashing, was wrong: no time moved (`n5/`).

Scaffolding for all of it: `scaffolding.patch` (not the final rule: it predates n3 and the relay's
last form in parts).
