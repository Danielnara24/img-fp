# v72: the new defaults, correlation 0.65 and points 5 (2026-11-14)

0.85.0 with the defaults moved (`src/lib.rs`) and `suggest::CUT` 2/3 -> 5/7
(the `--suggest correlation` cut stays 0.9 at the default bar). No bar passed:
what a user gets. `batch.sh`; cached on v71's cache and DISC21 `cache57`.
Old defaults (0.7 / 10) from `v71-bars` (`sweep/*-c0.7`, `fam/base`).

| | IMGS-ALL | IMGS-ALL2 | IMGS-ALL3 |
|---|---|---|---|
| F1 | 0.9749 -> **0.9802** | 0.9417 -> **0.9566** | 0.9213 -> **0.9377** |
| precision | 98.83 -> 98.77% | 98.61 -> 98.53% | 98.53 -> 98.35% |
| recall | 96.18 -> 97.28% | 90.11 -> 92.95% | 86.50 -> 89.60% |
| traps | 3,207 -> 3,977 | 4,128 -> 4,886 | 3,625 -> 5,706 |
| cross-family | 12,334 -> 12,612 | 13,493 -> 14,286 | 13,487 -> 14,259 |
| seed pairs past 300 | 4 -> 4 | 3 -> 3 | 4 -> 4 |
| perfect transforms | 53 -> 52 | 12 -> 18 | 23 -> 26 |
| keep rule (`redundant.py`) | 100% / 69.5 -> 69.61% | 100% / 56.4 -> 56.33% | not scorable* |

*`suggest_score.py` knows the first two catalogues' variants only: on
IMGS-ALL3 it reads every file as unedited, so its 100% / 20.4% says nothing.

DISC21 shard 0: true 317 -> 348, false 81 -> 125, µAP 57.48 -> **62.49**,
recall at 90% precision 47.83 -> 49.72.

Small families (`fam/`, `fam.compare`), total joined:

| | 0.7 / 10 | 0.65 / 5 | different scenes |
|---|---|---|---|
| IMGS-ALL | 65.3 | 86.0 | 0 -> 0 |
| IMGS-ALL2 | 63.0 | 85.7 | 0 -> 0.7 |
| IMGS-ALL3 | 63.0 | 85.0 | 0 -> 0 |
| IMGS5 | 40.7 | 51.7 | 0 -> 0 |
| HOLI-DEV | 969.7 | 1,114.0 | 0 -> 2 |

The different-scene joins: IMGS-ALL2 `n2-d1` (one draw of three), IMGS4
`club` with IMGS `6` and IMGS3 `cross1`; HOLI-DEV `f`, 126400 and 126402
with 126500 (Holidays groups 1264 and 1265; 14 points at 0.695, and 0.668).
The correlation bar alone (v71 `fam/c0.65`) made the same.
