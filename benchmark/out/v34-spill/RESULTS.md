# v34: analysis arrays in an mmapped scratch file (spill.rs)

IMGS-ALL, --work-size 512, --no-cache, plain x86-64 builds, ab.py (bench.py protocol: idle 76.2 C,
ceiling +3, 60 s cooldown, corpus evicted). Order base, spill, spill, base.

| run | binary | CPU-s | sys | wall s | MHz | heap+swap peak MB (RssAnon+VmSwap) | PSS+swap peak MB |
|---|---|---|---|---|---|---|---|
| 0 | base  | 2,353 | 36.0 | 338 | 2,214 | 2,334 | 2,340 |
| 1 | spill | 2,488 | 39.2 | 351 | 2,024 | 1,062 | 2,344 |
| 2 | spill | 2,494 | 37.9 | 357 | 2,015 | 1,015 | 2,349 |
| 3 | base  | 2,518 | 35.0 | 357 | 2,071 | 2,327 | 2,338 |

All four reports identical (1,127,898 pairs; F1 0.982, P 99.6%, R 96.9%).
Heap peak 2,330 -> ~1,040 MB. PSS unchanged: it counts the scratch file's mapped pages, which
the kernel may drop and re-read. CPU: the two base runs differ by 165 s; spill sits between them
(+~3 s sys).

## Where the remaining peak was (memline.py timeline, -v, 20 Hz)

Spill build: heap peak 997 MB at 424 s, in output assembly (after corroboration, before groups):
pre-copy edge lists kept alive by shadowing (~110 MB; `with_copies` also duplicated the list when
there were no copies), dump-only verdicts `all_direct`/`variant_all`/`all_propagated` held to the end
(~65 MB), and 2.3 M path strings in the 1.13 M `OutPair`s.

Fixed (img-fp-assembly: `with_copies` by value, dump-only lists dropped after the dump, `OutPair::a/b`
as shared `Arc<str>`): heap peak **776 MB** at 408 s, report byte-identical to all-0-img-fp-base.json
(timeline.txt; previous run in timeline-spill.txt). Still assembly: ~370 MB at its start, +105 graph and
its seen-set, +~110 suggestion, +~180 OutPairs and the stable par_sort's buffer. Matching peaks ~390 MB,
analysis transients ~300 MB.
