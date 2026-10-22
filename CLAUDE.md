# img-fp

An image deduplicator, and the benchmark that judges it. Sibling to **vid-fp**
at `/home/daniel/Documents/Vscode_repositories/deduplicator/`, whose
`benchmark/README.md` is where this methodology comes from: read it before
changing how anything is scored.

The full measurement history (every table, every pass, every rejected idea
with its numbers) was in this file until 0.44.0. It is in git:
`git show 5de81bc:CLAUDE.md`. Look there before re-trying anything listed
under *Tried and rejected* below, or before quoting a figure not given here.

## Status (0.64.0)

- Baseline corpus **IMGS-ALL** (IMGS + IMGS2 + IMGS3 + IMGS4: 27,659 files,
  304 seeds, 90 transforms). At the default `--work-size 512`: **F1 0.9821**,
  99.57% precision, 96.90% recall, 47 perfect transforms; 7 cross-family
  pairs, no family merge (`out/v33-fit-pyramid/RESULTS.md`; 0.45.0 was
  0.9805 / 95 cross-family). At 640 (0.51.0, `out/v36-cliffs`): F1
  0.9846, recall 97.40%, 8 cross-family, no merge. SSCD, the best competitor:
  F1 0.748, and every other tool has zero perfect transforms
  (`benchmark/BASELINE.md`, field in `out/v17-all4`).
- 0.47.0 (audit fixes, `out/v35-audit`): F1 **0.9823**, recall 96.93%,
  same 7 cross-family pairs and trap FPs, 48 perfect transforms; keep rule
  17,299 DELETE, none loses content, 276 / 304.
- 0.48.0 (`out/` not written; measured in a session scratch dir): keep rule
  **297 / 304** originals (was 276) from `EXTRA_PER_OCTAVE` 0.1 -> 0.2,
  17,300 DELETE, none loses content, 304 / 304 faithful; pairs and groups
  byte-identical. Also: signals started ignored stay ignored (`nohup`),
  "changed since the scan" counts from its start, the window demotes
  suggestions resting on a changed kept copy as `--last` does, and a JPEG's
  measured colour is kept in its cache record.
- 0.48.1: a same-size copy softer by more than
  `SOFT_GAP` (0.15) may not delete a file (`suggest::Ctx::softer`); a file
  held back only by that is KEEP. Before, an original among several soft
  same-size copies was outvoted and deleted (177 / 178 synthetic families,
  now 6). IMGS-ALL: 17,259 DELETE, none lost, 297 / 304, REVIEW unchanged.
- 0.49.0 (audit fixes, no change to pairs or suggestions): a header
  claiming more pixels than memory is a per-file problem, not an abort of
  the run (`fits_in_memory`), and decoder panics are caught; reports and the
  dump are written whole (`report::write_whole`); `lost+found` set aside; the
  window names a kept copy's folder and zooms its images (`[-][+]`).
