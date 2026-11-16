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
