# v66: a proxy of DISC21 shard 0 for `validate.py`

2026-11-12. The battery's DISC21 stage was the whole shard (50,000 queries
against 50,000 references): about 40 minutes a build to fill a cache and 15
a run, some three of the battery's seven hours. User: too long, a proxy
instead.

## The proxy

`benchmark/disc_proxy.ids`: the 529 queries and 529 references of shard 0's
true copies, 5,000 other queries and 10,000 other references of the shard
(the sample of 2026-11-01 in `~/Documents/DISC21/proxy`, drawn at random
then; kept as a list because its seed was not). 5,529 against 10,529. A cold
fill is about 4 minutes, a cached run under one.

## Does it see what the shard sees

Four builds whose shard figures are known, each against HEAD (f1e62ab),
bar 0.5, cached on caches each build filled. True copies of 529, µAP,
difference from HEAD:

| | shard 0 | proxy | `p2` (5,477 x 25,265) |
|---|---|---|---|
| HEAD | 382, 66.89 | 392, 73.50 | 382, 71.55 |
| HEAD `-k 50` | 364, 64.84 (-18, -2.05) | 382, 71.79 (-10, -1.71) | 376, 70.49 (-6, -1.06) |
| HEAD `--skip-mirrored` | 339, 60.12 (-43, -6.77) | 352, 66.08 (-40, -7.42) | 343, 64.47 (-39, -7.08) |
| `h1` (v65, rebuilt) | 371, 63.68 (-11, -3.21) | 388, 72.58 (-4, -0.92) | 385, 72.15 (+3, +0.60) |
| `h3` (v65) | 381, 66.48 (-1, -0.41) | 394, 73.85 (+2, +0.35) | 390, 73.12 (+8, +1.57) |

(`h1` and `h3` on the shard are v65's figures, their shard caches are gone;
`h1` here is `full-512-step.patch` with `snapshots/pipeline.h1.rs` and
`analysis.h1.rs`.) False copies at 0.5: shard 393, proxy 48, `p2` 50.

- The proxy has the sign of all four. A loss of retrieval reach (`-k`, no
  second look) reads at a half to all of the shard's.
- `h1`'s loss is under-read: -4 copies for the shard's -11, and its µAP
  loss, which on the shard is true copies ranked below 393 false ones on
  weaker evidence, hardly shows where there are 48.
- Two builds the shard calls level are not the same list: `h3` loses 12 of
  HEAD's copies and gains 14, `h1` loses 18 and gains 14. So a difference
  of a few copies is within what level builds differ by, and "base's true
  copies not found" is marked `lost`, not `WORSE`.
- `p2` (the true copies, every other reference of the rest, every tenth
  query; `p2-refs.ids`) costs twice as much and reads `h1` as better than
  HEAD: more references did not make the proxy more like the shard. Not
  used.

Not tested: design A of v65 (`g8`, -33 on the shard), whose build could not
be remade from what was kept. Its causes were narrow windows and no deep
second look, which are the two HEAD rows above.

## What changed

First `validate.py`'s `disc` became the proxy and the whole shard a stage
run only when named (`validate-head-h1.txt` is that stage's run, HEAD
against `h1`, 3 minutes cached). Then, the battery not being run for every
release (user, same day): `disc` is the whole shard again and the proxy is
`--stages proxy`, only when named; both read one cache.

## Files

`one.sh`, `cal1.sh`-`cal3.sh`, `score.py`, `log.txt` (CPU-s, wall, scores;
`p-` proxy, `s-` shard, `q-` `p2`). Reports, found lists and caches were not
kept.
