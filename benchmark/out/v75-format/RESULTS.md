# v75: identical sets stated once in the JSON, and validate.py on 0.85.0 against it (2026-11-16)

## 1. The format change is lossless

The JSON's `identical` lists each set of byte-identical stated files (against
a library, scanned files only), and `pairs` names only each set's first file
by name. `same.py` expands both reports through `benchmark/imgfp_json.py` and
compares every pair by its two names, points, overlap, correlation and flags.
`img-fp-pre` is 5f74257 (the commit before), `img-fp-new` the change; one
cache (`cache/c.db`), `-r -x '*' -t 8`; `check.sh`.

| run | pairs written, pre | written, new | stated (both) | identical sets | differ | groups / files | JSON bytes, pre -> new |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| IMGS-ALL | 1,366,634 | 1,282,971 | 1,366,634 | 737 | 0 | equal | 373.0 -> 352.0 MB |
| IMGS-ALL2 | 1,325,204 | 1,277,690 | 1,325,204 | 382 | 0 | equal | |
| IMGS-AGAINST | 653,882 | 635,349 | 653,882 | 179 | 0 | equal | 186.7 -> 181.8 MB |
| 2,000 copies + a crop, `--no-cache` | 2,001,000 | 1 | 2,001,000 | 1 | 0 | equal | 741.1 -> 1.3 MB |

The first IMGS-AGAINST run differed by 415 pairs: library sets were listed too,
and a set says each two of its files are a pair, which two library files never
are. Only scanned files make a set now. The 2,000 copies: 7.6 s and 435 MB
peak either way (the run still holds the pairs; only the report is smaller).

## 2. validate.py

`validate.sh`: base `img-fp-085` (v0.85.0), new `img-fp-new` (0.65 / 5
defaults + this format). All three builds read each other's cache records (one
analysis), so both share `cache/` and `disc-cache/`. Results in `validate/`,
`validate.report`.

Finished 2026-11-16 07:5x, exit 1 (marks below). The base/new difference is
the 0.65 / 5 defaults (the format is checked in section 1).
- DISC21 shard 0: 0.5 true copies 382 -> 386 (none of base's lost), µAP 66.89
  -> 67.28, false 393 -> 450; 0.7: 317 -> 321, µAP 57.48 -> 58.08.
- F1 IMGS-ALL 0.9749 -> 0.9802, IMGS-ALL2 0.9417 -> 0.9566 (as `v72-defaults`);
  `--fast` 0.9500 -> 0.9680, 0.9046 -> 0.9372. Seed pairs past 300 at the
  default level (4, 3).
- WORSE: small families total joined 65.3 -> 86.0, 63.0 -> 85.7 (the cost
  `v72-defaults` recorded); keep rule original or lossless kept 357 -> 356,
  351 -> 349, content lost 0 both; IMGS-ALL perfect transforms 53 -> 52
  (IMGS-ALL2 12 -> 18). New: under `--fast` one more seed pair past 300 on
  each corpus (IMGS-ALL Mural1/2 327, IMGS-ALL2 Floriana1/2 343), which
  `v72-defaults` did not measure. Accepted (user, 2026-11-16).
- Detail steps: 0 true pairs lost at each step on both corpora.
- Cost: level everywhere (cached IMGS-ALL2 -2.9% CPU).
