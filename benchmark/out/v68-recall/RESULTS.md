# v68-recall: the keep rule as a classifier, precision first (2026-11-12/13)

Follows `v67-keep` (same cache, `v67-keep/cache/`; HEAD's build is `v67-keep/base/img-fp`). `src/suggest.rs`
only; pairs are HEAD's to the byte. `new/img-fp` is the final plain build, `final.sh` its battery
(`final.out`; `final-before-both.out` is the same battery before the two-versions rule of `v69-bar`). The sweeps' reports and the scaffolded builds were removed; `suggest.scaffold.rs` is the
source with every switch (`IMGFP_*`), `try.sh` / `battery.sh` what ran them. User's instruction: recall as high as it goes with precision at 100%; a composite or a
mirrored / turned copy counts as holding the picture; a composite that fully contains the photograph
stands in for it, and the found corpus's stretched crops are to be DELETE (2026-11-13).

## 1. The measure (`redundant.py`)

A file is redundant when a file that stays holds it (`suggest_score.holds`). The ideal keeps a smallest
set of files that between them hold every other (greedy, per seed), so TP = deleted and held by a file
that stays, FP = deleted and held by none, FN = the rest of what the ideal deletes.

| | TP | FP | precision | recall |
| --- | ---: | ---: | ---: | ---: |
| IMGS-ALL2 HEAD | 15,523 | 1 | 99.994% | 55.41% |
| IMGS-ALL2 `--suggest correlation` | 20,169 | 4,793 | 80.80% | 71.99% |
| IMGS-ALL2 `--suggest representative` | 22,508 | 9,089 | 71.23% | 80.34% |
| IMGS-ALL2 v67 | 13,819 | 0 | 100% | 49.33% |
| **IMGS-ALL2 new** | 15,790 | 0 | 100% | 56.36% |
| IMGS-ALL HEAD | 19,733 | 0 | 100% | 71.34% |
| IMGS-ALL `--suggest correlation` | 23,201 | 2,892 | 88.92% | 83.88% |
| IMGS-ALL `--suggest representative` | 25,625 | 6,066 | 80.86% | 92.65% |
| IMGS-ALL v67 | 17,607 | 0 | 100% | 63.66% |
| **IMGS-ALL new** | 19,224 | 0 | 100% | 69.50% |

## 2. Where the full corpora cannot see: small families

`small.py`: every seed's original and one or two random variants (`v51-families` lists, three draws each).
`pairs.py`: the original and one named variant.

Lost is counted two ways. Strict: the file that stays shows the picture at 0.95 of its size or more
(`suggest_score.py`'s `EFF`). By the user's allowance: at 0.7 or more, the half octave `SHRINK` allows
(`IMGFP_EFF=0.7`).

| deleted / lost strict / lost by the allowance | HEAD | new |
| --- | --- | --- |
| IMGS-ALL2, original + 1 | 329 / 5 / 5 | 353 / 0 / 0 |
| IMGS-ALL2, original + 2 | 665 / 7 / 7 | 705 / 0 / 0 |
| IMGS-ALL, original + 1 | 211 / 0 / 0 | 236 / 2 / 0 |
| IMGS-ALL, original + 2 | 741 / 27 / 23 | 596 / 9 / 3 |
| `pairs.py` (two files, each transform) | 6,648 / 26 / 25 | 7,292 / 54 / 1 |

`pairs.py`'s strict 54 is the scorer's, not the rule's: 44 `phone_screenshot`, 9 `slide_deck`. The
manifest's coverage for a composite is a nominal constant; the photograph sits in it at its own pixel
size. 51 were checked by template matching (cv2, correlation >= 0.996 at scale 1). The one left by the
allowance is `IMGS/16.jpg`, deleted for a phone screenshot showing it at 0.897 (the `on_canvas`
lone-pair branch).

HEAD's losses (`smallwhy.py`): the original deleted for a copy 0.18-0.74 of its size (`extra` read it as an
enlargement), or, of three different pictures of one frame, for the edit `choose` picked (noise, sharpen,
posterize, a JPEG saved several times). The three left: a twelve-level posterize of a black and white
photograph and a JPEG re-saved several times, each within every bar a faithful re-encode keeps to
(`TAU`, `SOFT_TIE`, the whole-frame check); no order of `choose` moves them (`s-g-midfirst`).

## 3. What changed, in the order tried (quick build unless said; ALL2 / ALL deletions, all at 0 lost there)

| step | ALL2 | ALL | note |
| --- | ---: | ---: | --- |
| v67 | 13,819 | 17,607 | |
| a file left open deletes what it holds | 14,196 | 17,820 | only kept files did |
| mirrored pairs take part | 14,834 | 18,408 | two alone are an undecided pair |
| a rising tone curve in the whole-frame check, not a line | +694 | +394 | measured at the next row's rule |
| corroborated keeper; no file for a smaller one; the nearest before the smaller in `choose` | 15,534 | 19,037 | small families 39 lost -> 3, `pairs.py` 26 -> 2 |
| nor for a larger frame showing it smaller (fit's error) | 15,515 | 18,887 | `pairs.py` 2 -> 0; found corpus DELETE 125 -> 7, rejected by the user |
| the half-octave allowance back (`SHRINK`); composites stand in | 15,801 | 19,207 | plain, `final-before-both.out` |
| two versions each with a copy are undecided (`v69-bar`) | 15,790 | 19,224 | plain, final |

- **Corroboration** replaces "three pictures of the frame": `k` deletes a different picture of its frame
  and size only beside a third that is `k`'s as nearly as a re-encode is (`same`, grey within `TAU`,
  softness within `SOFT_TIE`).
- **No file for one showing its picture more than half an octave smaller** (`SHRINK`, `shows_smaller`);
  such a file is REVIEW (`Facts.shrunk`). The `extra` statistic no longer deletes past it: the
  statistics kept cannot prove an enlargement. Over 684 such pairs of the two-file lists (627 true
  enlargements, 57 originals taken for one), `extra` per octave, the larger's softness and the two
  files' softness difference all overlap. A stricter allowance (`SAME`, 0.2 octave, and the fit's error
  for a larger frame) lost nothing more but took the found corpus's stretched crops to REVIEW (DELETE
  125 -> 7); the user wants those deleted, so it is `SHRINK`.
