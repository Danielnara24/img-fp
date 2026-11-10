# IMGS-ALL2, hard version (fifth round), img-fp 0.74.0 cold, 2026-10-27

Rounds (each a clean regeneration then this run): v47-all2 (first pass, 51
perfect) -> v48-all2-hard (round 3, 14 perfect; `void/` is a resumed build
whose files had shifted names, discarded) -> v49-all2-r4 (14 perfect, a
different 7 after the one-seed flips) -> v50-all2-r5 (7 perfect, all exempt).
Probes on the 46 changed transforms alone: /home/daniel/Documents/IMGS-probe.

| | v47 (first pass) | v50 (hard) |
|---|---|---|
| F1 | 0.9681 | 0.9356 |
| precision | 99.59% | 98.59% |
| recall | 94.18% | 89.01% |
| perfect transforms | 51 / 88 | 7 / 88 |
| traps / cross-family | 3,566 / 1,817 | 4,059 / 13,515 |
| keep rule | 0 lost, 357, 364 | 0 lost, 352, 364 |
| CPU-s | 3,043 | 2,545 |

Per transform: per_transform.csv. Small families: out/v51-families.
