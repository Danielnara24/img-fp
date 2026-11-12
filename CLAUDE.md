# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from: read it before
changing how anything is scored.

Every rule below was measured; the measurements are in `HISTORY.md` and
`benchmark/out/`. The window's notes are in `src/gui/CLAUDE.md`, the Flatpak's in
`flatpak/CLAUDE.md`, the competitors' runners in `benchmark/CLAUDE.md`.

## Where things stand (2026-11-10, 0.84.1 + the cache fix)

What each version changed, how it was proved and its numbers are in
`HISTORY.md` (from 0.47.0 on; before that, `git show 5de81bc:CLAUDE.md`).
Read the entry before changing what it built, or before quoting a figure not
given here. Each `benchmark/out/vNN/` holds a session's scripts, outputs and
`RESULTS.md`.

**Baselines** at the defaults, cached on caches this build filled
(`out/v62-floor`, HEAD rows of `table.out`):
- IMGS-ALL: F1 **0.9749**, precision 98.83%, recall 96.18%, perfect
  transforms 53 / 87, traps 3,207, cross-family 12,334; past 300 false
  pairs: Fountain3/5, Floriana1/2, bird1/2, Floriana3/4 (all IMGS5).
- IMGS-ALL2: F1 **0.9417**, precision 98.61%, recall 90.11%, perfect 12 /
  88, traps 4,128, cross-family 13,493; three seed pairs past 300.
- Seed pairs past 300 at the far steps (same table): IMGS-ALL overlap 0.6: 6,
  0.5-0.4: 8, 0.3-0.25: 9, correlation 0.4-0.2: 6, points and `-k`: 4;
  IMGS-ALL2 overlap 0.6: 6, 0.5-0.25: 8-9, correlation 0.4-0.2: 7-9, `-k`
  10: 2. Every one is two photographs of one scene.
- Keep rule (0.84.0): 0 lost on both; originals or a lossless copy kept
  IMGS-ALL 358 / 364, IMGS-ALL2 351 / 364; faithful copy 364 / 364;
  `pairs.py` loss 26; IMGS-PROVENANCE 126 / 266 originals, 0 lost.
- DISC21 shard 0 (50k dev queries `--against` one 50k reference zip, 529
  true copies): µAP 66.69 at bar 0.5 (recall at 90% precision 52.74), a real
  0.7 run 57.48. Whole dev set (0.76.0, 20 shards): 65.3 at 0.5, 59.9 at 0.7;
  SSCD's published 61.5-63.7 (72.5-75.3 with score normalization).
- The field (`benchmark/BASELINE.md`, `out/v54-field`, 0.75.0): img-fp F1
  0.974 / 0.939 on IMGS-ALL / IMGS-ALL2; SSCD, the best of the rest, 0.729 /
  0.472 at ~20x the CPU.
- Cost, cold, `--no-cache`: IMGS-ALL2 ~2,700 CPU-s (~6 min), found corpus
  ~840 CPU-s.

**Defaults, and why**:
- `--min-pixel-correlation` **0.7**, chosen for what users want grouped,
  not for F1: 0.5 grouped merely similar photos, 0.6 paired five of IMGS5's
  sixty seeds with their near-miss sibling directly. 0.8 was tried and
  reverted (four times the recall cost of 0.6 -> 0.7; DISC21 µAP 0.607 ->
  0.494). It does not stop IMGS5's merges: those go through clean anchors.
  Below the bar, `CLUSTER_SLACK_CORRELATION` (0.1) lets pairs inside a
  cluster stand; a hard floor was measured and rejected (*Tried and rejected*).
- Working size **512** (256 under `--fast`, a separate cheaper analysis).
  `--detail 1024 | 2048 | 4096` adds finer levels above it and states
  everything the default states (0 true pairs lost at each step); on
  DISC21, whose pictures are ~1024 px, it costs +72% CPU for nothing. "A
  step that finds nothing costs nothing" is not promised (the cost is
  describing the level; `out/v60-detail-cost`).