- **A composite stands in** (user, 2026-11-13): a larger frame that is not a plain picture deletes
  what it holds when the pair is clean, the transform is a similarity showing the picture no smaller,
  within the fit's error (`shown_as_it_is`), and the held file is not itself several pictures
  (`shows_more_than_one`). Without `shown_as_it_is` originals went for perspective `wall_poster`s and
  a captioned copy; a collage cell at 0.74x made a `column_roll` trap a `repeat`.
- **Two versions each with a copy** (`v69-bar`): a different picture of the frame that has a faithful
  copy of its own is not deleted for a corroborated keeper; both stay, REVIEW.

## 4. Tried and not kept

- **A looser whole-frame bar.** For a larger frame (crops), 16 / 20 / 28: ALL2 +618 / +1,001 at 16 / 28
  with nothing lost on the corpora, but `pairs.py` loses 4 at 16 and 10 at 24-28 (an original for a
  `blur_fill`, a `wall_poster`, a `contact_sheet`). For one frame, 16 / 20 / 32 / 48: ALL2 16,725 at 20,
  17,467 at 48, nothing lost in any regime measured, every file gained an edit. Not taken: no corpus
  holds an edit that was copied in its turn. `v69-bar` built that corpus: the bar is not what decides
  it, and 12 stays.
- **`choose` by the nearest before lossless**: the same three losses, fewer deletions.

## 5. What still stays, IMGS-ALL2 (`fn.py`, `t2-ALL2`), and why

12,226 files stay that a file that stays holds, by the truth. The table was read at 12,503, before
composites stood in and the allowance came back; the rows are otherwise as they were:

| files | why | can it be had at 100%? |
| ---: | --- | --- |
| ~7,400 | edits and warps paired only weakly, or never grouped (`pinch`, `swirl`, `wave`, `perspective`, `tiled_watermark`, `qr_sticker`, `vignette`, `redaction_bars`, `invert`) | not from what is kept: their pixels are not the kept file's under any affine transform; a warp would need a non-affine registration, a local edit is content of its own to any pixel test |
| ~1,860 | one frame by the corners, pixels past the bar (`watermark_corner`, `sharpen`, `duotone`, `greyscale`, `hue_rotate`) | only by a wider bar (section 4) |
| ~970 | a composite or an `upscale` that stays holds a kept file | partly had since: a composite shown at full size stands in; ALL2's `upscale` is made from a small copy, which the truth cannot see |
| ~690 | crops and turned copies over the bar under the fitted transform | the bar (section 4); or a registration by the pixels, not built |
| ~370 | `blur_fill`: the truth counts the padded copy redundant; its frame reaches past the original's | no pixel test tells blurred padding from a caption |
| ~315 | undecided pairs (no corroborated keeper) | no |
| ~300 | the fit cannot say which frame is larger (`FIT_ERROR` to `FIT_ERROR_MOST`) | a better-fitted pair only |
| rest | enlargements, softer keepers, lossless files | no (section 3) |

## 6. Elsewhere (plain build, `final.out`)

- IMGS-PROVENANCE (both trees): 2,262 DELETE (HEAD 2,368), 0 lost, originals kept 106 / 266 (126): the
  nearest-first order keeps a q90-q97 re-save where the smallest-first kept q85.
- IMGS-AGAINST: 0 lost; `repeat` 7,249; held files that are `repeat` 3,520 / 4,430 (HEAD 3,317); named
  keeper does not hold 183 (443); false library claims 73 (71).
- Found corpus: DELETE 126 (HEAD 125), REVIEW 296 (300), KEEP 1,468 (1,465); 22 files moved; pairs
  identical.
- Originals or a lossless copy kept: IMGS-ALL2 351 / 364, IMGS-ALL 357 / 364; no deletion rests on an
  `upscale` alone.
- Cost, the suggestion stage, IMGS-ALL2 cached, base new new base: 6.5, 12.5, (10.4 with the matching
  taken), 7.0 s of a ~140 s run. The tone curve reads each check's samples once more than the line did.
- Unit tests 243 pass.
