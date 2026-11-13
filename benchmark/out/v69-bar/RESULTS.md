# v69-bar: the whole-frame bar where edits have copies of their own (2026-11-13)

Follows `v68-recall` (its cache and builds). The user's question: set the same-frame bar
(`WHOLE_RESIDUAL`, 12) below the first wrong deletion, on the case no corpus held: an edit that was
copied in its turn.

## 1. The corpus (`editcopies.py make`)

`~/Documents/IMGS-EDITCOPY`: 9,065 families of four files, over 26 kinds of edit: an original, its
`jpeg_high` copy, the edit, and a JPEG q90 re-save of the edit. Wrong: the original and its copy both
DELETE with only the edit's side staying. `editcopies.py run BINARY NAME`, `score A B`.

## 2. The hole was the rule, not the bar

| | families wrong | deletions wrong |
| --- | ---: | ---: |
| HEAD | 2,563 | 5,126 |
| `v68-recall` before this | 2,236 | |
| with the two-versions rule (final) | 319 | 630 |

`corroborated` asked whether the keeper had a faithful copy, never whether the other picture had one
too. Now a different picture of the frame that is itself corroborated is not deleted: both versions
stay, REVIEW (`two_versions_each_with_a_copy_are_left_for_a_look`). On the main corpora: IMGS-ALL2
15,801 -> 15,790 deleted, IMGS-ALL 19,207 -> 19,224, 0 lost.

## 3. The sweep (`sweep.out`, with the rule; `img-fp-bar` reads the bar from the environment)

| same-frame bar | 2 | 3 | 4 | 6 | 8 | 12 | 16 | 24 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| families wrong | 192 | 248 | 276 | 301 | 306 | 319 | 322 | 332 |

No bar is below the first wrong deletion, and there is no cliff. 12 stays.

## 4. What is left (final, deletions wrong by edit)

`watermark_corner` 170, `jpeg_generations` 96, `channel_swap` 46, `temperature` 44, `noise` 38,
`saturation` 38, `posterize` 28, `cinematic_grade` 24, `contrast` 24, `brightness` 21, `duotone` 18,
`hue_rotate` 18. The edit reads as a faithful copy of the original (`faithful`: grey grid within `TAU`,
softness within `SOFT_TIE`), so the family is one version with three copies. Colour-only edits cannot
be seen: every comparison is on grey, and `Traits.colour` is one number, not measured for a JPEG at
analysis. Open: a `faithful` that reads colour (chroma per file, a change to the analysis).