- 0.50.0: the keep rule on two-file families (`benchmark/pairs.py`:
  every seed's original + one variant, each transform alone). 0.49.0 lost
  content in 2,241 of 27,355 pairs (an original deleted for its brightened,
  recoloured, sharpened, watermarked copy, or for a composite holding it);
  now 24, all where `extra` takes a reduction past `SHRINK` for an
  enlargement, as before. Two rules: a frame of two different pictures
  that nothing measured separates is undecided, both REVIEW; and a lone
  larger frame whose extra area reads as canvas (`on_canvas`) does not delete
  what it holds. REVIEW in pairs 8,114 -> 29,368 of 54,710 files (every lone
  re-encode and format conversion). IMGS-ALL: 16,873 DELETE, none lost,
  297 / 304, 304 / 304, REVIEW 3,004. Still open: which of several lossy
  copies is the original (IMGS-PROVENANCE keeps it in 34 / 266, a faithful
  copy in all; size picks a q97 re-save instead, so it is no answer), and
  colour edits in families of three or more.
- 0.51.0 (audit fixes A-F; cache format IMGFPC14):
  - **A**: a lossless file is not deleted for a file of its frame and size
    whose pixels differ (`suggest::Ctx::own_pixels`): two lossless files
    are an undecided pair (REVIEW), a lossy one does not stand in for a
    lossless one. Why: invoice screenshots differing in the total ($1,250 /
    $9,750) or a line of text agreed at 1.00 and one was deleted (with a
    re-encode as third file, the other invoice; with a changed line, the
    original). Nothing the run keeps can see it: their keypoints matched at
    descriptor distance 0, and a residual at full working resolution read
    5.4 for the other invoice against 9-10.6 for a plain downscale. So lossy
    documents (two JPEG invoices) are **still open**.
  - **B**: `Traits::frames` (GIF/APNG/WebP animation, TIFF pages, HEIF
    sequence brands, JXL animation): deleted only for its own bytes, never a
    same-pixels twin (`Same::settle` skips it). Before: two GIFs sharing a
    first frame were "same pixels" and one DELETE; a 3-page TIFF DELETE for
    a PNG of page 1.
  - **C**: TIFF-container camera raw (`DNGVersion`, `CR` at byte 8, a
    reduced-resolution first directory, or `Make` + `SubIFDs`) is refused
    as a skip (`decode::Refused::CameraRaw`), still in the exact pass. Before: read
    as its 256-px preview and DELETE beside its JPEG under `-x '*'`.
  - **D**: a 16-bit master next to two JPEGs of its 8-bit export was
    outside `TAU` (7.96 summed, 0.5/cell: one requantisation) and was
    deleted for the q85. Now held by A (KEEP; the q85 stays KEEP too).
  - **E**: `decode::budget::cgroup_room` (v2 and v1, ancestors included): budget
    and `max_alloc` no longer use the host's `MemAvailable` alone. Before:
    under `MemoryMax=300M` one 1.5 MB TIFF OOM-killed the run (137).
  - **F**: TIFF read a strip / tile row at a time (`tiff::chunks`)
    and 16-bit PNG rows (`png::rows`), bit-identical to `other::whole`
    (pixels and traits; 8/16-bit, alpha, grey, tiled, LZW, deflate+
    predictor, JPEG, oriented, truncated). 12000x12000 TIFF peak RSS:
    8-bit 850 -> 20 MB, 16-bit 1.69 GB -> 19 MB.
  - Measured: IMGS-ALL pairs and groups byte-identical (1,128,340 pairs),
    0 lost, 297 / 304, 304 / 304, DELETE 16,873 -> 16,767, REVIEW 3,004 ->
    3,052 (lossless halftone / channel_swap / posterize edits now kept);
    pairs.py and IMGS-PROVENANCE unchanged to the row.
  - Cost, alternating pairs, cold unless said: IMGS-ALL -t 8 (bench.py,
    ABBAAB) base 2,735 / 2,690 / 2,647 CPU-s, new 2,804 / 2,838 / 2,794,
    every new run at a lower mean clock (1,789-1,833 vs 1,886-1,943 MHz);
    IMGS-ALL -t 4 (ABBA) the other way, new 1,632 / 1,675 vs 1,724 /
    1,746, the new runs at the higher clock. Sign follows the clock, so
    neither is the code: IMGS -t 4 -v (ABBA x2) base 262.9 / new 261.5 CPU-s
    mean, decode 93.25 / 93.5 s, features 120 / 120 s; IMGS-ALL cached
    (ABBA x2) 438.0 / 438.75; per-symbol ymm counts identical; the 304
    IMGS-ALL TIFFs decode 4% faster streamed. Peak PSS within run spread.
- 0.51.1 (audit fixes, no change to pairs or suggestions): `-o`/`--dump`
  replace only a regular file (`report::Dest`): a descriptor (`/dev/stdout`,
  `>(...)`, `>>`) is written as opened, a FIFO or device in place, with no
  probe open (it read as EOF to a FIFO's reader). "Changed since the scan" is
  to the nanosecond (`Notes::started_ns`; whole-second ctimes still read
  coarse): files written in the scan's first second were never deleted and
  the Trash refused them. `cache::gone` lists each folder once instead of a
  `stat` per carried record. The window says "review", not "weak match":
  most REVIEWs are undecided same-picture pairs at 1.00. Not measured on the
  corpora (no rule changed).
- 0.52.0 (the keep rule rebuilt, not retuned): `suggest::Component::refusal`
  answers "may `k` delete `d`" with a reason and decides nothing; what a
  reason makes of a file is the order of `Refusal` (`No < Softer < Held <
  Undecided`, by caution: the greatest of a file's refusals decides); every
  action is read once (`Facts::action`). Before: refusals were pushed into
  three side lists by a `bool` closure, read back with an unwritten order
  (Softer above the rest), so a held or undecided file a softer copy also
  refused was kept and deleted others. Proved first with that order kept: a
  random differential test (40,000 families of up to six files, every
  reason reached thousands of times) against the old code, identical; then
  the order corrected: 112 of 40,000 families move (28 fewer deletions, one
  more, a file deleted for its own bytes). IMGS-ALL, IMGS-PROVENANCE and all
  90 `pairs.py` reports byte-identical to 0.51.1 both ways (no corpus has
  the case). `suggest::revoke` is the one place a DELETE whose kept copy or
  own file has gone or changed becomes REVIEW (`--last`, the window's
  `note_changed`, `drop_gone`, `regroup`; was six loops).
- 0.52.1 (refactor, no change to output): "is this file another's" is
  one `same::Same` (bytes, analysis, colour; was three parallel vectors read
  by hand in eight places); an edge is one `Edge { i, j, v }` (the transform
  and inversion were held twice; 112 -> 76 bytes); a stated pair's report
  row, `--against` pair and keep-rule link are read from `Stated`'s methods
  once. IMGS-ALL (cold and cached), dump, IMGS-AGAINST, IMGS-PROVENANCE,
  all 90 `pairs.py` reports and the found corpus byte-identical to 0.52.0;
  cached IMGS-ALL peak RssAnon+swap (ABBA) 729 / 685 MB base, 635 / 631 new.
- 0.52.2 (refactor): `run` (1,572 lines, one scope, `against` named 46
  times, the verify step written twice, three hand-built exits) is
  `pipeline.rs`: stage functions with typed hand-offs, a `Scope`, one
  `Pass::verify`, a `Dump`, one `finish`; see *The pipeline*. IMGS-ALL (cold,
  cached, dump), IMGS-AGAINST (report, dump, `--list`), IMGS-PROVENANCE, the
  found corpus (report, dump), all 90 `pairs.py` reports, `--last`,
  `--skip-mirrored`, `--prune-cache` and the window's worker copy identical
  to 0.52.1 but for `runtime_seconds`. An empty walk now goes the common way:
  its `runtime_seconds` is the real one (was 0.0) and its log says "groups: 0"
  and "suggested: 0". Cached IMGS-ALL (ABBA): peak RssAnon+swap 633 / 631 MB
  base, 615 / 612 new; CPU 457.8 / 435.3 base, 440.2 / 437.8 new.
- 0.53.0 (decoding rebuilt): `decode.rs` (4,327 lines; four parallel
  `match`es on the format, for analysis, preview, colour and probe, each
  repeating the ICO, TGA, JXL and HEIF quirks; per-format facts in three
  more; `Traits` measured through a thread-local reset by hand) is
  `decode/`: a file is identified once and opened once by its format's
  module, which gives `Facts` and reads its pixels into a `Sink`; see
  *Decoding*. Fingerprints of every file of IMGS-ALL, the found corpus and
  a 118-file format zoo (`decode::tests::fingerprints`: working plane at 512
  and 160, size, traits, damage, colour, chroma, `preview(256)`, four
  `preview_fit`s, probe) identical but where meant: TGA, HDR and farbfeld
  are lossless (TGA and HDR have no signature, and the lossless list was
  only asked of sniffed formats, so their entries never applied); a camera
  raw file is refused by the preview too; a TIFF directory chain is read as
  far as it reads (a broken later pointer threw away a first directory's
  raw marker); placed previews of TIFF, HEIF, 16-bit PNG and EXIF-turned
  PNG are box-reduced while read, as 8-bit PNGs already were (±1 px, the
  row path's rounding; a turned picture's factor is taken at its shown
  size). Previews claim the decode budget. JPEG, WebP and JXL: no
  difference at all. IMGS-ALL (cold, cached, dump), IMGS-AGAINST (report,
  dump, `--list`), IMGS-PROVENANCE, the found corpus, the Desktop edge runs
  (`--last`, `--skip-mirrored`, `--prune-cache`, stdout, empty walk) and all
  90 `pairs.py` reports identical to 0.52.2 but for `runtime_seconds`.
  12000x12000 TIFF (8, 16-bit) and 16-bit PNG still peak at ~19.5 MB; under
  `MemoryMax=300M` both builds finish alike. Cold IMGS -t 4 -v (ABBA x2; one
  base run at 2,109 MHz against ~2,400 dropped): CPU 302.6 / 302.4 / 307.6
  base, 306.2 / 304.2 / 305.4 / 304.3 new; decode 122 / 106 / 111 s base,
  105-107 new; peak RssAnon+swap 410-437 MB base, 409-453 new. Code lines
  (no comments or blanks) 2,246 -> 2,256. Cache format IMGFPC15, for the
  lossless change (a record of 14 would keep calling those files lossy).
- 0.54.0 (the claim graph rebuilt, `claims.rs`; see *The claim graph*): a
  verdict's acceptance is one `verify::Standing` (`Rejected < Corroborating
  < Anchor < Clean`, each clearing every bar below it), read once by
  `Policy::standing`; a composed one is `Policy::composed`; the gate is
  `Policy::gate` (were four `Rules` read by seven `accepted(&policy.X)`
  calls). `Edge`, the bridge test (now a demotion of the clean flag),
  admission and propagation moved to `claims.rs`, over one `Clusters` that
  admission builds and propagation and corroboration read (were five
  union-find / BFS passes over the same partition). The claims are one
  `Vec<Claim>` with `How` it was made (were a three-list tuple, "`n_match ==
  0` is propagated", a `bool` in `Stated::Match` and string compares).
  Proved identical first: differential tests against the old code (2 M
  random verdicts against the four tiers; 3,000 random graphs through bridge
  test, admission, propagation and corroboration), then cached IMGS-ALL
  report and `--dump`, the `ref.sh` battery (cold, cached and `--against`
  IMGS-ALL with dumps, IMGS-PROVENANCE, the found corpus, Desktop edge runs:
  84 outputs) and all 90 `pairs.py` reports, identical to 0.53.0. Then
  each rule was switched off alone (IMGS-ALL at the default and its cliffs,
  IMGS3 alone): all but one load-bearing somewhere (*Tried and rejected*).
  The one: **`CLUSTER_SLACK_POINTS` (2) is gone**; a corroborated match needs
  the anchor's aligned points or a whole frame. IMGS-ALL: F1 0.9823 ->
  0.9822, recall 96.93 -> 96.90%, 425 pairs fewer (every one corroborated,
  same-seed, 8-9 points: TP -393, traps -22), same 7 cross-family pairs,
  perfect transforms 48 -> 47 (Henares1 `mirror_crop`, 8 points at 0.547),
  halves 0.9802 / 0.9830 -> 0.9801 / 0.9828. Cliffs: IMGS-ALL identical but
  overlap 0.6 (18 -> 16 cross-family) and overlap 0.5 (`docks`, 6,645 ->
  6,644); IMGS3 identical or fewer (points 8/7/5: 8/9/11 -> 7/7/10, overlap
  0.6: 8 -> 7). Keep rule: 16,767 DELETE, none lost, 297 / 304, 304 / 304,
  two edits REVIEW -> KEEP; `pairs.py` unchanged. Verified pairs read by the
  pixels 850,290 -> 829,142 (gate 8 -> 10 points). Cached IMGS-ALL (ABBA):
  CPU 466.6 / 498.9 base, 467.4 / 507.6 new (the runs' own spread); peak
  RssAnon+swap 617 / 623 MB base, 614 / 612 new (no copy of the anchors
  held through propagation). Code lines flat (4,784 -> 4,778 in the files
  touched).
- 0.54.1 (window only): options set away from their default are kept
  between runs (`gui.json` `options`, `settings::Kept`); untouched ones
  are not written, so they follow a moved default. Checked in the window
  under Xvfb: changed, scanned, reopened, restored defaults.
- 0.55.0 (a finished scan rebuilt, `outcome.rs`; see *A finished scan*):
  the report, `--last` and the window read one `Outcome` (groups by number,
  each member's `Evidence`, the content rule's suggestions); the group rules
  and gone / changed files are its methods (`actions`, `lapse`, `look`,
  `shown`). Before: the JSON report was also the storage, parsed back by
  `last::Kept` and `gui::scan::Member`, each applying the group rules and
  lapses its own way (`last::rebuild`; the window's `prepare`, `regroup`,
  `note_changed`, `drop_gone`), the 1.0-for-one-file agreement written
  three times, `keep_scan` swapping suggestions into the report and back,
  `OutPair` a relation as four `bool`s. Drift found: `--last` fell back to
  correlation 0.6 (default 0.7); the window forgot a reference once its
  group was down to it and the Trash was used again (`--last` kept it a
  reference; now both do). Kept scans of 0.54 and before are not read
  (`last-scan/found.json`, was `result.json`): run a scan. Proved: a random
  differential test (20,000 scans, three rules, random gone / changed sets
  and window Trash / change sequences) against 0.54.1's `rebuild` and window
  code, identical but the case above; `ref.sh` battery (104 outputs) and a
  `--last` battery on a Desktop copy before and after deleting and touching
  files (every format x scan rule x `--suggest`), identical to 0.54.1. The
  window under Xvfb: restored a CLI scan with a file gone, marked 309 (=
  `--last`), trashed, switched rule (68 = `--last --suggest correlation`),
  reopened with marks, scanned (7 = `--last`). Kept file on IMGS-ALL 19.0
  -> 11.1 MB. Cached IMGS-ALL (ABBA): CPU 467.0 / 481.3 base, 454.9 / 476.8
  new; peak RssAnon+swap 610 / 609 MB base, 605 / 612 new. Code lines (no
  comments or blanks, tests included) in the files touched 9,741 -> 9,398.
- 0.56.0 (a file's analysis and the cache rebuilt; see *The analysis and
  the cache*): before, `pipeline::analyse_all` did the cache's bookkeeping:
  it held the cache's map, took each walked file's record out and compared
  keys by hand (`in_cache`, `same_key`), moved a byte copy's record to its
  original, worked out `superseded` and the bar's `rewrite`, and carried
  `names` and `span_of` through `Analysed` into the suggestion for
  `keep_colour`; what survives was assembled in `save_cache` from four
  lists (this run's spans, `carry_over` over what was left in the map,
  `other_settings` filtered by `gone`, the store's own `foreign`). `Item`
  was `ok` / `err` / `damaged` (a copy's placeholder: not ok, no error), and
  skips were told from problems by comparing error strings. Now
  `cache::open` takes the run's files and the store answers by number
  (`take`, `append`, `keep_colour`); `Store::finish` is the one keep rule;
  `Item::read` is an `analysis::Read`; `decode::Refused` is a typed error.
  Drift fixed: a record made at another `--work-size` for a file on an
  unplugged drive was dropped (it was asked `gone` alone; only this run's
  settings were asked `unmounted`): one rule now, `cache::carried`.
  Proved: 200 random histories (4,836 scans at `-t 1` over subsets of a
  scratch tree, with deletions, chmods, byte copies, rewrites, added files,
  moved folders, two working sizes, `--prune-cache`, all three rules), base
  and new on the same tree: cache file bytes, reports, logs, stdout,
  stderr and exit codes identical (one seed's logs differed in timestamp
  width only); the `ref.sh` battery identical, and its three caches
  identical record for record (compared as multisets: at `-t 8` the append
  order varies run to run); all 90 `pairs.py` reports and the cache they
  leave identical; the `--last` battery identical. Interrupted runs keep
  their records and the next run reads them; two runs started at once on a
  new cache file kept both runs' records in 12 full trials and 80 small ones
  per build (one early base trial lost the second run's records, not
  reproduced; the open/lock code is unchanged). Window under Xvfb: its
  worker fills a cache the CLI then reads whole (635 / 635), and a scan from
  the window shows the CLI's 64 groups. Cached IMGS-ALL (ABBA): CPU 530.4 / 517.4 base (the first at 2,010 MHz, idle 83 C), 494.0 / 517.2 new; peak RssAnon+swap 626 / 604 MB base, 603 / 601 new; reports identical. Code lines (no comments or
  blanks, tests included) in the files touched 5,997 -> 6,106 (cache.rs
  +138, of which tests +47; pipeline.rs -84; lib.rs -61; analysis.rs +104
  new): the cache's API is wider (`Asked`, `Unasked`, `Kept`, `Compacted`)
  and the pipeline narrower.
- 0.57.0 (retrieval rebuilt, `retrieve.rs`; see *Retrieval*): before, the
  question "which files may this one be" was written twice: the direct pass
  (with the library rounds woven in by `if scope.against` / `first`) and the
  second look (its own query, rank and verify loop, three `Variant` literals
  indexed as `v as u8`), with `both_ways`, `rank_best`, `merge_variants`,
  `quantise` and `variant_features` in `lib.rs`, and the index's memory
  (postings dropped and rebuilt, the vocabulary an `Option` of `unwrap`s,
  the `ahead` lists) handled inline in the stages. Now `Index::candidates`
  answers any set of variants, `Index` owns the postings' and vocabulary's
  lifetimes (`let_go`, `turn`, `turned`), `sift::Variant` is `SAME` /
  `TURNED` with `Features::seen_as`, and who asks is `Scope::reached` /
  `re_askers`. The second look's verdicts join the direct list
  (`Pass::verify` keeps whatever stands; the dump's kind is read off
  `Verdict::variant`), and corroboration reads `Variant::SAME` verdicts by
  rule, with its measured reason (*Tried and rejected*). Proved: the
  `ref.sh` battery (cold, cached, `--against` IMGS-ALL with dumps,
  IMGS-PROVENANCE, the found corpus, Desktop edge runs) and all 90
  `pairs.py` reports identical to 0.56.0, both after the refactor and with
  the one list; per-symbol `ymm` counts identical. Cached IMGS-ALL (ABBA):
  CPU 560.8 (at 1,981 MHz) / 485.6 base, 497.3 / 503.7 new; peak
  RssAnon+swap 605 / 601 MB base, 600 / 604 new. Cached found corpus
  (ABBA): CPU 343.3 (2,006 MHz) / 293.7 base, 302.0 / 297.0 new; peak 182
  MB both. Code lines (no comments or blanks, tests included) in the files
  touched 7,049 -> 7,110 (`retrieve.rs` 198, of which tests 45).
- 0.58.0 (`--against` is an `Outcome`; see *A finished scan*): before, a
  run against a library ended in a second finish (`write_answers` beside
  `write_groups`) with its own copy of everything after the pairs:
  `against::Pair` / `Facts` / `Header` / `Row`, three writers, the stdout
  and broken-pipe handling, the evidence text, a holder-chain walk of its
  own and a sort by verdict string, and `Found` carried `against_pairs`
  beside `graph`. Drift: the text dropped "corroborated" (`report::evidence`
  said it; the copy wrote "9 points"), and a looping chain named a file kept
  where `kept_at_end` names none. And it could not be kept for `--last`.
  Now `Outcome::against` holds each scanned file's holder and library pairs,
  `against::answers` reads the verdicts as things stand, `report.rs` writes
  both layouts through one `write_layout`, `pipeline::finish` is one path
  (`outcome` + `numbered_groups`), and `--last` / `--last --list` rewrite a
  comparison (the window leaves one alone). Proved: a random differential
  test (old `answer` + `rows` + writers kept in a test module, 20,000
  libraries of up to 13 files with chains, mirrored / inverted / clean
  pairs, non-UTF-8 names) identical in all three layouts; with corroborated
  pairs too, 7,345 of 20,000 differ, all by the restored clause in the text
  alone. `ref.sh` battery (now with `--against` text, CSV, stdout and
  `--list -o`, Desktop and IMGS-AGAINST) identical to 0.57.0 (no IMGS-AGAINST
  row rests on a corroborated pair); all 90 `pairs.py` reports identical;
  `--last` of each `--against` report identical to the scan's (JSON less
  `pairs`), `--list` too. On a symlink farm of IMGS-AGAINST with a library
  file and a scanned holder deleted and a scanned file replaced: 11 repeats
  answered again (9 better by another library copy, 2 new), the changed file
  not a repeat, the gone ones left out. Cached IMGS-AGAINST (ABBA): CPU
  539.6 (2,140 MHz) / 508.3 base, 517.2 / 513.2 new; peak RssAnon+swap
  602 / 602 MB base, 604 / 602 new (the new one also keeps an 11.1 MB scan).
  Code lines (no comments or blanks, tests included) in the files touched
  6,452 -> 6,541: `against.rs` 375 -> 184, `report.rs` 741 -> 949 (the
  second layout), `last.rs` +30 (`--last` of a comparison), tests +60.
- 0.59.0 (the window's options are a table, `gui/options.rs`; see *The
  window*): before, each option the window shows was written out by hand in
  about thirteen places (a field of `Settings`, of `Kept`, two `From`
  lines, `Default`, `argv`; a field of `Setup`, its widget, struct init,
  `show`, `read`, a tooltip; `lib::Defaults`), so a place missed compiled
  and passed (the frozen defaults before 0.19, the clear-cache that deleted
  the default file). Now an option is one `Opt` (key, clap id, kind, words),
  its default read from clap itself (`img_fp::flag`; `Defaults` gone),
  `Settings` a value per option, `Setup` a `Control` per option, and a test
  (`every_flag_is_decided`) fails until every flag of `Args` is shown,
  passed by hand (`BY_HAND`) or not the window's (`NOT_SHOWN`). `--last`
  takes an allowlist (`WITH_LAST`, checked in `lib::parse`) instead of
  `conflicts_with_all` over eighteen scan options, so a new scan option is
  refused by it unnamed; same refusals, exit 2, clap's usage line worded a
  little differently. A settings value of the wrong type now falls back
  alone (it made the whole file the defaults, folders and all). Proved: a
  differential test (old `settings.rs` in a temporary test module, 20,000
  random settings stepped like spin buttons): img-fp reads the same options
  off both command lines (`img_fp::given`; only their order differs), the
  files hold the same JSON and each build reads the other's, and a flipped
  option always changes the command line; under Xvfb, base vs new with a
  kept `gui.json`: both tabs, the results page and "Restore defaults"
  pixel-identical, same worker argv (reordered), same saved `gui.json`,
  same report; cached Desktop scan and its `--last` txt / csv / json /
  `--suggest correlation` identical but `runtime_seconds`. Code lines (no
  comments or blanks) outside tests in the files touched 2,329 -> 2,323;
  tests +91.
- 0.60.0 (the window's keys and views, `gui/commands.rs`; see *The
  window*): before, a key was written in up to five places (an arm of one of
  six key controllers: the results page's image, Alt and Ctrl ones, the
  large view's, the scan page's, F1; a line of the hand-written F1 list; the
  results menu's two tables, one a view; its `gio` action; the mnemonic
  test's lists), and the page's two views were `tree_view: bool`, tested in
  ten functions, with three versions of "show what the view has selected"
  (`rebuild`, `toggle_view`, `reshow`). Now each part of the window that
  takes keys is a `commands::Section` of `Row`s (an F1 line and the chords
  it describes); a controller asks `Section::find` and does the command
  (`Results::key` decides only which keys the page takes from the focused
  widget, `Results::run` what each does, for keys, the menu and its
  actions alike); the F1 list, the menu, its actions and the quit
  accelerators are read off the tables. The views are `results::View`, each
  list one owner of its widgets (`Strip`, `Folders`), and `show_view`,
  `step`, `scope`, `side_list` the one `match` each. Tests: no key bound
  twice on a page, every command bound, every menu item a mnemonic and only
  those, a menu letter the same in both views, the F1 list as it was. Found
  by the table and dropped (user's decision): the large view also took Page
  Up / Page Down, and a folder list Backspace, which the F1 list never said.
  Proved under Xvfb with metacity, base vs new, three scripted runs (a
  kept `derived/Desktop` scan; a hard-link farm of it in nested folders,
  for the tree): every key of every section pressed, in both views, from
  the images, the lists and buttons, 60 screenshots pixel-identical and the
  marks files identical, but for the progress bar of a scan cancelled
  mid-run (before the two keys were dropped). Non-test code in the files touched 3,554 -> 3,783 (`commands.rs`
  321; `main.rs` -69; `results.rs` -20, `Results::new` 685 -> 529 lines);
  tests 111 -> 264.
- 0.61.0 (refactor, no change to output): what a run skips or could not
  do is one `problems::Note` per category (its weight, log tag and summary
  label in `Note::says`, its order in `Note::ALL`), counted by
  `Problems::note`. Before, each of the 16 categories was a `Tally` field,
  a line in `new`, a method, and an entry in `skips()` / `problems()` with a
  hand-counted array length, and a missed place compiled. The byte pass
  (`exact_groups`, its sample and whole-file hashes, the byte compare)
  moved from `lib.rs` to `same.rs`, beside the other two sameness answers.
  A transform's determinant, scale, mirroring and inverse are written once
  in `verify` (`det`, `linear_scale`, `mirrored`, `invert_affine`); the keep
  rule had its own `invert` and `mirrored` (its inverse divided by the
  determinant, `verify`'s multiplies by its reciprocal, so the last bit
  could differ). Proved: a tree that hits every category, ten configurations
  (wildcard, extension list, followed links, excludes, an unwritable cache
  and a cache path that is a folder, `--log-file` to a file and to stdout,
  JSON), stdout, stderr, log, report and exit code identical to 0.60.0;
  cached IMGS-ALL report (1,110,436 pairs, every suggestion) and console
  identical but `runtime_seconds`; all 90 `pairs.py` reports and lists
  identical; per-symbol `ymm` counts identical. Code and test lines (no
  comments or blanks) in the files touched 8,242 -> 8,212.
- 0.62.0 (compare view): the window's large view shows an image beside
  its group's reference (`C`, *Compare with reference*), the part each shows
  of the other outlined and the rest dimmed (`gui::results::Outline`). Drawn
  from `outcome::Member::at`, a `Place`: the verdict's transform put in
  fractions of each picture, four decimals, from representative to member;
  made only for representative-member pairs (`pipeline::placements`, after
  `group::find`, which moved from `finish` into the assembly). First built
  in `Evidence`: every one of the 1.1 M report pairs grew 28 bytes and the
  stable sort holds them twice, peak 640 / 643 -> 706 / 707 MB; now
  base 624 / 624 vs new 627 / 634 (cached IMGS-ALL, ABBA), CPU within spread
  (489-497 base at >= 2,200 MHz, 496-504 new). Kept scan 11.1 -> 13.6 MB
  (51,289 members). Report byte-identical (less `runtime_seconds`); kept
  scan identical less `at`. Checked against the generated truth (15,727
  same-seed members with plain or `flip_h` geometry): corner error of the
  shared area median 0.04% of its diagonal, p99 1.2%; 14 over 5%, each a
  match the scan itself verified that way (a symmetric scene matched
  unmirrored at 0.79-0.84, an Excel sheet one grid cell over). A scan kept
  before has no places: the view says to scan again. Orientation: the
  reference's right (X, red) and up (Y, green: towards its frame's top side)
  as arrows from the centroid of the other's frame's part on the picture
  (`axes_origin`: the frame's middle when all of it is on the picture, the
  picture's when the frame holds it all; it was the frame's middle whenever
  that was on the picture, so a slide's middle put a photo's arrows near
  its top left), its own on the reference and through the
  transform on the member (`overlays`, `axes_in`, `paint_overlay`), each
  reaching half way to the side of the frame's part on the picture it
  points at (`AXIS_REACH`, at least `AXIS_LEAST` px; `axis_tips`,
  `to_edge`), 1.5-2.5 px thick by length, both dark edges drawn under both
  arrows, and the caption in words (`turned`: whole
  degrees, a quarter turn snapped within 0.5°; exact copies of a seed read
  0.02-0.06° off, `rotate_small` 1.14°). Tested by rendering off screen and
  reading the pixels back (`the_axes_are_drawn_where_and_as_the_transform_
  puts_them`: 23 orientations, both sides, direction within 3°, origin
  within 4 px, length half way to the outline or the picture's edge and,
  for a frame wholly on the member, to its sides' middles within 3 px; it
  fails with X/Y swapped, the matrix transposed, the origin moved, Y down,
  a fixed length, a reach of 0.6, the frame unclipped, no least length or the old origin rule),
  and on window screenshots of a seed's crop, mirror, 90°, 180° and 1.14°
  copies: every direction within 0.9°, origins within 0.6 px of the frame's
  middle (1.8 px on the landscape one), lengths within 1.5 px of half way
  to the clipped frame's side (the round cap).
- 0.62.1 (window only): the large view no longer blocks the page. It
  shows the page's selected image and follows it (its arrows, the page's,
  a click on a tile), marks stay in step both ways, a group chosen on the
  page shows its first image, and asking again raises it; the info is three
  lines (path; size; evidence and suggestion). Checked under Xvfb; GUI
  tests pass.
- 0.63.0 (the stated pairs, `stated.rs`; see *The stated pairs*): a
  pair is stated once whichever way round it is met (`stated::unique`), and
  what it says is worked out once into a `Pair` (evidence, file scale,
  `Place`, clean) by file numbers, which the groups, `--against` and the
  JSON's `pairs` read. Before: the JSON's `OutPair`s (two `Arc<str>` names,
  two byte strings) were also where `numbered_groups` looked a member's
  evidence up (a map filled in name order, the last winning); the place
  was `pipeline::placements`, a second map copying that order by comparing
  names; `--against` took the first pair met (`(a, b, Evidence, bool)`
  tuples). Three answers to which pair states two files, held apart only by
  no pair being stated both ways (IMGS-ALL: 0 of 1,110,436). Proved: the
  `ref.sh` battery, `last.sh`, `lastag.sh` (only "N minutes ago" differs)
  and all 90 `pairs.py` reports and lists identical to 0.62.1; cached
  IMGS-ALL report and kept scan identical, `at` of all 51,289 members
  included. Cached IMGS-ALL (ABBA): peak RssAnon+swap 608 / 595 MB base,
  455 / 457 new; CPU 470.5 / 476.1 base, 444.9 / 459.1 new. Code lines (no
  comments, blanks or tests) in the files touched 3,691 -> 3,699.
- 0.64.0 (the report's records, `table.rs`; see *The report's records*):
  a row of a report or of `--dump` is written from one list of `Column`s
  (name, which layouts carry it, how its `Cell` is read off the row), from
  which the CSV header, the CSV cells and the JSON keys are all read. Before:
  a column was a field of `report::Row` / `AnswerRow` with serde attributes,
  a line where the row was filled in, an entry in `CSV_HEADER: [&str; 15]` /
  `ANSWERS_CSV_HEADER: [&str; 20]` (hand-counted) and a cell in a list that
  matched the header by position only (the dump's too), so a swapped cell
  compiled and passed; a path was three fields (text, bytes, raw) written by
  hand seven times; the six evidence columns were copied into every place.
  Changed on purpose (user's decision): a group row's JSON keys are in the
  CSV's order, so `path` follows `action` (was first). Proved: a random
  differential test (old writers in a temporary test module, 20,000 scans and
  libraries with odd names, every relation, lapses, all three formats and
  rules): text and CSV byte-identical, JSON equal as parsed (it fails with
  one cell swapped); `ref.sh` battery against 0.63.0 the same way. Code lines
  (no comments, blanks or tests) 1,800 -> 1,782 (`report.rs` 696 -> 524,
  `table.rs` 137, `write_dump` +17).
- **IMGS5** `/home/daniel/Documents/IMGS5` (held out until 0.54.0, now read
  once): 60 seeds in 25 near-miss sets, 5,400 variants. 0.53.0 and 0.54.0
  alike: F1 0.9131, precision 85.5%, recall 97.97 / 97.94%, **11 seed pairs
  merged** (8 past 300 pairs: Mural, bird, Fountain3/5, Puerto3/4,
  Floriana3/4, Macizo5/6, Floriana1/2, Tree1/2). Every one is joined by
  *clean* cross-family anchors, which join clusters unasked. Judged by eye:
  bird1/2 (a burst) and Floriana1/2, 3/4 (a portrait shot of the landscape
  from the same spot; an original agrees with the other's crop at
  `blk_min` 0.96, 358-389 clean links) are too near and may merge; Mural
  (a person added), Puerto3/4, Macizo5/6, Tree1/2 and Parish Church are
  real merges through 2-6 clean links, nearly all from `crop_micro`,
  `embed_tiny`, `crop_strip_top` variants where the differing part is cut
  away or too small to read; Fountain3/5 (a closer shot, 73 links) between.
  The bridge test stops one such link, not two. **Open**, and IMGS5 is now
  development data: the next rule needs fresh seeds again.
- Per-corpus at 512 (0.51.0, `out/v36-cliffs`): IMGS 0.9719, IMGS2 0.9846,
  IMGS3 0.9871, IMGS4 0.9860; at 640: 0.9764, 0.9867, 0.9885, 0.9882.
- Cost: ~2,500 CPU-s / ~6 min cold on IMGS-ALL; SSCD ~55,000 CPU-s.
- Released binary is the **plain x86-64 build** (`target/plain`), which
  `bench.py` measures by default. It differs from a native build only on
  JPEG XL (jxl-grid's FMA rounding). The repo no longer ships a
  `.cargo/config.toml` with `target-cpu=native`, so a plain `cargo build` is
  the release build too (a native build on an AVX-512 host SIGILLed on AVX2
  machines). `check_cpu` reads `cpuid` itself (`lib::cpu`):
  `is_x86_feature_detected!` is constant `true` for features the build
  assumes, so the old check could never fire; it refuses with raw `write` +
  `_exit`, since formatting an error ran AVX-512 code.
- Grey is BT.601 luma; a JPEG's grey is its Y plane.
- Shipped `--min-pixel-correlation` is 0.7 (0.54.0; was 0.6), chosen for
  what users want grouped, not for F1: 0.5 grouped merely similar photos,
  and 0.6 paired five of IMGS5's sixty seeds with their near-miss sibling
  (direct, 0.63-0.70: GanyutingStore, Kaper-12/13, Lizard, Los Gigantes,
  Macizo1/2). Cost at 512: recall -1.3 to -1.7 points on every corpus
  (IMGS-ALL 96.90 -> 95.45%, F1 0.9822 -> 0.9748; mostly `tiled_watermark`
  286 -> 216 of 304, warps), precision 99.57 -> 99.60%, cross-family 7 -> 4,
  keep rule unchanged (0 lost, 297 / 304, 304 / 304; `pairs.py` loss 24,
  REVIEW +25). It does not stop IMGS5's merges (11 at 0.6-0.75, 10 at 0.8):
  those go through clean anchors, which agree at 0.9 a block.
- Default `--work-size` 512, one step below the knee at 640. 640 buys ~0.5
  points of IMGS-ALL recall (mostly containment: `embed_tiny`,
  `contact_sheet`) for ~22% more CPU on photographs. `--work-size` is the
  only knob tied to the clock; nothing is analysed above it.
- Precision: on IMGS every false pair is a rearrangement trap. On the
  near-miss corpora stray single files join a sibling photograph's family
  (the clean-anchor rule's lone-file allowance). **Watch merges, not pairs.**
- Open: no downscaled JPEG decode path (worth ~4%); the second look is
  re-asked only where the first pass anchored <2 times; nothing tested above
  ~30k images; on the found corpus about half of sampled reported pairs are
  different specimens of one species, leaking through the propagated tier;
  5 of the 7 families not keeping their original are grey or near-grey
  originals beaten by a tinted copy (`temperature`) on "colour over grey".

Docs split: `benchmark/BASELINE.md` (competition), `benchmark/VALIDATION.md`
(held-out experiment), `README.md` is **users only**: install and usage, no
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
  cache.rs      on-disk analysis cache: answers for a run's files by number,
                and decides what the file keeps (`Store::finish`)
  outcome.rs    a finished scan (groups by number, or against a library each
                scanned file's pairs; evidence, content rule's suggestions),
                as the report, --last and the window read it
  report.rs     text / CSV / JSON reports of either kind, written from an
                Outcome; each layout's rows a `table::Table`
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
benchmark/
  bench.py      measurement harness (cold, sequential, instrumented)
  score.py      F1 vs generated ground truth
  analyse.py    + traps vs cross-family, merges, two seed halves
  suggest_score.py / against_score.py   score `action` / --against reports
  pairs.py      the keep rule on two-file families (each transform alone)
  replay.py     replay anchor tier + bridge test off one --dump
  runners/      one wrapper per competitor -> canonical JSON
  corpus/       transforms.py (CATALOGUE), make_variants.py, make_combined.py,
                make_against.py
  out/vNN/      measurement sessions; v17-all4 is the field's baseline
vendor/         third-party tools and venvs (gitignored)
```

## Corpora

- **IMGS-ALL** `/home/daniel/Documents/IMGS-ALL`: hard links to the four
  corpora, ground truths merged, built by `corpus/make_combined.py`. Default
  for `bench.py`, `score.py`, `analyse.py`. Re-run with `--clean` after
  `make_variants.py` (links go stale).
- **IMGS** `/home/daniel/Documents/IMGS`: 62 seeds. Seed `beach` has no
  extension, so comparable runs need `-x '*'` (`bench.py` passes it).
- **IMGS2** (design), **IMGS3**, **IMGS4**: photographs chosen in near-miss
  sets (same scene a moment / step apart), so each set is a family of hard
  negatives. IMGS3 and IMGS4 were held out and are now **spent**; the next
  rule to defend needs fresh seeds.
- **IMGS5**: 60 seeds in 25 near-miss sets, many a portrait or closer shot
  from the landscape's own spot; not in IMGS-ALL. Held out until 0.54.0,
  read once then (status above); now spent too. Score with
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
file as it is (pairs from both ends deduped, verified from the lower);
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

The cache is opened with the run's files (canonical name, key) and answers
for them by number: `take(i)` hands over a record whose key is the file's
(once; the pipeline takes a byte group's first record for its original),
`append(i, ..)` keeps one made for it, `keep_colour(i, c)` writes a JPEG's
colour where its record now is. `finish(prune)` is the one keep rule:
this run's taken and made records, plus every record it was not asked
about whose file is there (`carried`), plus what another run appended
meanwhile; compacted when the rest is a quarter. `stale()` is only the
progress bar's guess at whether it will write.

**Adding a record kind or a reason to keep one**: a field of `Asked` or
`Unasked` and a condition in `finish` / `carried`. Not bookkeeping in the
pipeline (names, spans, counts of the cache's map).

## The claim graph (`claims.rs`, `verify::Policy`)

What a direct verdict may do is its `Standing`, read once from the three
bars a user sets (`Policy::standing`; `Policy::gate` is what the lowest asks,
`Policy::composed` judges a propagated one). `pipeline::cluster` is then a
list of rules over one edge list: `claims::unique_pairs`, the clean flags,
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
- **A claim is a transform plus pixel agreement**, never a score over a
  threshold (rejects `tile_shuffle`).
- **Pixel check**: flat blocks abstain; flatness is relative to the
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
  had 7 points at 0.997. Featureless files (no keypoints) still match only
  identical / same-pixel copies.
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
  other. The representative is not the file to keep.
- **Vocabulary sized from the corpus** (`VocabParams::for_corpus`): occupancy
  held at ~3 descriptors/word by depth *and* narrowed branching (16^depth
  steps alone swung occupancy 16x and merged families at ordinary sizes).
  A word is a live leaf's centre slot. Centres are bytes (the one lossy step,
  measured neutral). Sample = a tenth of distinct descriptors, capped 1.28 M
  (`DESC_PER_SAMPLE`). Do not fix the vocabulary size.
- **Inverted file indexes every word**; no frequency cap.
- **"Identical" is a byte comparison**, after a 4x16 KB sample hash; under a
  wildcard only possible pictures are read (`decode::may_be_image`).
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
- **EXIF orientation** is applied to the working plane, not the decoded
  picture (`decode::picture::orient`).

### Keep, delete, review (`suggest.rs`)

User requirement: **a deletion never loses picture content.** Keeps may be
the original, a lossless copy, a re-encode, or a composite holding the whole
photo. REVIEW is for weak matches close to a copy; nothing acts on it.
On IMGS-ALL at 512: 16,873 DELETE, none loses content, 297/304 families
keep the original or a lossless copy, 304/304 a faithful full copy (0.49.0:
17,259; 0.48.0: 17,300; 0.47.0:
17,299, 0, 276, 304; 0.45.0: 17,986, 1 lost, 272, 299). Scored by `suggest_score.py`
(`benchmark/suggest_score.py REPORT.json`, ~100 s a cached run), and on
two-file families by `pairs.py` (`run BINARY OUT --cache CACHE`, a cache
an IMGS-ALL run made, ~11 min; `score`, `compare`): **IMGS-ALL cannot see
the pair regime**, where every majority the rule chooses by is missing.
0.49.0: 2,241 of 27,355 pairs lost content; now 24.

Rules (constants in `suggest.rs` with reasons): clean pair at 0.85 (its own
bar now, not `Standing::Clean`'s; 0.85/0.9/0.95 lose the same, delete ~the
same); a mirrored pair, like a negative, deletes nothing either way (a flip at
q95 was kept over its q92 original 14/19 times); weak pair
deletes at >= 0.95 (`WEAK_DELETES`), REVIEW at >= 0.9 (`REVIEW`); two crops
each missing <= 10% count as one frame on deleting pairs (`CROPS`); *holds*
by frame corners, keeper may shrink <= 0.5 octave (`SHRINK`) unless the extra
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
pictures (`Traits::frames`) is deleted only for its bytes. Files refused by
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
default bar (`CUT` = 2/3 of the way from the bar 0.7 to 1; it was 0.75 of the
way from 0.6, and the cut is what was chosen), the user's choice. Neither answers to the content requirement.

### `--against LIBRARY` (`against.rs`)

Verdicts `repeat` / `better` / `review` / `new` per scanned file; `--list`
prints paths. It is the content rule with the library pinned
(`suggest::suggest_pinned`). One index over both sides; the scan queries,
then library files it reached query in turn until none is new; candidates
ranked by `both_ways`. The second look also re-asks the best-connected member
of each clean-anchor component holding a scanned file. A scanned original
whose library copy is a same-frame re-encode is `repeat` (user's decision).
Copy election: the first member of an exact group is the original, so cached
and cold runs agree. Kept for `--last` like any scan (see *A finished scan*);
`--last --list` lists it; the window does not open one.

### `--last` (`last.rs`)

Every finished scan's `Outcome` is kept in `last-scan/found.json` beside
the cache file, `Notes` in `scan.json` (shared with the window); `--last`
re-writes text/CSV byte-for-byte, JSON without `pairs`. A scan kept before
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

Cliffs, distance from shipped to nearest merge, on the four corpora and
IMGS-ALL: the 512 column at 0.54.0's default (correlation 0.7; session
scratch, not in `out/`), the 640 column at 0.51.0 with correlation 0.6
(`out/v36-cliffs/cliffs.txt`; 0.44.0's in `out/v31-cost-cliff`). At 0.7,
512: points 3 is at most 66 cross-family pairs (IMGS3 `docks`, 62); overlap
0.5 merges `docks` on IMGS-ALL (5,702), IMGS3 alone reaches 168
(`Antoniego`) and IMGS, IMGS2, IMGS4 stay at 0-1 down to 0.4; correlation
below 0.7 is the 0.51.0 sweep (no merge 0.2-0.6).

| option | 512 (default) | 640 |
|---|---|---|
| `--min-aligned-points` 10 | none down to 3 | none down to 3 (0.44.0: `bust` at 7) |
| `--min-frame-overlap` 0.85 | 0.35 (`docks` at 0.5) | 0.25 (`docks` at 0.6) |
| `--min-pixel-correlation` 0.7 | none 0.2-0.8 | 0.3 below (`game` at 0.4, IMGS3) |
| `-k` 150 | none from 10 up | none |

`bust` no longer merges at 640 on any corpus, and `Henares` no longer joins
`docks` at 512 overlap 0.3. Nearest misses (no seed pair >= 300): IMGS 640
correlation 0.2, `product-white-bg`|`sunflowers` 261; IMGS-ALL 640 overlap
<= 0.5, `Antoniego` 258-262. `-k` below 25 measures the propagation budget
(clusters get starred), not the candidate list. Every remaining merge is two
photographs of one scene.

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

glibc floor is **2.34**; `release.yml` fails if it moves. f32 libm functions
(`hypotf` is 2.35) can raise it: check
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

### Tried and rejected (numbers in `git show 5de81bc:CLAUDE.md`)

Do not retry without new evidence:
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
- Claim graph (0.54.0 survey, each rule off alone, cached IMGS-ALL at 512
  and its cliffs, IMGS3 alone): one cover test for every join, clean
  anchors included (recall -1.0, 17 perfect transforms, Segovia merges at
  the default: 1,059 pairs); every anchor weak (Segovia + game merge, 14,176
  cross-family) or every anchor clean (24 merges); no enclosure (`docks`
  merges, 93); no bridge test (`docks` at overlap 0.6, 1,545); no cycle rule
  (inert on IMGS-ALL at every cliff, Segovia merges on IMGS3 alone at
  overlap 0.6, 7,528). Every one is load-bearing somewhere.

## The window (`img-fp-gui`, feature `gui`)

- The scan is a **child process** (`img-fp-gui --worker RESULT <argv>`, which
  saves its `Outcome` to RESULT),
  so Cancel is SIGINT. Spawned from the GTK main thread (PDEATHSIG is
  per-thread) by our own fork + `execv` of `/proc/self/exe` (`scan::spawn`;
  `std::process::Command` pulls in GLIBC_2.39 weak symbols, `current_exe()`
  breaks after a package upgrade).
- The worker's `Outcome` is written to `$XDG_RUNTIME_DIR`, read off the main
  thread (`results::prepare`); read on exit 1 too. The page keeps the
  `Outcome` and asks it for each rule's actions as files go or change.
- The Trash leaves a file whose length differs from the scan's, or whose
  ctime is from the scan's start on (`Notes::changed_after`): written,
  replaced, renamed or chmod'ed since. Suggestions resting on such a file
  turn REVIEW, as under `--last`.
- Rendered with cairo; textures premultiplied BGRA at drawn size
  (`thumbs::Size`); JPEG thumbnails via libjpeg scaled decode; freedesktop
  thumbnail cache used when current (`Thumb::MTime`, and `Thumb::Size` when
  written: mtime alone survives `cp -p`/`rsync -a`); HEIF/AVIF/JXL decode one
  at a time;
  texture budget by bytes (`KEEP_BYTES`, `STRIP_KEEP`).
- Mosaic (`gui/mosaic.rs`) is virtual and its tiles pooled; strip is a
  `ListView`; `still.rs` keeps texture arrival from relaying out the page;
  tooltips/classes set only on change. Marks are vectors by file number
  (`Member::id`).
- UI decisions by the user: pictures take the page, details in a bottom bar
  of fixed columns; one selected image; Shift marks ranges; nothing
  pre-marked, suggestions offered (*Mark suggested deletions*, Alt+D),
  rule switchable without rescanning; Trash dialog names groups with every
  image marked; animations off; mnemonics always underlined (Cinnamon grabs
  Alt); F1 lists keys; dark theme followed from the desktop's text colour;
  tree view by folder (`gui/folders.rs`).
- The large view (`Results::preview`) is a window of its own, not modal
  (transient, so it stays above the page): it shows the page's *selected*
  image, so its arrows, the page's arrows and a click on a tile all move
  the one selection, and it looks again (`large_changed`, coalesced on idle)
  whenever `select` or `refresh_bar` runs; a new group selects its first
  image while it is open. Pictures are asked again only when their file
  changes. Info is three lines: path; size; evidence and suggestion.
- Mnemonics live in `gui/labels.rs` with a clash test; add labels there.
- Keys live in `gui/commands.rs`: a `Section` per part of the window
  (`WINDOW`, `SCAN`, `RESULTS`, `LARGE`), each `Row` a line of the F1 list
  and the chords it describes; the results menu is `MENU` and its items'
  mnemonics. **Adding a key**: a binding in the row that says it, and an
  arm where the section's commands are done (`Results::run` for the page);
  a new command is a variant of the section's enum, and the tests fail
  until it is bound and described. Not a key controller of its own.
- The results page's two views are `results::View` (`Groups`, `Folders`):
  a question that differs between them is one `match` (`show_view`,
  `step`, `scope`, `side_list`) or a `View` method, not a flag tested where
  it is used.
- Options are one table, `gui/options.rs` (`Opt`: key in `gui.json`, clap
  id, `Kind`, label, tip, hint; `ALL`). `Settings` holds a `Value` per
  entry, `Setup` a `Control` per entry; defaults are clap's
  (`img_fp::flag`) unless the entry says otherwise (`recursive` is on in
  the window). **Adding an option**: an `Opt` in `ALL` and a place on the
  page for its widget; a flag the window does not show goes in `BY_HAND`
  or `NOT_SHOWN` (the test says which is missing). Not a field.
- Zoom (`Mosaic::zoom`, header `[-][+]`, Ctrl+Plus/Minus/0): a chosen row
  height, steps of √2 (twice/half the area); a group that fits the page at
  rows at least that tall is fitted as before. Limits are the page's: a row
  no taller than the page shown, none shorter than the tick plus a badge line
  (`Results::least_row`; an overlay measures as its main child only), no
  step down while all is on screen. `flow` ends each row where its height is
  nearest the target (stretch or shrink), not always after the overflow.
- Paths persist (`$XDG_CONFIG_HOME/img-fp/gui.json`), and options only
  where set away from their default, under `options` (`settings::Kept`), so
  an untouched one follows a moved default; clear/prune cache never kept;
  `recursive` on by default. Folders on the command line replace the
  remembered ones. Last scan and its marks (`marks.json`, keyed by scan id)
  kept in `$XDG_CACHE_HOME/img-fp/last-scan/`.
- Testing: Xvfb + `xdotool windowfocus --sync` + `import -window root`;
  on the real Cinnamon display activate with `wmctrl -i -a`. Point
  `XDG_DATA_HOME` at scratch before testing Trash. Other distros in Docker
  with `--network host` and a distinct display number per container; install
  `librsvg2-common`. Wayland: sway headless (`setcap -r`, `wtype -s 400`,
  `grim`) or headless gnome-shell driven via `org.gnome.Shell.Eval`.

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
- **Skips vs problems** (vid-fp's summary format). Skips: extension not
  taken by `-x` (extensionless files only under `-x '*'`), non-pictures under
  a wildcard (`decode::sniff::plausible` checks short signatures), hidden folders
  without `--hidden`, Trash/thumbnail/NAS recycle and snapshot folders
  (`walk::SetAside`: `#recycle`, `@eaDir`, `#snapshot`, `@Recycle`,
  `@__thumb`, `@Recently-Snapshot` too; and `lost+found`, root's 0700 on
  every ext4 drive, which made a drive scan exit 2 and refuse a prune), symlinks
  without `--follow-symlinks`, second names for one (device, inode),
  macOS `._` metadata files (AppleDouble magic, any name; were a problem and
  a byte-identical "image" pair), featureless images (were a problem: a
  folder with one plain backdrop exited 2 every run).
  Problems: undecodable, unreadable path, unresolved `--exclude`,
  cut-off JPEG (`decode::jpeg::cut_off`; analysed, marked
  `DAMAGED`, never cached), cache I/O failure.
  **Adding a category**: a `problems::Note` variant, its arm in `Note::says`
  (skip or problem, log tag, summary label) and its place in `Note::ALL`;
  callers say `problems.note(Note::X, what)`. Not a counter of its own.
- `-o`, `--dump`, `--log-file` may not name a picture or each other; at most
  one may be stdout. Reports and the dump are written whole or not at all
  (`report::write_whole`: beside the destination, through any symlink, with
  its mode, then renamed; an interrupt removes the half-written one): a
  failed write used to leave the last report cut off. `--cache` with `--no-cache` only beside
  `--clear-cache`.
- **Cache**: one file per machine, `$XDG_CACHE_HOME/img-fp/analysis-IMGFPC15.bin`
  (named for its format, so upgrades start a new file). Records keyed by
  canonical path + size + mtime + ctime, settings per record. A run keeps
  what it took and made, and every record it was not asked about (a path it
  did not walk, or other settings) while the file exists or its volume
  (`/media`, `/run/media`, `/mnt`, gvfs, `/Volumes`) is unmounted
  (`cache::carried`, `volume_root`). Appended as made; compacted only when
  dead bytes reach a quarter. Runs sharing the file hold it with `flock`
  shared; compaction, header writing and torn-tail repair need it alone
  (`exclusive`, never waited for) and are skipped otherwise: a compaction
  under another run sent its later appends to an unlinked file. A
  compaction keeps what another run appended after this one read the file
  (`Store::foreign`), and counts it live: it used to drop it. A record's
  file is gone only on "not found" (`cache::gone`): an unreadable folder or
  a dropped sshfs/NFS mount is not a deletion.
  A JPEG's colour, measured later by the suggestion, is written in place into
  its record's colour field as `-2 - c` (below the decode's -1, so a build
  of the same format that predates it reads "not measured"); the value used
  is `cache::kept_colour(c)`, so cold and cached runs decide alike.
  Another format version's file is left untouched. Damage costs only the
  damaged record. **Cost measurements must pass `--no-cache`.** Sweeps
  should use their own `--cache`. `--prune-cache` keeps only what the scan
  used; refuses after a partial walk.
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

## Gotchas already paid for

- czkawka exits 11 on finding duplicates (`-W`), skips files < 16 KB unless
  `-m 1`; use `-z Lanczos3 --geometric-invariance mirror-flip-rotate90`.
- findimagedupes: space-separated output; needs a hard-link farm under
  `~/.cache` (it resolves symlinks).
- imagededup: no CLI, not recursive, keys by basename; runner builds a flat
  symlink farm.
- SSCD: unreadable files need a placeholder tensor of the batch's shape.
- difPy: run `.../site-packages/difPy/dif.py` directly; `-s 200` already
  merges everything.
- dupeGuru: runner supplies a PIL-backed `Photo` subclass.
- A tool that cannot decode a format looks like one that failed to match.

## Working style

- The user reads results critically and pushes back on measurement
  artifacts; check numbers against a second method before reporting.
- Do not start long tool runs without being asked.
- Report failures as results (they are in `BASELINE.md`).