- The released binary is the **plain x86-64 build** (`target/plain`, which
  `bench.py` measures). It differs from a native build only on JPEG XL
  (jxl-grid's FMA rounding). There is no `.cargo/config.toml` with
  `target-cpu=native` (a native build on an AVX-512 host SIGILLed on AVX2
  machines). `check_cpu` reads `cpuid` itself (`lib::cpu`), since
  `is_x86_feature_detected!` is constant for features the build assumes,
  and refuses with raw `write` + `_exit` (formatting ran AVX-512 code).
- Grey is BT.601 luma; a JPEG's grey is its Y plane.

**Precision is measured where siblings meet.** In the full corpora a seed's
candidates are its own ~90 variants, and a vocabulary trained on them is
finest around each seed, so a near-miss sibling's matches collapse and it is
never paired: every precision, cross-family and "no merge" figure on the full
corpora, the cliffs, and every constant justified by a merge cliff
(`CLEAN_WORST_BLOCK`, `GRID / BLOCK`, the flat-block bar, the inlier
tolerance, the claim-graph survey) is **not to be trusted as safety**.
Recall, pixel-check values and the keep rule on IMGS-ALL are. Score precision
with `benchmark/families.py` (`run BINARY OUT --cache C`, `score`,
`compare`; pass `--corpus`): seeds alone, seeds + 1 / 2 variants (three
draws each), seeds among the found photos; the figure is **total joined**
(seed pairs reported as one picture), and HOLI-DEV / HOLI-TEST count
different-scene joins apart from siblings. Where families are small,
correlation is the only bar that separates siblings, and some (Segovia1/2,
game2/3, bird1/2's burst) pass even at 0.9-0.99. On IMGS every false pair
is a rearrangement trap; on the near-miss corpora stray files join a
sibling's family. **Watch merges, not pairs.** History: HISTORY.md, *Precision
is measured where siblings cannot meet* and *Scoring on small families*.

**Open**:
- A verdict depends on what else is scanned (the vocabulary is the scan's):
  the same pair passes in a folder of 60 and not of 5,460; adding one file
  to the found corpus moved 818 pairs out and 393 in. Fixed seeding was tried
  and rejected.
- Keep rule: `pairs.py` loses 26; lossy documents (two JPEG invoices); which
  of several lossy copies is the original (IMGS-PROVENANCE 126 / 266); grey
  originals beaten by a tinted copy on "colour over grey"; IMGS5's keep rule
  never scored (DELETEs resting on a sibling).
- N copies of one picture are stated as N(N-1)/2 pairs (3,000 copies: 4.5 M
  pairs, a 1.65 GB JSON); stating an identical set once is the user's call
  (the format).
- `WHOLE_RESIDUAL` (12) is a ramp with no cliff: faithful crops, turns and
  resizes read 5-37, same-frame re-encodes p99 2.1.
- A 48 px thumbnail of a 700 px photo agrees at 0.87 (both sides hold a pixel
  a cell; a two-pixel floor would be a new constant).
- No downscaled JPEG decode path (~4%); nothing tested above ~150k images;
  on the found corpus about half of sampled pairs are two specimens of one
  species (the propagated tier).
- No accuracy check in CI. The `ref.sh`, `last.sh`, `lastag.sh` batteries
  are lost; `benchmark/validate.py` (2026-11-12) is the battery a matcher or
  analysis change answers to. Its DISC21 stage (the whole shard) and
  `cold` have not been run through `run` yet. `--stages proxy` is a sixth
  of the shard for a first look, five minutes a build (`out/v66-proxy`):
  it has the sign of every loss tried at a half to all of the shard's
  size, but read `h1`'s -11 copies as -4, and level builds differ by a
  few copies there.
- The keep rule loses content once on IMGS-ALL2 at HEAD (a
  `phone_screenshot` composite deleted; `validate.py`'s first run), where
  the baseline above says 0. Not looked into.

Docs split: `benchmark/BASELINE.md` (competition; the held-out experiment's
write-up, `VALIDATION.md`, was deleted 2026-10-28), `README.md` is **users only**: install and usage, no
benchmark figures, comparisons or local measurements; same for `--help` and
the man page generated from `Args`. Hidden flags (`hide = true`, re-added in
`print_man` since `clap_mangen` drops hidden args): `--dump`, `-k`,
`--no-last`, `--prune-cache`.

## Layout

```
src/
  lib.rs        Args; cli_main, worker_main
  pipeline.rs   one scan as stages: run -> analyse_all -> index_all ->
                retrieve_and_verify -> second_look -> cluster -> assemble
                -> finish
  retrieve.rs   the index a file asks (`Index`): quantise, candidates (query,
                rank, merge over variants), the postings' and vocabulary's
                lifetimes
  main.rs       img-fp: calls cli_main
  analysis.rs   one file's analysis (`Item`) and what reading it came to
                (`Read`); `analyse`, `Planes`
  gui/          img-fp-gui (feature `gui`, GTK 4.10+)
  decode/       a file to a grey working plane, its traits, or a preview
    mod.rs      public API; `open` (the one choice of format), probe, previews
    sniff.rs    which format a file's bytes are, and whether they are one
    picture.rs  Picture, Facts, Sink, Rows, Layout; orientation
    jpeg.rs png.rs tiff.rs jxl.rs heif.rs other.rs   one module a format
    reduce.rs   the analysis's Sink: rows -> working plane; area resampling
    measure.rs  Traits measured on the reduction's rows; CellColour
    shown.rs    the preview's Sink
    budget.rs   decode budget, max_alloc, cgroup room
    tests.rs    + `fingerprints`, the per-file differential check
  heif.rs       libheif loaded at run time (libloading), not linked
  sift.rs       local features; `Variant` and `Features::seen_as`
  index.rs      vocabulary tree, inverted file, containment scoring
  verify.rs     correspondence, geometry, pixel check; `Policy` and `Standing`
  claims.rs     Edge, Clusters, bridge test, admission, propagation, Claim/How
  stated.rs     the pairs a run states: what each rests on (`Stated`) and
                what it says (`Pair`: evidence, scale, place, clean)
  same.rs       which files are one file (bytes, analysis, colour), and their pairs;
                the byte pass (`exact_groups`: size, sample, hash, bytes)
  group.rs      pairs -> groups around a representative
  walk.rs       roots -> files (lists, -x, --exclude, symlinks, dedupe by inode)
  cache.rs      on-disk analysis cache: an SQLite index (path -> key, content
                hash; hash + analysis version + settings -> place in a pack)
                and append-only zstd packs; answers for a run's files by
                number, and decides what is kept (`Store::finish`)
  matched.rs    the last run's matching, taken by a run with nothing changed
  thumb.rs      the thumbnail an analysis keeps (`Thumb::build`)
  outcome.rs    a finished scan (groups by number, or against a library each
                scanned file's pairs; evidence, content rule's suggestions),
                as the report, --last and the window read it
  report.rs     text / CSV / JSON reports of either kind, written from an
                Outcome; each layout's rows a `table::Table`
  outputs.rs    where a run's outputs go (`plan`: report, dump, log, list;
                every rule about them), and how each is written (`To`)
  table.rs      a record written as columns (`Column`, `Cell`): CSV header,
                CSV cells and JSON keys from one list
  suggest.rs    KEEP / DELETE / REVIEW per file
  against.rs    --against: each scanned file's verdict, read off an Outcome
  last.rs       --last (the last scan's Outcome, kept beside the cache)
  problems.rs   skips, problems, exit codes, --log-file; one `Note` a category
  progress.rs   progress bar
  simd.rs       x86-64-v3 vs portable kernels (`dispatched!`, `v3()`)
  spill.rs      analysis arrays in an unlinked scratch file, read via mmap
build.rs        sets cfg(dispatch) for x86-64 builds without AVX2
flatpak/        the window as a Flatpak: manifest (GNOME 51 runtime, libde265
                + libheif 1.23 built in), cargo-sources.json, metainfo, icon,
                screenshots (see *The Flatpak*)
benchmark/
  bench.py      measurement harness (cold, sequential, instrumented)
  score.py      F1 vs generated ground truth
  analyse.py    + traps vs cross-family, merges, two seed halves
  suggest_score.py / against_score.py   score `action` / --against reports
  validate.py   a build against a base build on everything a change is
                judged by (DISC21, both corpora, --fast, containment, small
                families, the keep rule, cached and cold cost), resumable;
                `report` marks what got worse
  pairs.py      the keep rule on two-file families (each transform alone)
  replay.py     replay anchor tier + bridge test off one --dump
  runners/      one wrapper per competitor -> canonical JSON
  corpus/       transforms.py (CATALOGUE), make_variants.py, make_combined.py,
                make_against.py
  out/vNN/      measurement sessions; v54-field is the field's baseline
vendor/         third-party tools and venvs (gitignored)
```

## Corpora

- **IMGS-ALL** `/home/daniel/Documents/IMGS-ALL`: hard links to the
  corpora, ground truths merged, built by `corpus/make_combined.py`. Default
  for `bench.py`, `score.py`, `analyse.py`. Re-run with `--clean` after
  `make_variants.py` (links go stale). **Since 2026-10-27 it holds all five
  (IMGS-IMGS5: 33,119 files, 364 seeds, the first generation's 90
  transforms)**; every IMGS-ALL figure in this file before that date is the
  four-corpus one (27,659 files, 304 seeds; rebuild with `make_combined.py
  --parts IMGS,IMGS2,IMGS3,IMGS4 --out DIR`). Baseline, 0.74.0 cold
  (`out/v52-all5`): F1 0.9735, precision 98.88%, recall 95.86%, perfect
  transforms 51 of 87, traps 2,955, cross-family 11,932 (IMGS5's sets:
  Fountain3/5 6,645, Floriana1/2 2,884, bird1/2 799, Floriana3/4 474, Mural
  325 over 300), 3,133 CPU-s; keep rule 0 lost, 358 / 364, 364 / 364. TP
  and FN within 0.03% of 0.73.0's four-corpus and IMGS5 runs added.
- **IMGS-ALL2** `/home/daniel/Documents/IMGS-ALL2`: the **hard** corpus (built
  2026-10-27). The same 364 seeds as IMGS-ALL, 91 transforms, generated in
  `/home/daniel/Documents/IMGS-v2/<part>` (seeds hard-linked from the
  originals; `CLEAN=1 IMGS-v2/generate.sh` regenerates and merges, ~17 min;
  without `CLEAN` it resumes, which is only safe after an interruption: a
  changed format moves every later file's name, and the resume reused files
  under other transforms' names once, `out/v48-all2-hard/void`). Where and
  which way are drawn per seed as well as how much (crops anywhere, strips
  off any side, keystones from any edge, rotations, shears, swirls either
  way, stickers / watermarks / insets in any corner, tile orders:
  `transforms._oriented`). Then, in five rounds, every transform 0.74.0 found
  for all 364 seeds was made harder in kind until none was (`out/v47-all2`
  to `out/v50-all2-r5`): stronger ranges (`rotate_large` 12-45°, shear
  0.30-0.50, swirl 25-80°, JPEG quality to 1, posterize 2-12 colours, noise
  30-90 on a smaller copy, blur 0.8-2.4% of the long side), and an exact
  operation made as it is met (`transforms._shared`: a turned, mirrored,
  graded or captioned copy is also smaller and re-saved; greyscale through a
  colour filter; a negative graded; channel swap any order, one channel may
  be inverted; duotone inks per seed; an upscale is of a small copy). Left
  perfect on purpose: `copy_exact`, `strip_metadata`, `to_png`, `to_tiff`,
  `webp_lossless` (identical pixels by definition) and `alpha_on_*` (a plain
  re-save on an opaque seed). Renamed: `crop_strip_top` -> `crop_strip`,
  `perspective_top` -> `perspective`, `keystone_side` -> `perspective_two`
  (both axes), `wave_vertical` -> `wave`; `pinch` new.
  Baseline, 0.74.0 cold (`out/v50-all2-r5`): F1 **0.9356**, precision
  98.59%, recall **89.01%**, perfect transforms 7 of 88 (the seven above),
  traps 4,059, cross-family 13,515 (Fountain3/5 6,010, bird1/2 5,587,
  Floriana1/2 483, Floriana3/4 372 over 300), 2,545 CPU-s; keep rule 0
  lost, 352 / 364, 364 / 364. Hardest: `pinch` 242, `tiled_watermark` 248,
  `swirl` 263, `wave` 274, `crop_micro` 279, `perspective` 291, `blur` 307.
  15 transforms are at 363 / 364, nearly all one seed (`earth.jpeg`), which
  flips between runs: one seed is not a margin. Small families
  (`families.py ... --corpus all all2`, `out/v51-families`): total joined
  IMGS-ALL **66.0** (n0 17, n1 16.0, n2 17.0, f 16), IMGS-ALL2 **63.7** (n0
  17, n1 14.0, n2 16.7, f 16); n0 and f are the same seeds in both, so they
  agree by construction. Pass `--corpus` explicitly; defaults name IMGS-ALL.
- **IMGS** `/home/daniel/Documents/IMGS`: 62 seeds. Seed `beach` has no
  extension, so comparable runs need `-x '*'` (`bench.py` passes it).
- **IMGS2** (design), **IMGS3**, **IMGS4**: photographs chosen in near-miss
  sets (same scene a moment / step apart), so each set is a family of hard
  negatives. IMGS3 and IMGS4 were held out and are now **spent**; the next
  rule to defend needs fresh seeds.
- **IMGS5**: 60 seeds in 25 near-miss sets, many a portrait or closer shot
  from the landscape's own spot; not in IMGS-ALL. Held out until 0.54.0,
  read once then (`HISTORY.md`, *IMGS5*); now spent too. Score with
  `analyse.py --corpus=/home/daniel/Documents/IMGS5`.
- **IMGS-ELONG**: 33 elongated pages of 2-10 photos (68 positives), for the
  area rule.
- **IMGS-AGAINST**: IMGS-ALL split into scan + library as symlinks; run with
  `--follow-symlinks -r -x '*'`.
- **Found corpus** `/home/daniel/Downloads/archive` (exactly that path:
  `~/Downloads` adds a stray file that changes pair counts). 9,285 224x224
  photos, unlabelled: **cost and byte-identity only, never accuracy.**

Regenerate: `cd benchmark/corpus && ../../vendor/venv/bin/python
make_variants.py --clean` (deterministic). Every transform amount is drawn
per seed from a hash of the seed's thumbnail (`seed_key_of`); transform and
its recorded region/coverage come from one factory (`_crop_jit`,
`_rotate_jit`, `_scales`) so the ground truth cannot drift from the pixels.
Known failure: `strip_metadata` on AVIF is deleted by the generator.

## Ground truth: generated, never labelled

The most important decision in the project. Hand-labelling found photos
measured the labeller's bias and was deleted. Generated truth: positives are
same-seed pairs whose regions say SAME or CROP; negatives are different seeds,
or same seed with disjoint regions or different scramble; PARTIAL is excluded.
One F1.

`pair_relation`: (1) different scramble keys -> DIFFERENT, always;
(2) region IoU >= 0.95 SAME, one contains >= 95% of the other CROP, disjoint
DIFFERENT, else PARTIAL; (3) coverage < 90% of the other's downgrades SAME to
CROP.

## The pipeline (`pipeline.rs`)

`run` is a list of stage calls; each stage is a function that takes what the
one before handed it, returns what the next reads, and lets go of the rest
(memory peaks are kept by who owns what, not by `drop` order in one scope).
What a run was asked about is a `Scope` (`asks_first`, `reached`,
`re_askers`, `speaks_of`, `pinned`): a plain scan is the scope with no
library, and the library rounds of retrieval are the same loop, which ends
after one round when every file has asked. `Scope::against` is read directly
only where the library mode does something else, not the same thing with
fewer files: the second look's per-component re-ask (`re_askers`), the
round's already-verified set, the report kind, and `--last`. Both
verification passes put a candidate through `Pass::verify`, which keeps
every verdict that stands at all; the second look's go into the same list as
the direct pass's (`Verdict::variant` says which asked). `--dump` is a `Dump` the stages hand their verdicts to.
Every run ends in `finish` (verdicts or groups; an empty walk is the same
path with nothing `Found`). Logging goes through `Ctx` (`stage!`, `say!`).

The matching (`match_all`) is a list of steps (`detail_steps`): the working
size, then each doubling up to `--detail` some picture reaches. A run that
reaches none, the default's among them, is the one step. The first step is
the default's matching whatever the detail. A finer step is a `Finer` on
its `Pass` (scale bounds, the verdicts earlier steps settled, the files
with a new level), which `Pass::verify` and `retrieve_and_verify` read.
Its other differences are in the loop: windows a fifteenth of `-k`, words
added to the first step's vocabulary (`index_adding`), no second look, and
the veto on joins of two families the first step holds apart. Until
2026-11 the steps were a second function beside the default's
(`match_by_detail`), and its first step kept the better of two verdicts the
second look met from both ends under one variant (`merge_best`), where the
default keeps the first.

**Adding a mode or a stage**: what the run was asked is a `Scope` method;
a verdict recorded for its own sake goes to `Dump`; a new stage is a function
between two others with its inputs and outputs typed. Not an `if` on an
`Args` flag inside a stage.

## Retrieval (`retrieve.rs`)

A file asks the index under one or more variants of itself
(`sift::Variant`: `SAME`, or `TURNED`, mirrored / inverted / both, whose
features are `Features::seen_as`). `Index::candidates` is the one question:
each list it is handed is queried, read both ways round against a library
(`both_ways`), ranked (`rank_best`), and the lists merged into the best `-k`
(`merge_variants`; one list is its own merge). The direct pass asks every
file as it is (pairs from both ends deduped, verified from the lower), and
`-k` counts rejections: a file is asked again for the next window of its
ranking as long as its last window held verdicts that stand (`window`), so
its own copies do not use up the `-k` its other relations are looked for in;
the second look asks `Scope::re_askers` turned, verified from the asker
as it asks (its turned features exist only then). The index owns what the
questions need held: the postings are let go between the passes (`let_go`)
and built again for the second look (`turn`), whose askers' turned words are
made ahead where that lets the vocabulary go.

**Adding a question**: who asks is a `Scope` method, what they ask as is a
`Variant` (or a set of them), and the answer is `Index::candidates`. Not a
third retrieval loop.

## Decoding (`decode/`)

A file is identified once (`sniff::identify`: signature, then a format with
none by its name) and opened once (`decode::open`, the only `match` that
picks a decoder) by the module of its format. Opening gives a `Picture`:
`facts` (lossless, frames, damage: what the file says of itself, asked by
the analysis only) and `read`, which decodes into a `Sink`. A sink is the
question asked of the pixels: `reduce::Reduce` (the working plane, with
`Traits` measured on the same rows) or `shown::Shown` (a preview). A sink
says what will do (`streams`: 8-bit rows rather than the `image` crate's
whole picture at its own depth; `grey`: a JPEG's Y plane alone) and what it
holds (`holds`, claimed with the decoder's buffers in `picture::claim`);
`begin` starts it afresh, so a reader that gives up part way (PNG, TIFF)
hands the picture to `other::whole` and leaves nothing behind. Rows are one
of four `Layout`s; orientation travels with them and the sink applies it.
`colour_of` reads a JPEG's own colour planes (`jpeg::cells`) and anything
else through `preview(256)`; `probe` reads headers with each module's
`size`, never the whole file. Verify a change with
`decode::tests::fingerprints` on IMGS-ALL + the found corpus, base build
against new.

A file `decode` turns away for what it is is a typed error,
`decode::Refused` (`NotAnImage`, `MacMetadata`, `CameraRaw`), read back with
`decode::refused`; what it is to a run is `analysis::Read`'s.

**Adding a format or a question**: a format is a module with `open` (its
`Facts` and its reader) and `size`, and one arm in `decode::open` and
`probe_header`; a question of the pixels is a `Sink`. Not an `if` on the
format inside a sink, nor on the sink inside a reader.

## The analysis and the cache (`analysis.rs`, `cache.rs`)

A file's analysis is an `Item` whose `read` says what reading the file came
to: `Whole`, `Damaged(why)` (analysed, never cached), `Refused(decode::Refused)`
or `Failed(why)`. Skips, problems, the report's `failures`, what may be
cached and what is matched all read that one field (`tell_problems`,
`Read::ok`, `damaged`, `error`); nothing compares error strings.

**An analysis is kept by what the file holds** (0.82.0): under its file's
BLAKE3 content hash (`same::Hash`), the analysis that made it
(`analysis::version`: `IMGFP_ANALYSIS`, which `build.rs` hashes from the
analysis's sources less whole-line comments, the crates they call and those
crates' dependencies at their `Cargo.lock` versions (`analysis_crates`; 63
of 219, less `COMPUTES_NOTHING`), the target and its CPU features, plus the
`sift::Params` the run passes; not the compiler nor other flags: a rustc 1.92
and a 1.99 build write Desktop's packs byte for byte alike) and the working
size and thumbnail. Which path holds which content is a second table: path,
stat key (size, mtime, ctime), hash, sample (`decode::sample`: length and
the first, middle and last 16 KB, hashed to 64 bits), and the day a run first
found it gone. A walked file whose key is its row's takes the row's content
and is not read; a file another walked file shares a size with (a possible
copy) or the index knows a file of that size (a possible move) is sampled,
and read whole up front only when its length and sample are another walked
file's or a row's (`same::content_hashes`); every other file is hashed by
its decode from the bytes it already holds (`decode::Decoded::hash`), since
no analysis can be kept for it and it is no copy: hashed up front as well, a
cold run read its corpus twice, and with sizes alone the 3,552 IMGS-ALL
files sharing a size (0.67 GB) still were. Equal hashes are byte-identical files (`exact_groups`), no
byte comparison (user's decision). So a moved, renamed, chmod'ed or
hard-linked file, a remounted drive or a restored backup costs a read, not
an analysis, and copies share one record. The stat key is only ever compared
for its own path, which is why 0.69.0's ctime collision cannot recur.

Storage: the index is SQLite (rusqlite, bundled; a write-ahead log,
synchronous NORMAL, so a commit syncs nothing and a power cut costs the last
batches, never the index; on a network share, `on_network_share` by
`/proc/self/mountinfo`'s type, a rollback journal, synced: a log's shared
memory does not work there, and not mapped; locally mapped, `mmap_size`,
and a 64 MB page cache); the
records are append-only packs `X.pack.N`, read with one `pread`
each and packed with zstd level 1 (three streams: keypoint byte planes,
descriptors, Paeth thumbnail residuals). Measured before choosing: SQLite
holding the records as blobs was +2.7% size, 3-4x slower writes and 2x
slower cold reads than appends (9,000 x 35 KB); zstd-1 against deflate-fast
on 3,000 found-corpus records 0.709 / 0.730 of raw, packing 3.2x and
unpacking 4x faster. Each record begins with 16 bytes of its hash, checked
on read: an index and pack that disagree cost a record. A row is committed
as soon as the index is free (`Db::commit_waiting`: a worker that finds a
commit under way leaves its row to it, so none waits), and by `seal` at an
interrupt: a SIGKILL loses the rows of the commit under way (636-file
Desktop killed after 250 described: 259 kept). A damaged index (SQLite's
`CORRUPT` / `NOTADB`, at open or later) is removed by the run that has the
cache alone and the next starts afresh. `X.lock` is held shared by every
run; a run that gets it alone at `finish` folds other runs' side packs into
the newest (a run appends to the newest pack when it can `flock` it, else to
one of its own) and rewrites the packs when a quarter is unread. Kept: a row
of a walked file; of a file not walked while it is there and `GRACE_DAYS`
(30, user's decision) after a run first found it gone (this replaced
`away`, `left_behind` and `Key::dev`); an analysis while some row holds its
hash, one by another analysis version `GRACE_DAYS` unused (`used` day,
updated once a day). `--prune-cache` keeps only the run's. A libheif-decoded
record carries libheif's version (`made_by`) and is not read under another.

Until 0.82.0 (`IMGFPC04`-`21`): one append-only file of records keyed by
path + key, a `.heads` index of it, a `.same` file of pairs proven
identical, records copied for byte copies, `away` heuristics for unplugged
drives, and a format number bumped by hand when the analysis changed (missed
after 0.76.0: stale baselines). `remove_other_formats` deletes those files
from the default folder.

`<cache>.matched` (`matched.rs`) is the last run's matching: its claims and
its console notes, under a `Fingerprint` of everything the matching reads
(the build, `IMGFP_BUILD`, which `img-fp` and `img-fp-gui` of one build
share; the bars, `-k`, `--skip-mirrored`, the work size, the detail, and each
file's path, content hash, read state and side). A run with the same
fingerprint takes it after the analysis and goes on to the assembly; not
with `--dump`, nor a file with an analysis and no hash. Keyed by content
hash since 0.82.0 (it was the stat key), so a chmod or `touch` keeps it.

**Adding a record kind or a reason to keep one**: a column of `analyses` or
`files` and a condition in `Store::settle`. Not bookkeeping in the pipeline.
**Adding to what an analysis computes from**: a file under the sources
`build.rs` lists (`ANALYSIS_SOURCES`); `the_analysis_reads_only_its_sources`
fails when an analysis source names a module outside them.

## The claim graph (`claims.rs`, `verify::Policy`)

What a direct verdict may do is its `Standing`, read once from the three
bars a user sets (`Policy::standing`; `Policy::gate` is what the lowest asks,
`Policy::composed` judges a propagated one). `pipeline::cluster` is then a
list of rules over one edge list: `claims::unique_pairs`, the clean flags,
`demote_shared_parts` (a file that is part of files in two clusters, by
one-way clean links, keeps only the most-linked cluster's links clean),
`demote_bridges` (a lone clean link between two groups stops being clean),
`admit` (clean anchors join clusters; the rest join two only under the cover
test and the cycle rule), which hands back the `Clusters` that propagation
rounds (`claims::propagate`) and corroboration then read without rebuilding
them. The result is one `Vec<Claim>`, each an `Edge` and `How` it was made
(`Direct`, `Propagated`, `Corroborated`), which `Same::expand` carries to
copies and the report reads (`Stated::Claim`).

**Adding a rule**: a bar on one verdict is a `Standing` (a new level, or a
condition in `standing`); a rule about the graph is a pass over the clean
flags or the `Clusters`, between two others in `cluster`. Not a new edge
list, a second partition of the files, or a relation read off a verdict's
fields. Verify a refactor with a cached IMGS-ALL run *and* its `--dump`
against the base build, then the `ref.sh` battery and `pairs.py`.

## The stated pairs (`stated.rs`)

Every pair the run states is stated once, whichever way round it is met
(`stated::unique`, first met wins), from what it rests on (`Stated`: the
same bytes, the same pixels, or a `Claim`). The keep rule reads that
(`Stated::link`); everything after it reads the `Pair` worked out from it
once, by the run's file numbers: its `Evidence`, file scale, `Place` (`a`
in `b`; `place_from` reads it from either end) and clean flag. The groups
(`numbered_groups`), `--against` (`against::asked`) and the JSON's `pairs`
(`report::Pairs`, named from the run's files as written) all read the one
list. Before 0.63.0 each reader stated its pairs again: the JSON's records
(`OutPair`, two names and two byte strings each) were also where a group
member's evidence was looked up, last in name order; the compare view's
place was a second map (`placements`) copying that order by comparing
names; `--against` took the first pair met.

**Adding something a pair says**: a field of `Pair`, set in
`Stated::pair`. Not a side table keyed by its two files.

## The report's records (`table.rs`)

A row the run writes as columns (a group member, a file against a library,
a `--dump` verdict) is a `Table`: a list of `Column`s, each its name, whether
the CSV carries it, what the JSON does with it (`Json::Always`, a `null`
when there is nothing; `Present`, a key only when there is something;
`Never`) and a function reading its `Cell` off the row. The CSV header, the
CSV cells and the JSON object are all read off that list, in its order, so
the formats cannot drift apart and a CSV cannot shift. A `Cell::Path` is
written as its own bytes in the CSV and as `name` plus `name_bytes` (when
not UTF-8) in the JSON, everywhere. The pair's six columns are
`report::evidence_columns`, shared by both layouts. The text layout is
sentences, not columns, and is written by hand from the same rows. The
JSON's `pairs` (`report::Written`, 1.1 M records, derived serde) and
`failures` (a `serde_json::Value` kept in the `Outcome`) keep their own
shapes.

**Adding a column**: one `Column` in its table (`group_columns`,
`answer_columns`, `pipeline::dump_columns`). Not a field, a header entry and
a cell.

## Where a run's output goes (`outputs.rs`)

A run's outputs (the report, `--dump`, `--log-file`, `--list`) are placed
once, before anything is written, by `outputs::plan`, which holds every rule
about them: the report goes to stdout unless `-o` names a file or `--list`
or the window's progress has stdout; at most one output on stdout, and
under the window none; none names a picture; no two name one file, nor the
cache or the kept scan (`last-scan/found.json`, `scan.json`) while the run
uses them; each file written at the end can be (`check_writable`). The
window's pre-check (`check_args`) asks the same `plan`. Each output is a
`To` (`Stdout` or a path) and is written one way: `To::write` (stdout
through a buffer, a reader gone is the pipeline's end, an error names the
output; a file whole, `write_whole`), or for the log, written as the run
goes, `To::open_log`. A path is written as what it names (`Dest`): a
descriptor (`/dev/stdout`, `>(...)`, `>>`) as opened, a FIFO or device in
place, only a regular file replaced. Before 0.65.0 the rules were four
checks in `lib.rs`, whether the report went to stdout was decided three
times (`lib`, `pipeline::finish`, `last::rewrite`), each output wrote stdout
its own way (`--dump - | head` exited 1), and the log was opened with
`File::create`, so `--log-file /dev/stdout >> run.log` emptied `run.log`.

**Adding an output**: a field of `Outputs`, set in `plan` with its entry in
`Outputs::claims`; written through `To`. Not a check in `lib.rs` nor a
stdout writer of its own.

## The progress bar (`progress.rs`)

The bar is the share of the run's time that has passed. Every frame the
run works out the seconds it has left (`State::left`) and the bar closes
`dt / left` of its distance to the end: in step with the clock when `left`
is right, faster or slower from where it stands when it is revised, never
a jump, a step back or 100% before the end. No stage is given a stretch of
the bar in advance.

`left` is counted work at a measured pace. Each stage's work is in its own
unit (`Stage::unit`: files, bytes, descriptors, pairs; images plus direct
anchors for propagation and assembly, which cost about the same each), as
counted (`Known`, told by the pipeline with `progress.learn` when it has
counted it) or until then the most the code allows (`FEATURES` descriptors
an image, `-k` candidates, every file asked again by the second look, a
square working plane per file). A bound tightens as the stage that decides
it goes: while the cache is read, each original it is seen to hold under
its file's key is one less to describe (`cache::open`'s `met`); while
verification runs, a file anchored twice is past the second look
(`Scope::settled`, the rule `re_askers` applies) but for one file a family,
its representative, the one of most descriptors until it is known
(`pipeline::Settling`, families joined as anchors come in); against a
library nothing is settled, since a component's best-connected member is
asked however many it has.
"Every file asked again" is what a photo library is (97% of the found
corpus is asked again) and far from a folder of copies (6% of IMGS), where
the bar runs slow until verification and then catches up; half was tried:
IMGS's first run 0.021-0.042 mean error, the found corpus's 0.083 (base
0.031). The pace of a unit is measured: the running stage by itself,
the analysis file by file (each file's decode timed against its source
pixels by format, the rest against the pixels analysed: `Describing`), a
stage that has run (later library rounds; the second look, priced by the
quantising, querying and verifying it repeats) by what it took. Only a
stage not yet run uses `Stage::prior`, its cost per unit on this machine
(Ryzen 7 3700U, 8 threads), scaled by how fast this run's processor has
been against the priors so far (`Paces::speed`; the analysis, read as it
goes, past its decode, against `EXTRACT_PER_PIXEL`). Stages that wait on
the disk (identical files, headers, the cache read and write:
`Stage::computes`) time themselves but say nothing of the processor: read
into the speed, a rescan's cache read (1.6-2.4x its prior under swap) had
every stage after it expected at twice what it took. A stage that runs past
its estimate is given `expected² / 4 secs` left (`overrun`), so the bar
slows rather than reaching the end. To re-read the priors: `-v` ends with a
`pace:` line per stage, `ns a unit on each of N threads`; each prior's
comment gives the spread it was read from.

Before 0.66.0 the bar gave each stage a stretch when it began, from a
`Forecast` of about 25 constants fitted to the found corpus (pairs per
query, files with no duplicate, keypoints per image, per-format ns per
pixel and per byte, a second-look overhead) and an `analysis_cost` that
sized the working plane itself and missed the area rule. Later library
rounds and the assembly and suggestion were in no stage.

**Adding a stage**: a `Stage` variant with its label, unit and prior, its
units in `Known::units`, and a `begin` where it starts. Not a cost model of
the stage's insides in `progress.rs`.

## A finished scan (`outcome.rs`)

What a scan found is one `Outcome`: the grouped files by number (path,
size as shown, length, damaged), the groups (representative first, each
member with its pair's `Evidence`: one `Relation`, points, overlap,
correlation, mirrored, inverted) and the content rule's suggestions with
each deletion's kept copy. The pipeline builds it (`pipeline::outcome`),
`report.rs` writes text / CSV / JSON from it, and `--last` and the window
read it back (`Outcome::save` / `load`, its own JSON, paths absolute and as
bytes when not UTF-8). The rules on it are its methods: `actions(mode)`
(the group rules over groups whose representative has not lapsed; nothing
deletes a lapsed file), `lapse(ids, Gone | Changed)` (takes back what rests
on a lapsed file, `suggest::revoke`), `look` (which files are gone or
changed now), `shown` (the groups a report writes: no gone file, no group
without its representative or with one file).

A run `--against` a library is an `Outcome` too: no groups, and `against`
holds each scanned file (`Asked`) with its nearest holder and its pairs with
library files (`Match`: `Evidence` and the clean flag). Its verdicts are not
stored but read as things stand, `against::answers` (so `--last` lapses them
as it lapses groups: a file gone is not answered, nor is it one the library
has; a repeat whose holder or kept file has lapsed is answered by the
library alone), and the report writes them in its second layout. Before
0.58.0 it ended in a report of its own (`against::Row`, three writers, its
own holder-chain walk and stdout handling) and could not be kept; the text
had dropped "corroborated".

**Adding a question about a finished scan**: an `Outcome` method, asked by
the report, `--last` and the window alike. No reader parses the report: it
is for people and scripts, not for img-fp.

## How it works: the load-bearing pieces

Each was measured; changing one needs a re-measure.

- **Local features, not a global descriptor.** That is why containment
  works. Check `collage_2x2`, `embed_quarter` after any global shortcut.
- **Retrieval scores containment** (normalised by the query's idf mass), not
  cosine (-5 recall).
- **Guided matching** (0.73.0, `verify::guided`): a hypothesis with >= 3
  agreeing points has its correspondences found again by position, each of
  A's keypoints carried by the transform into B and matched to the nearest
  descriptor within the inlier tolerance under `GUIDE_D2` (320, derived:
  the distance at which an unrelated pair expects one coincidental
  correspondence), refit up to twice. Without it a verdict depends on the
  vocabulary's fineness, so on what else is scanned (the same sibling pair
  passes among 60 files and not among 5,460). Cost: +16% CPU on the found
  mix, +70% on IMGS5's matching (all its own refinement, `out/v46-matcher`
  section 6); gated to hypotheses already past the points bar it is +3.4%
  and loses ~1 point of recall.
- **`-k` counts rejections** (0.73.0): a file whose window of candidates held
  verdicts that stand asks for as many more, until `-k` are rejected. In a
  folder of many copies a sibling ranked past `-k` was never asked. A verdict
  with a file clean verdicts had already joined to the asker before its round
  is verified but not counted (`joined`): counted, a family of N copies had all
  N(N-1)/2 pairs verified (1,600 crops: 1,251,350 candidates, 169 rounds, 89 s;
  now 258,498, one round, 32 s, same groups and suggestions). IMGS-ALL (pairs
  and actions), IMGS5 (F1 to four places, 2 TP fewer), `families.py` and the
  found corpus (byte for byte) as before. Passing such files over unverified
  was tried: IMGS5 bird1/2's merged burst lost 826 true pairs (recall -0.37).
- **The geometric look** (0.83.0, `retrieve::Index::deeper`, in the second
  look): a re-asked file (see *The family's representative*) also reads its own ranking
  past `-k` down to `GEOMETRIC_DEPTH` (1,000), scores each candidate's weak
  geometric consistency (`Asker::peak`: keypoint pairs sharing a word, the
  asker's three-path words against the candidate's best word, voting in 16
  rotations x 12 octaves of scale; `Shape` holds each file's best words and
  a byte each of angle and scale, made in the same descent as its words) and
  verifies those beating the file's own null (the best peak among its first
  `-k`) with at least 3. No budget constant. Why: in a large folder a true
  copy's share of the words falls and it ranks past `-k` (DISC21 shard 0: 91
  of 529 true copies never verified, 40 passing alone); `-k` cannot reach
  them (ranks 150 to 82,000, score ~0.005 against a top ~0.16). Depth swept
  (`out/v58-reach` section 6). See `HISTORY.md`, 0.83.0.
- **The family's representative** (0.84.0, `Scope::re_askers`,
  `ReAsk`): the second look asks every matched file with fewer than two
  anchors, and in a plain scan also each family (a component of anchors)
  once, through its best-connected member, which keeps only the *clean*
  verdicts it finds; propagation carries them to the family. Why: a file
  with two copies never asked for its mirror, the same file alone did.
  Against a library: each clean-anchor component holding a scanned file,
  every answer kept (clean only lost a mirrored true copy in each of two
  DISC21 shards). Families of any anchors, not clean ones, and clean
  answers, not any: the library rule applied to plain scans cost +26% /
  +37% CPU on IMGS-ALL / IMGS-ALL2 and merged bird1/2, Mural1/2 and, at
  correlation 0.3, bust2 | Classroom1. See `HISTORY.md`, 0.84.0.
- **Shared-part rule** (0.73.0, `claims::demote_shared_parts`): a one-way
  clean link (overlap bar one side only) makes a part; a part whose containers
  lie in two clusters keeps only the most-linked cluster's links clean. A
  shared distant mountain merged Macizo1/2 (two clean links into one file are
  no bridge).
- **Inliers are the size the transform gives them** (`verify::SIZE_AGREEMENT`,
  one octave, the pyramid's step): a verdict whose inliers' median
  log2(size in B / (size in A x the transform's scale)) is further off has no
  points. A picture shrunk 3-15x into a spot of another gathered fine
  keypoints onto the spot's few coarse ones (the tolerance is the larger
  frame's, ten times wider in a picture shrunk tenfold; 27-67 points on 6-13
  keypoints), and the pixel check at the spot's 25 x 10 thumbnail pixels
  agreed at 0.70-0.76: three different-scene pairs joined on INRIA Holidays,
  medians +2.0 to +3.2 octaves. True verdicts within 0.3 of zero (one
  panorama +1.18). Counting inliers one to one instead lost tiny embeds
  (`out/v56-seed`, section 7).
- **Each direction of a transform is bounded** (0.77.0, `verify::Policy::frames`,
  `verify::axis_scales`): `MAX_SCALE` bounds its singular values, not
  √|det|. A fit through nearly collinear inliers (a wave crest, a horizon)
  is free across their line: a yacht on calm sea went into surf as a sliver
  forty times longer than wide, 61 points, correlation 0.775.
- **The vocabulary's leaves are copies** (0.77.0, `index::Vocabulary::levels`),
  at most `SAMPLE_CAP` x 128 = 164 MB: as pointers into `spill.rs`'s
  mapping the descent read it all at random, and 100,000 files on a 5.8 GB
  machine sat in quantising for 90 minutes.
- **A claim is a transform plus pixel agreement**, never a score over a
  threshold (rejects `tile_shuffle`).
- **Pixel check**: blocks flat in both pictures abstain, a block flat in
  one only counts at |r| times SSIM's contrast term (0.72.0; it abstained,
  and a person added onto a plain wall agreed at 0.9 a block); flatness is relative to the
  thumbnail's own contrast (`Thumb::build` stretches each thumbnail); blocks
  are averaged (|r|), not counted; both sides are read through a mip pyramid
  (`Thumb::lod`) at the right footprint. **Do not reintroduce a plain tap.**
  A level is the one below filtered [1 3 3 1]/8 each way and halved (0.46),
  not a 2x2 mean: a box's levels depend on where its grid falls, so a crop or
  a reflection of an odd-sided thumbnail compared out of phase (exact copies
  under the clean bar through the *exact* transform: 12/288 -> 2/288).
  Levels are kept for a run of comparisons on one thumbnail (`Thumb::id`).
- **The fitted transform is the least-squares fit**, kept even when it moves
  an inlier out of tolerance (0.46). It used to be dropped then, which was
  nearly always, leaving the one-keypoint similarity hypothesis: exact copies
  were 1-9 working px out at the corners and 110/288 fell under the clean bar.
  Probe: 48 seeds x 6 exact transforms, fitted vs exact transform's
  `blk_min` (`out/v33-fit-pyramid`).
- **`encloses_centre`**: an anchor's inliers must bracket the middle of what
  it claims (over the keypoints in the claimed region, either frame).
  Without it: 36 merges.
- **Propagation** composes transforms and re-checks pixels; worth ~7 points
  of F1 for ~6% of the clock, unconditional. BFS tree from the best-connected
  member. Later rounds only in components that gained a pair. Work per round
  is a budget (`prop_budget` = direct candidates verified); a component that
  does not fit is **starred** (each file vs. the root). Spread by row.
- **Acceptance is a `Standing`** (`verify::Policy::standing`, ordered
  `Rejected < Corroborating < Anchor < Clean`, each clearing every bar below
  it) for a direct verdict, and `Policy::composed` for a propagated one (no
  inlier count, no enclosure). `Corroborating` is the anchor's rule without
  enclosure and with the correlation bar `CLUSTER_SLACK_CORRELATION` (0.1)
  lower. Collapsing them costs recall or precision.
- **Whole-frame matches** (`Policy::standing`, `verify::frame_onto_frame`):
  a verdict whose transform puts each of A's corners on B's same corner
  (inlier tolerance; resize only: a half-turn let a `quadrant_rotate` and a
  `tile_shuffle` of one dark seed in on 3 points and delete the trap) clears
  anchor/corroborated with >= 3 aligned
  points if `blk_min >= CLEAN_WORST_BLOCK`; never clean, so it adds a file
  and joins no clusters. For icons/logos: a 512 px icon vs its 128 px copy
  had 7 points at 0.997. Featureless files (no keypoints) match only
  byte-identical copies: `same_analysis` passes them over (two blank
  pictures of one size stretch to one thumbnail whatever their shade), so a
  blank PNG and its BMP are no pair.
- **Weak bridges are demoted** (`claims::demote_bridges`): any clean bridge
  whose far side is more than one file, including mirrored/inverted anchors,
  stays an anchor and is no longer clean (may add one file, counts in the
  cover test). Without the bridge test: 4 merges (0.53.0: none at the
  default, `docks` at `--min-frame-overlap` 0.6, 1,545 false pairs).
- **Clean-anchor clusters** (`claims::admit`): an anchor is clean when no
  detailed block correlates below 0.9 (`blk_min`, `Standing::Clean`, the
  rule's one fitted number; 0.85 until the fit and pyramid fixes raised near
  misses ~0.04 and Segovia1/2 merged at 0.85. Swept 0.875-0.95: no merge,
  F1 0.9823/0.9821/0.9816/0.9806; 0.9 is the plateau's lower middle). Weak anchors may join two clusters only when
  every file of the smaller has a weak anchor into the larger, and **not**
  when both close a cycle of clean anchors unless one file of the larger has
  a weak anchor to every file of the smaller. This is what stops near-miss
  families merging (IMGS2/3 at 87-92% precision without it).
- **Groups are a representative plus everything that matched it**
  (`group.rs`), not components (fragile, invent false pairs) and not cliques
  (thousands of groups). Groups overlap; members were not compared with each
  other. The representative is not the file to keep, but among files of
  equal reach it is the one the content rule keeps (`group::find`'s
  `better`), which `--suggest representative` keeps.
- **Vocabulary sized from the corpus** (`VocabParams::for_corpus`): occupancy
  held at ~3 descriptors/word by depth *and* narrowed branching (16^depth
  steps alone swung occupancy 16x and merged families at ordinary sizes).
  A word is a live leaf's centre slot. Centres are bytes (the one lossy step,
  measured neutral). Sample = a tenth of distinct descriptors, capped 1.28 M
  (`DESC_PER_SAMPLE`). Do not fix the vocabulary size. Leaves of one parent
  within `WORD_FLOOR` (143) are one word (0.75.0): without it two copies of a
  descriptor both in the sample got a leaf each, and a logo shared no word
  with its own copy.
- **Inverted file indexes every word**; no frequency cap.
- **"Identical" is one BLAKE3 content hash** (0.82.0, user's decision; it
  was a byte comparison after a sample hash, since the FNV hash then used
  could be made to collide); under a wildcard only possible pictures are
  read (`decode::may_be_image`).
- **Byte-identical copies and same-analysis twins** (`same.rs`: one
  `Same` answers "is this file another's" for the whole run) are matched
  through one original and get its pairs via `Same::expand`; the pairs
  between them come from `Same::pairs`. Otherwise they count each other as
  matches and suppress the second look. Twins are reported as `same_pixels`.
  A twin must also match in colour (`same_colour`: measured colour equal to
  the bit, or
  two JPEGs differing only in APPn/COM, else `decode::chroma_of` grids within
  `COLOURFUL`): grey is luma only, so a lossless-greyscale JPEG was a twin of
  its colour original, stood as one with it, and won as the smaller file.
- **Area rule** (`decode::area_factor`, 0.44.0): a picture wider than 4:3
  gets the pixels a 4:3 one gets (factor `sqrt(3/4 * long/short)`, thumbnail
  too, capped by `THUMB_MAX_SIDES`). 4:3 or squarer is untouched.
- **Enlargement** (`upsample_below`, `analysis::enlarge_below`): small pictures
  doubled up to `min(work, 512)`, never past the working size. 512 is the low
  edge of a plateau; no cap on the factor.
- **EXIF orientation**: a turned or mirrored picture is box-reduced as
  stored, straight into the plane as shown (`decode::reduce::Turned`: shown
  edges trimmed, each box summed in the upright picture's order), never
  turned at full size; its plane is its upright copy's to the bit
  (`a_turned_picture_reduces_to_its_upright_copys_plane`). Reduced then
  turned (`orient`, before), a turned PNG and its upright copy were two
  pictures at 512, both REVIEW.

### Keep, delete, review (`suggest.rs`)

User requirement: **a deletion never loses picture content.** Keeps may be
the original, a lossless copy, a re-encode, or a composite holding the whole
photo. REVIEW is for weak matches close to a copy; nothing acts on it.
Current figures: *Where things stand*. Scored by `suggest_score.py`
(`benchmark/suggest_score.py REPORT.json`, ~100 s a cached run), and on
two-file families by `pairs.py` (`run BINARY OUT --cache CACHE`, a cache
an IMGS-ALL run made, ~11 min; `score`, `compare`): **IMGS-ALL cannot see
the pair regime**, where every majority the rule chooses by is missing
(0.49.0 lost content in 2,241 of 27,355 pairs).

Rules (constants in `suggest.rs` with reasons): clean pair at 0.85 (its own
bar now, not `Standing::Clean`'s; 0.85/0.9/0.95 lose the same, delete ~the
same); a mirrored pair, like a negative, deletes nothing either way (a flip at
q95 was kept over its q92 original 14/19 times); weak pair
deletes at >= 0.95 (`WEAK_DELETES`), REVIEW at >= 0.9 (`REVIEW`); two crops
each missing a strip the other has are two frames, however thin (`CROPS`,
10%, made them one until the audit after 0.79.0); *holds* by frame corners, a
corner outside by at most `FIT_ERROR` / the pair's points, and a frame is the
larger only past `FIT_ERROR_MOST` / points, neither deleting the other between
(audit after 0.79.0; was 2% + 3 px either way), keeper may shrink <= 0.5 octave (`SHRINK`) unless the extra
pixels hold no detail (`Traits::detail`, `extra` above 0.2 an octave + 0.03,
`EXTRA_PER_OCTAVE`: enlargements read <= 0.3/octave at p99, real originals
>= 0.44 at p01; at 0.1 a 3x Lanczos upscale beat its original; originals
kept at 0.1/0.15/0.2/0.25/0.3/0.4: 276/296/297/297/298/298, none lost;
above 0.25 real originals start to read as enlargements); a whole-frame blurred check before
every deletion (`whole_worst`, 12 grey levels on a 64-sample grid,
`WHOLE_GRID`: at 32 thin lines blurred to haze and, once transforms fitted,
composites of one photograph deleted each other; losses at 32/48/64/96:
3/1/0/0), every holder asked in turn
(a cap of three tries, removed: it cost 108 safe deletions and no time);
composites are deleted only by their own copies; where no picture of a set
is copied at its own frame, a larger frame deletes what it holds only if its
8-px detail reads nearer "the rest is more picture" than "the rest is flat"
(`on_canvas`, split at sqrt of the share: at 1 px a third of slight crops
read as canvas; composites deleting their original in pairs 971 -> 40, losing
content 133 -> 2); a class of two different pictures of one frame (twins
count once) that neither `more` nor a size gap past `SHRINK` separates
deletes nothing and is REVIEW, both files, however they agree (not against a
library: a pinned file decides); no file is deleted by one
of its size softer by more than `SOFT_GAP` = 0.15 (the p95 softening of a
faithful lossy re-encode; the keeper is chosen by majority, and nine soft
enlargements outvoted their original; a file held back only by this is
KEEP, the test asked last so it holds back nothing else);
tie-break by grey closeness
(`TAU` = 3, the p99 grey-grid distance between two files holding the same
pixels), sharpness (`SOFT_TIE` = 0.05, the p99 softening of a q84-96 JPEG
re-encode), lossless, EXIF, size, colour (`COLOURFUL` = 1.5, above the 1.31
the noisiest grey copy reads; `decode::colour_of`, lazily, and kept in the
JPEG's cache record, `cache::Store::keep_colour`: it was decoded again on
every rescan, 4.5x a cached rescan's CPU on grey JPEGs). Each derivation
and its sweep is on the constant. Colour is the colour of a 4x4 grid's cell
means (`decode::measure::CellColour`): pixel by pixel, noise on a grey photo read as
colour and no bar separated it from a faint tint. A palette (indexed PNG,
<= 8-bit BMP, colour-mapped TGA) is not lossless (each format's `Facts`): a
256-colour copy used to be kept over its PNG original. Nor is a
JPEG-compressed TIFF; a WebP is lossless when its picture chunk is `VP8L`
wherever it lies (EXIF or ICC make the first chunk `VP8X`), a JPEG XL when
its first frame is Modular without XYB (a recompressed JPEG is VarDCT).
A lossless file is not deleted for a file of its frame and size whose
pixels are not its own (`own_pixels`): another lossless file makes an
undecided pair, a lossy one does not stand in for it. A file of several
pictures (`Traits::frames`, a JPEG's MPF pictures too) is deleted only for
its bytes. Of pixel-identical twins the keeper carries most besides its
pixels (`Traits::deep`, `profile`, `exif`), and none is deleted for a twin
lacking what it carries (`carries_more`). A file that decoded only in
part (`Picture::damaged`) is not chosen over an intact one and does not
delete one (0.75.0). Files refused by
either are KEEP and **delete nothing themselves** (`Refusal::Held`, unlike
`SOFT_GAP`'s `Refusal::Softer`): spreading from them let a kept posterize
edit delete its original.

**Adding a rule**: a new refusal in `Component::refusal` and its place in
`Refusal`'s order (what it makes of a file nothing deleted), never a new
list or a pass after settling. The action is read in `Facts::action` only.

A DELETE row carries `kept_copy` (JSON only), which the window shows: the
end of its chain of holders (`suggest::kept_at_end`), which stays. Deletion
spreads one holder at a time, and the nearest holder was itself a DELETE for
15% of deletions. A deletion whose kept copy is gone (`--last`, or the
window after the Trash or on restore) becomes REVIEW: carried out, it would
take the last copy (`Outcome::lapse`, shared by `--last` and the window). The window labels it "review: its kept copy is gone", not
a weak match. The two group rules judge only by representatives still there
(`--last` and the window): a group whose representative has gone suggests
nothing for its files.

`--suggest correlation | representative` (`suggest::by_group`): simple
group-only rules, by user request; `correlation` deletes at 0.9 at the
default bar (`CUT` = 2/3 of the way from the bar 0.7 to 1; the cut is
what was chosen), the user's choice. Neither answers to the content requirement.

### `--against LIBRARY` (`against.rs`)

Verdicts `repeat` / `better` / `review` / `new` per scanned file; `--list`
prints paths (one a line, or NUL-separated with `-0`; a path holding a line
break stops the run with an error unless `-0`, since `a<LF>b.jpg` listed as
two paths, and a list is what a script hands to `rm`). It is the content rule with the library pinned
(`suggest::suggest_pinned`). One index over both sides; the scan queries,
then library files it reached query in turn until none is new; candidates
ranked by `both_ways`. The second look also re-asks the best-connected member
of each clean-anchor component holding a scanned file. A scanned original
whose library copy is a same-frame re-encode is `repeat` (user's decision).
A file both sides reach is the side's whose root is nearer it, the scan's
on a tie (`walk::settle`): an import folder inside the library is scanned, a
library inside the scanned folder is the library (it was the scan's: an empty
library, every file `new`). `--exclude` never covers a whole library root.
Copy election: the first member of an exact group is the original, so cached
and cold runs agree. Kept for `--last` like any scan (see *A finished scan*);
`--last --list` lists it; the window does not open one.

### `--last` (`last.rs`)

Every finished scan's `Outcome` is kept in `last-scan/found.json` beside
the cache file, `Notes` in `scan.json` (shared with the window); `--last`
re-writes text/CSV as the scan wrote them but with every path absolute (as
kept), JSON without `pairs`. A scan kept before
0.55.0 (`result.json`, the report) is not read: run a scan. Not kept under
`--no-cache`, `--no-last`, or a walk that found no picture; the window keeps
by the same rule. Paths are kept absolute
(`Outcome::save`): read back from another folder, or by the window from
`$HOME`, relative ones named other files. The window's
Trash leaves a file whose length differs from the scan's, or whose ctime is
from the second the scan *started* on (`Notes::changed_after`; it was the
end, so an edit saved mid-scan passed); `--last` and the window
(`State::note_changed`, on showing, *Mark suggested deletions* and the
Trash) suggest deleting no such file and nothing resting on one. The Trash dialog also warns when a
marked DELETE's kept copy is marked (groups overlap, so "every image of a
group marked" missed an original + re-encode marked with a crop left).

### `--skip-mirrored`

Skips the second look: ~60% off a cached found-corpus rescan, but -6.6 to
-9.6 points of recall on the benchmark corpora and family fragments that join
near-miss siblings. A real trade, which is why it exists (and
`--no-propagate`, a dominated one, does not).

## Parameters, and the rule about them

**A number earns its place by being derived, not by scoring best.** Removing
a parameter is the cheap experiment; run it before adding one.

- **Sweep for the cliff, not the peak.** Acceptance thresholds are monotone
  in F1 up to a family merge. Quote the distance to the nearest merge (†: a
  seed pair with >= 300 false pairs), and always split false pairs into
  rearrangement traps vs. cross-family errors.
- **Score on two disjoint seed halves** (`analyse.py`) to check a shape
  generalises. Halves are blind to near-miss merges.
- **Re-test survivors after a removal, stacked**: `max_scale` is inert alone
  and load-bearing without the ratio test.
- **Names are load-bearing**: `--min-frame-overlap` (geometry only),
  `--min-pixel-correlation` (pixels), `--min-aligned-points` (inliers).
  Overlap and correlation each catch a failure the other cannot see; neither
  is a cost knob (tightening them makes propagation do *more* work).
- `--min-aligned-points` is the only bar propagation can overrule; below 3 is
  run as 3, with a note.
- Self-deriving the aligned-points bar from a null model was built and is
  worse (coincidence was never the binding constraint).

Current cliffs: *Where things stand*; every earlier table is in `HISTORY.md`.

Internal constants, measured (`out/v24-constants`, `out/v25-slack-budget`):
thumbnail 128 is the low edge of a plateau; **`GRID / BLOCK` (48/8 = 6 blocks
a side) is one step from a merge cliff** (4-5 blocks merge); flat-block bar 4
sits on a 1-6 plateau (8 merges); `CLUSTER_SLACK_CORRELATION` is a point on
a ramp, no cliff, ~0.5 recall (`tiled_watermark` and non-affine warps;
`CLUSTER_SLACK_POINTS` = 2 beside it was 0.03 and went in 0.54.0); decode budget `MemAvailable / 8` is
the knee; inlier tolerance 0.015 of the diagonal is on a plateau (0.03 is
unsafe); `index::shared`'s texture cut removed, its 60,000 cap kept as a work
bound. Lowe ratio test, `PROP_NCC`, `PROP_GAP`, per-block cut and coverage
fraction were removed as inert or redundant.

**How to measure without a run per value**: `--dump` writes every verdict
(gate loosened to 3 points / 0.2) with `blk`/`blk_min`, so any bar >= 3 and
the cluster rules can be replayed offline (`replay.py`,
`out/v14-fullsweep/replay_*.py`). To sweep an internal constant, read it
from an env var temporarily, hard-code the result, delete the scaffolding,
and re-measure the final build.

## Speed and memory

Shares (at 640, IMGS): decode ~35%, extraction ~45%, rest ~20%; at 384
decode ~48%. On the found corpus the second look is ~41% (nearly every file
has no duplicate) and enlargement 224 -> 448 is the biggest knob.

Rules that decide what is worth trying on this machine (Ryzen 7 3700U, 4
cores / 8 threads, power-capped):
- At 8 threads it charges for **µops, not waiting**: removing a stall shows
  at `-t 1` and vanishes at `-t 8`; removing work or bytes moved shows at
  both. `prof` stage tables are thread-time; only an end-to-end A/B is a
  pass's worth.
- `-t 4` costs ~35% fewer CPU-s for ~15% more wall.
- Every optimisation pass is **byte-identical** (same pairs, groups,
  fields) on the **whole** corpus, not a subset; check with `-o a.json` /
  `-o b.json`. The exceptions (byte centres, luma, streamed resample, area
  rule) were measured for accuracy instead.
- Peak memory depends on `MemAvailable` (decode budget); use `-t 1` for a
  deterministic reading. **The analysis is not in the heap** (`spill.rs`):
  each record's descriptors, keypoints and thumbnail pixels are written as
  made (cold, and on cache unpack) to an unlinked file beside the cache and
  read back through a read-only `MAP_SHARED` mapping; the same bytes, so
  output is byte-identical. Their pages are the kernel's to drop and re-read.
  So the real figure is **RssAnon + swap**, not PSS, which counts the mapped
  file pages. IMGS (5,641 files): heap held after analysis 426 -> 56 MB, at
  the end of verification 480 -> 118 MB; a cached run under a 380 MB cgroup
  cap with no swap finished (old build OOM-killed), at 300 MB it thrashed.
  What is left peaks inside decodes (a per-thread transient) and, cached, at
  assembly (the pair records, which scale with pairs). Writes go through
  `pwrite`, never the mapping (ENOSPC is an error, not a SIGBUS); any failure
  leaves that record in the heap. IMGS-ALL (`out/v34-spill`): heap peak
  2,330 -> ~1,040 MB, PSS unchanged at ~2,340, output identical, CPU within
  the base runs' own spread; then 776 MB once assembly stopped holding a
  second copy of the pairs, dump-only verdicts and a path string per pair
  end. The peak is still assembly (graph, suggestion, the pairs and the
  stable sort's buffer); matching ~390 MB, analysis transients ~300 MB.
  0.63.0: a pair is held by file numbers (`stated::Pair`), named only as
  the JSON is written: cached IMGS-ALL peak RssAnon+swap ~600 -> ~455 MB.

Portability: `cargo install` builds plain x86-64; `simd.rs` dispatches the
hand kernels and `dispatched!` gives hot functions a v3 copy. **Anything that
keeps code out of line** (closures to `LocalKey::with`, `Option::map`,
generic `extend`) silently runs the plain copy; check with `objdump`, counting
`ymm` per symbol. Test both copies agree:
`RUSTFLAGS="-C target-cpu=x86-64" cargo test --release --lib`.
Other targets are checked from here without their C (SQLite, zstd and
blake3 build C, and there is no cross C toolchain): `LIBSQLITE3_SYS_USE_PKG_CONFIG=1
ZSTD_SYS_USE_PKG_CONFIG=1 PKG_CONFIG_ALLOW_CROSS=1 cargo check --target
aarch64-unknown-linux-gnu --all-targets`, and `CARGO_FEATURE_PURE=1` as well
for `aarch64-apple-darwin`.

glibc floor is **2.28** for the CLI and **2.34** for the window; `release.yml` fails if
either moves. A binary asks for the glibc it was linked against, not what
Rust's std needs (2.17): built on the ubuntu-24.04 runner the CLI asked for
2.34 (`__libc_start_main`, the pthread functions), which shut out RHEL 8,
Debian 11 and Ubuntu 20.04 for nothing. Since 0.71.0 the CLI that ships is
built in `quay.io/pypa/manylinux_2_28_x86_64` (AlmaLinux 8); checked: it runs
on AlmaLinux 8, Debian 11 and Ubuntu 20.04 and writes reports identical to
the host build's, which those refuse (`GLIBC_2.34 not found`). It also needs
`libdl.so.2` and `libpthread.so.0` (separate from libc before 2.34). The
window needs GTK 4.10, so a newer system anyway. f32 and f64 libm functions
(`hypotf` and `hypot` are 2.35; 0.34-0.35 and 0.62-0.64 were never released
for it) can raise it, in either binary: run the workflow's "Verify the
binaries" step on a `RUSTFLAGS="-C target-cpu=x86-64"` build of both before
tagging, or at least check
`objdump -T target/plain/release/img-fp | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1`.

Decoder hazards:
- **Any decoder running work on the global rayon pool can deadlock the
  decode budget** (a thread steals an outer task and claims twice). JXL gets
  `JxlThreadPool::none()`, and a thread already holding a claim never queues.
- `image` is built with decoders only, no `rayon` (exr spawned a pool per
  file). libheif is dlopened; missing libheif makes HEIF files a problem.
- `max_alloc` = `MemAvailable` (min 512 MiB, but never above the
  control group's room, `cgroup_room`), applied to JXL headers and HEIF
  headers too. PNGs (8- and 16-bit), TIFFs (strips or rows of tiles; not
  planar, CMYK, palette or 1-bit) and JXL renders stream a row at a time.
  `into_decoder` + `from_decoder` skip the crate's own `limits.reserve`, so
  `decode::budget::fits_in_memory` asks first: without it an 18 KB BMP claiming
  96 x 788M px allocated 227 GB and aborted the whole run (3 of 3,000
  fuzzed files). Decoder panics are a per-file problem (`decode_guarded`,
  `probe`), never the end of the run or of a window thumbnail worker.

Measurement tools:
- `--features prof`: per-stage table, timed with `rdtsc` (this machine's
  clocksource is HPET, where clock reads cost ~1.5 µs). Stages nest.
- `cargo test --release -- --ignored --nocapture <name>`: `kernel_timings`,
  `reduce_timings`, `quantise_*` (incl. `quantise_threads`), `shared_timings`,
  `shared_pool`, `shared_overlap`, `match_timings`, `pixel_timings`,
  `extract_threads` (checksums `b8e5341a1d7b5cad` / `5c2fd85ed71708a8`; run it
  on any `sift.rs` change), `octave_threads`, `blur_threads`. Bench the
  regime (threads, cache-busting sizes), not just the kernel.
- `objdump -d` is the only instruction-level evidence (`perf` and `ptrace`
  are blocked here).

### Tried and rejected (numbers in `HISTORY.md` and `git show 5de81bc:CLAUDE.md`)

Do not retry without new evidence:
- Keeping the **larger** of lossy same-frame copies nothing else separates,
  and **lossless files with no lossy twin ranked after lossy ones** (0.72.0
  audit A and B, reverted; `out/v40-keep`). The cases they were for are
  real: of a q95, a q70 and a q60 JPEG of one picture the q60 is kept (the
  smaller, after `TAU` and the softness median), and a PNG saved from a q30
  JPEG is kept over the q95 and q85 JPEGs beside it (lossless first), on 7 of
  7 IMGS seeds each. But on IMGS-ALL (pairs identical, 0 content lost by the
  main measure in every variant) families keeping the original or a lossless
  copy went 295 -> 64-65 / 304 with both, 58 with B narrowed to lossless
  files sharing no lossy file's pixels and the smaller kept, 74 with B
  narrowed and the larger. Larger is not better: `noise`, `alpha_on_white`
  and `jpeg_high` edits are larger than their original and pass `TAU`, and
  win; without the softness filter, 209 families keep only an edit.
  IMGS-ALL's lossless copies (`webp_lossless`, `to_png`, `to_tiff`) differ
  from img-fp's own JPEG decode by a level and are no twin of their
  original, so ranked after the lossy files they were deleted, through a
  chain: the lossy keeper deletes the 2x upscale (no detail in its extra
  pixels), and the deleted upscale deletes the lossless copy, since
  `own_pixels` guards only against a file of its size. That chain is in
  0.71.0 too and fires nowhere in IMGS-ALL only because the lossless copy is
  the keeper there. IMGS-PROVENANCE: larger kept the original in 91 / 266
  (q97 instead), 0.71.0 in 126 (q85 instead), no content lost either way.
  The softness filter also works against an original among more compressed
  copies (they read sharper: dog2 q95 0.43, q60 0.31). Open.
- Answering a moved file with another name's record whose key (length,
  mtime, ctime to the nanosecond) is its own (0.69.0, reverted): the
  kernel's ctime is a clock tick's, shared by files written or touched
  together, so same-size files took each other's analysis and different
  pictures were reported "same pixels" with a DELETE. **Solved by content
  identity in 0.82.0** (the cache keys analyses by content hash; a key is
  compared only for its own path).
- `TAU` less one shift of every cell alike, up to half a grey level (for
  a 16-bit master against its 8-bit export): a posterize edit, whose tones
  move nearly uniformly, came into the keep candidates, was chosen, and
  deleted its original (IMGS3 `red_rocks_sign1`, 297 -> 296).
- Fine-detail deletion check from keypoints (correspondence at the fitted
  transform) or from a full-working-resolution residual: see A above.
- Second look: narrowing its descent, dropping any variant, cutting its
  candidate list, inlier or correspondence ceilings, dropping unseen words,
  relabelling words instead of descending (-21% of its pairs), an
  equivariant vocabulary (symmetrised pool merges families).
- Descent: partial-distance pruning, batching level by level, measuring
  several parents together, float centres in dimension-major blocks.
- Extractor: branchless extremum test, whole-row or strip blur passes,
  wider horizontal blur blocks, folding taps, row-by-row gradient fill, lazy
  gradients, descriptor histogram tricks (paired corners, register-held
  bins, two alternating histograms, vectorised trilinear corners).
- Memory/alloc: per-thread plane pool, `malloc_trim` at phase boundary,
  pinned mmap threshold, glibc tunables, holding the vocabulary sample as
  references.
- Other: DCT-scaled JPEG decode (~4% ceiling, second decoder),
  `posix_fadvise(WILLNEED)` read-ahead, Hamerly k-means bounds, parallel
  k-means update, mip pyramid instead of blur in `whole_worst`, one-file
  bridge test (`IMGFP_CUT`), cosine retrieval, best-edge-first propagation
  tree, k-truss / biconnected grouping, plain-tap pixel sampling.
- Corroboration from the second look's verdicts (0.57.0; turned verdicts
  above the corroborating bar and below an anchor's, admitted inside
  clusters like direct ones): +437 true pairs on IMGS-ALL, IMGS3, IMGS5
  (recall +0.02 to +0.03 points; IMGS5 +16 false, all inside its existing
  merges), keep rule unchanged; on the found corpus +12 pairs, every one a
  mirrored match of two specimens of a butterfly species by eye (a subject
  with two sides alike matches its kind mirrored). Kept out:
  `pipeline::cluster` reads `Variant::SAME` verdicts for corroboration.
- **Fixed seeding** (after 0.82.0, reverted 2026-11-08, user's decision;
  `out/v57-fixed/RESULTS.md` sections 1-7, code in `scaffolding.patch` and
  `fixed.vocab.zst` there): a verdict's first correspondences from a
  vocabulary trained once on 10,000 DISC21 references (65,536 leaves,
  canonical orientation, asker 3 paths x 2 orientations, candidate 1,
  mutual nearest within `GUIDE_D2`), the run's own vocabulary for retrieval
  only. One pair, one verdict: originals alone vs among the found photos
  identical evidence IMGS-ALL 15 / 15, HOLI-DEV 215 / 233 (base 10 / 14, 146
  / 219). Why rejected (the bar: no recall or precision lost anywhere, <= 5%
  CPU): with retrieval at one path (the CPU saving, IMGS-ALL2 +3%, found
  corpus -23%) DISC21 shard 0 at 0.5 lost 35 true copies (374 -> 339, µAP
  65.86 -> 60.23), every one never verified (alone, both builds judge them
  alike): retrieval paths, not seeding; at three paths recall is level (377)
  but false 380 -> 548 and CPU +24% (IMGS-ALL2 +16-23%). Precision on the
  full corpora falls wherever base hides siblings (IMGS-ALL bird1/2 851 ->
  6,849; new merges Mural1/2, and at loose bars Parish Church, game,
  Acueducto, Henares, png3 | baseball5): base pairs them alone, so a
  pair-intrinsic verdict cannot be level there. Better: HOLI-TEST every
  measure (joined 711 -> 691.7), pairs.py 26 -> 27 lost (two composites
  of siblings), IMGS-AGAINST level. Found on the way, open: the keep rule
  deletes IMGS `document2`'s original for its `motion_blur` copy once that
  copy is fitted right (0.995; base's wrong fit hid it), strict line 64 ->
  83; and DISC21 recall rests on the retrieval's three descent paths.
- **The correlation bar as a hard floor** (2026-11-10, user's decision:
  the correctness is not worth the recall; `out/v61-acceptance` section 10,
  `out/v62-floor`, patch `v61-acceptance/floor.patch`): `CLUSTER_SLACK_CORRELATION`
  removed, so no stated pair is below `--min-pixel-correlation` (HEAD states
  3.1% of IMGS-ALL2's pairs below 0.7). At 0.7 IMGS-ALL F1 0.9749 -> 0.9650,
  IMGS-ALL2 0.9417 -> 0.9272; swept 0.5-1.0 it is HEAD's curve moved about
  0.05 of bar (precision at equal recall -0.01 to +0.05 points, traps -8 to
  -17% at 0.7-0.85, cross-family and merges level); cost, DISC21 shard 0,
  IMGS-PROVENANCE and cliffs level. With the floor, the corroboration pass
  adds only 19 / 34 true pairs: the slack was all it carried.
- **256 as the base of `--detail`, replacing `--fast`** (2026-11-10/12,
  three builds, user's decision; `out/v64-ladder`, `out/v65-ladder`). The
  step above the base is then the default itself. Leaning on the 256 step's
  verdicts (windows `-k` / 3, no second look, one feature budget) it costs
  no more than HEAD and scores level or better on IMGS-ALL / IMGS-ALL2, but
  DISC21 shard 0 falls 66.89 -> 62.50 at 0.5 (33 true copies lost, never
  verified) and 256 -> 512 is not contained. As a matching of its own on
  top of the 256 scan (256 contained, one cache for every detail, DISC21
  66.48, recall +0.4 to +0.7) it costs +42-47% cached and +21% cold. Between
  the two, pairs the 256 step judged keep its weaker evidence (median 75
  aligned points against 118; µAP 63.2). `--fast` stays a scan of its own
  with records of its own.
- Claim graph (0.54.0 survey, each rule off alone, cached IMGS-ALL at 512
  and its cliffs, IMGS3 alone): one cover test for every join, clean
  anchors included (recall -1.0, 17 perfect transforms, Segovia merges at
  the default: 1,059 pairs); every anchor weak (Segovia + game merge, 14,176
  cross-family) or every anchor clean (24 merges); no enclosure (`docks`
  merges, 93); no bridge test (`docks` at overlap 0.6, 1,545); no cycle rule
  (inert on IMGS-ALL at every cliff, Segovia merges on IMGS3 alone at
  overlap 0.6, 7,528). Every one is load-bearing somewhere.

## Conventions

- **Canonical runner output**: `{"tool", "config", "runtime_seconds",
  "groups", "pairs"}`. **`pairs` wins over `groups`** everywhere (expanding
  a closure invents matches). img-fp's groups are representative + matches,
  each file entry the CSV row; `score.py` reads both bare paths and dicts.
- Reports give one overlap per pair, the larger direction (user's choice);
  `--dump` has both.
- **Exit codes** (vid-fp's): 0 clean, 1 fatal, 2 finished with problems,
  130 interrupt (instant; records are appended as made). A signal the run
  was started ignoring (`nohup`'s SIGHUP, a background job's SIGINT) stays
  ignored; the window's worker resets all three, since Cancel is SIGINT. **Every benchmark
  run exits 2** (`Foto 38622.png` is truncated); `bench.py`'s `OK_EXIT`
  accepts it.
- A run started as `cmd &` from a script ignores Ctrl-C (the shell starts
  it with SIGINT ignored, and img-fp keeps a signal it was started ignoring,
  for `nohup`); `kill -TERM` stops it. Not a defect.
- **Skips vs problems** (vid-fp's summary format). Skips: extension not
  taken by `-x` (extensionless files only under `-x '*'`), non-pictures under
  a wildcard (`decode::sniff::plausible` checks short signatures), hidden folders
  without `--hidden`, subfolders without `--recursive` (`Note::Subfolder`: a
  scan of a folder of folders said "No images found." and exit 0 with nothing
  to say why), Trash/thumbnail/NAS recycle and snapshot folders
  (`walk::SetAside`: `#recycle`, `@eaDir`, `#snapshot`, `@Recycle`,
  `@__thumb`, `@Recently-Snapshot` too; and `lost+found`, root's 0700 on
  every ext4 drive, which made a drive scan exit 2 and refuse a prune), symlinks
  without `--follow-symlinks`, second names for one (device, inode),
  macOS `._` metadata files (AppleDouble magic, any name; were a problem and
  a byte-identical "image" pair), featureless images (were a problem: a
  folder with one plain backdrop exited 2 every run).
  Problems: undecodable, unreadable path, unresolved `--exclude`,
  cut-off JPEG or PNG (`decode::jpeg::cut_off`, `decode::png::cut_off`:
  analysed, the PNG's missing rows mid-grey as the JPEG decoder fills them,
  marked `DAMAGED`, never cached), cache I/O failure. A failure's text is
  `img_fp::why` (the cause too; `{e}` of a context dropped "Permission
  denied"). Empty files are in no byte group (`exact_groups`), and files
  refused as no picture format are stated in no pair, whatever their name.
  A `--against` path that puts no image in the library is a problem
  (`EmptyLibrary`): every file was `new` and the run exited 0.
  **Adding a category**: a `problems::Note` variant, its arm in `Note::says`
  (skip or problem, log tag, summary label) and its place in `Note::ALL`;
  callers say `problems.note(Note::X, what)`. Not a counter of its own.
- `-o`, `--dump`, `--log-file` may not name a picture or a name with a
  picture's extension, each other, the
  cache or the kept scan; at most one output may be stdout, none under the
  window (`outputs::plan`, see *Where a run's output goes*). Reports and
  the dump are written whole or not at all (`outputs::write_whole`: beside
  the destination, through any symlink, with its mode, then renamed; an
  interrupt removes the half-written one): a failed write used to leave the
  last report cut off. `--cache` with `--no-cache` only beside
  `--clear-cache`.
- **Cache**: one per machine, `$XDG_CACHE_HOME/img-fp/analysis-IMGFPD01.db`
  (the index; named for its schema) with `X.lock`, `X.pack.N`, `X.matched`
  beside it; see *The analysis and the cache*. A record's file is gone only
  on "not found" (`cache::gone`, a folder listing at a time): an unreadable
  folder or a dropped share is not a deletion. A JPEG's colour measured
  later by the suggestion is kept on its analysis row (`colour`), as
  `cache::kept_colour(c)`, so cold and cached runs decide alike. Another
  version's file named with `--cache` is left untouched; in the default
  folder it is removed (`remove_other_formats`), and a file that is no cache
  is never written or deleted. **Cost measurements must pass `--no-cache`.**
  Sweeps should use their own `--cache`. `--prune-cache` keeps only what the
  scan used; refuses after a partial walk.
- Extensions: DDS not taken; `.hdr`, `.pam`, `.apng`, `.dib`, `.avifs`,
  `.heics`, `.jif`, `.jfi`, `.pjpeg`, `.pjp`, `.jpg_large`, `.jpg_orig`,
  `.png_large` are. ICO with an RGB PNG is read (`decode::other::ico_png`). HEIF is
  recognised by any `ftyp` brand.
- Venvs: `vendor/venv` general; `vendor/venv-imagededup` (torch) for SSCD
  and imagededup; `vendor/venv-imgdupes` for imgdupes.

## Benchmarking discipline

- One tool at a time; >= 60 s cooldown then wait for measured idle + 3 C
  (never a fixed temperature). Clocks swing 1.2-3.0 GHz; wall moves 25% with
  temperature. Compare only alternating pairs in one session (A B B A),
  never a single before/after or `cpu x MHz`.
- Clear tool caches (img-fp `--no-cache`, czkawka `-H`, imgdupes
  `--no-cache`, fresh dupeGuru db, SSCD without `--embeddings`) and evict the
  corpus with `posix_fadvise(DONTNEED)` (no sudo here).
- Peak memory: RssAnon summed over the process tree, plus swap (swappiness
  100). PSS also counts `spill.rs`'s mapped file pages, which are reclaimable.
- `bench.py --only <tool>` merges into `metrics.json`; give it a fresh
  `--out`.
- `derived/Desktop` (636 files) is the quick subset; `--cache` isolates the
  matcher.
- Accuracy reproduces to the pair; cost figures are soft (third significant
  figure means nothing).
- **A matcher or analysis change is judged by `benchmark/validate.py run
  BASE NEW OUT`** before anything is said of it: DISC21 shard 0 first (a
  build level on IMGS-ALL and IMGS-ALL2 lost 33 of 382 true copies there,
  `out/v65-ladder`). About seven hours whole; `--stages` and `--corpora`
  pick (`--stages proxy`: a five-minute first look at DISC21, not a
  verdict), `--base-results` takes the base's accuracy from an earlier run.

## Working style

- The user reads results critically and pushes back on measurement
  artifacts; check numbers against a second method before reporting.
- Do not start long tool runs without being asked.
- Report failures as results (they are in `BASELINE.md`).
