# Candidate tools for the img-fp benchmark

Survey date: 2026-09-14. Star counts and last-push dates from the GitHub API on
that day. The rule from the vid-fp benchmark applies here unchanged: **every
tool that will be scored must be in the pool before anybody labels a pair.**

## Tier 1 — CLI, Linux, maintained, machine-readable output

| tool | lang | stars | last push | output | knob |
|---|---|---:|---|---|---|
| Czkawka `czkawka_cli image` | Rust | 33.5k | 2026-09-09 | `-C` compact JSON, `-p` pretty JSON | `-s/--max-difference` (0–40, default 5) |
| imgdupes | Python | 394 | 2026-07-21 | fdupes-style groups on stdout | hamming distance, positional |
| imagededup (idealo) | Python | 5.7k | 2025-08-15 | dict from the Python API (no CLI) | `max_distance_threshold` / `min_similarity_threshold` |
| difPy | Python | 547 | 2025-11-17 | JSON files | `-s` MSE similarity |
| PDQ (Meta ThreatExchange) | C++/Py/Go | 1.4k (monorepo) | 2026-09-11 | hash files + faiss matcher | hamming distance on 256 bits |
| findimagedupes | Perl | Debian `2.20.1-3build3` | ancient | groups on stdout | `-t` threshold |
| fclones / rdfind / jdupes | — | — | — | — | exact bytes only, the floor |

Notes per tool:

- **Czkawka** is the direct competitor and the only one here that is both fast
  and tunable. Relevant flags: `-s/--max-difference` (0–40, default 5),
  `-g/--hash-alg` (`Mean`, `Gradient` (default), `Blockhash`, `VertGradient`,
  `DoubleGradient`, `Median`), `-c/--hash-size` (8/16/32/64, default 16),
  `-z/--image-filter`, `--geometric-invariance`
  (`off` (default) / `mirror-flip` / `mirror-flip-rotate90`),
  `-C/--compact-file-to-save` for JSON, `-R/--not-recursive`, `-T/--thread-number`.
  It was also in the vid-fp pool, so the comparison carries over.
- **imgdupes** wraps `ImageHash`: `ahash`, `phash`, `dhash`, `whash`,
  `phash_org`, with `--hash-bits` (64/144/256) and optional NGT/hnsw ANN.
  Usage: `imgdupes --recursive DIR phash 4`. `--dry-run` / `-m` for reporting.
- **imagededup** has **no CLI** — it needs a ~20-line wrapper emitting our JSON.
  Worth two rows in the table: the hashing side (PHash/DHash/WHash/AHash) and
  the **CNN side** (MobileNet embeddings + cosine), which is the only mainstream
  packaged tool doing learned features rather than a hash.
- **difPy** is not a hash at all — it compares resized tensors by MSE. Different
  failure modes from everything else here, which is exactly what a pool wants.
  `-D` dirs, `-s` similarity, `-r` recursive, `-ro` rotation detection,
  `-Z` output dir, JSON by default.
- **PDQ** is the production-grade perceptual hash (Meta, 2019, 256-bit, pHash
  lineage). Meta's guidance is that distances of ≤30 (of 256) work in
  production. It ships a hasher and a faiss matcher rather than a dedup UX, so
  it needs a thin grouping wrapper — but it is the strongest classical hash in
  the field and belongs in the pool.
- **findimagedupes** (`apt install findimagedupes`) is the 20-year-old Linux
  baseline. Cheap to add, and it sets the "what did people use before" line.
- **Exact-byte finders** (fclones, rdfind, jdupes, fdupes) define the floor:
  the share of the corpus that needs no perceptual anything. vid-fp's README
  makes this argument in prose; here it can be a row.

## Tier 2 — GUI, run by hand (the MFDF slot from the vid-fp bench)

- **dupeGuru** Picture mode, 7.8k stars, pushed 2026-09-07. Cross-platform,
  fuzzy block matching, CSV export. The most likely tool a normal person
  actually uses on Linux.
- **digiKam** Similarity view / Find Duplicates. Haar-wavelet fingerprints
  (Fast Multi-Resolution Image Querying), configurable similarity range,
  requires importing the corpus into its database first.
- **AntiDupl.NET** is Windows-only and reviews report both false positives and
  missed cross-format exact duplicates. Skip unless we want a Wine run.
- Commercial Windows finders (Duplicate Photo Cleaner, Ashisoft, Awesome
  Duplicate Photo Finder) are paid and/or adware-bundled. Not worth a row.

## Tier 3 — research SOTA, and the oracle

The vid-fp benchmark needed an independent dense-sampling proposer so recall
wasn't measured against vid-fp's own candidates. The image analogue is a
learned descriptor:

- **SSCD** (Meta, CVPR 2022, "A Self-Supervised Descriptor for Image Copy
  Detection") is *the* SOTA for image **copy** detection — the exact task, as
  opposed to semantic similarity. 512-d L2-normalised descriptors, trained on
  and evaluated against **DISC2021**. The repo is archived (last push
  2022-08-02) but the weights are published, and DINOv2's own dataset curation
  used SSCD's copy-detection pipeline to strip near-duplicates.
- **DINOv2** embeddings are the general-purpose modern alternative; recent
  copy-detection papers benchmark DINOv2/MoCoV3 against SSCD at ViT-L.
- **CLIP** is the wrong tool and should be named as such in the writeup: it
  retrieves *semantically* similar images, so it proposes "two different photos
  of a beach" as readily as "the same photo twice". Useful only as a
  deliberately over-loose proposer, if at all.

Recommendation: **SSCD as the oracle**, with generous thresholds and the same
kind of admission filters gt.py grew for video — plus PDQ at a loose distance as
a second, non-learned proposer so the pool isn't one model's opinion.

## Public datasets worth knowing about

Hand-labelling stays the plan (the corpus has to look like a real library), but
these exist and can supplement:

- **California-ND** — 701 photos from one real personal travel collection,
  annotated by 10 observers into a *non-binary* ground truth. 4,609 of 245,350
  pairs were called near-duplicate by at least one subject, and **raters
  disagreed to some extent on 82% of them**. That disagreement figure is the
  single most useful number in this document: it says the SAME/DIFFERENT line
  for images is genuinely fuzzy, and our labelling guide has to define it
  operationally rather than appeal to "looks like a duplicate".
- **DISC2021** — Meta's Image Similarity Challenge set, the standard copy
  detection benchmark (1M reference images, synthetic edits). Good for
  measuring robustness to specific transforms, bad as a stand-in for a real
  photo library.
- **Copydays / INRIA Holidays / UKBench** — older, small, transform-based.

## What the image case makes different

Three things that should shape the benchmark design, not just the tool list:

1. **Crops are the new clips.** In vid-fp the differentiator was CLIP — one
   file's footage contained in another. The image analogue is a crop or an
   inset, and every global perceptual hash (pHash, PDQ, Czkawka's gradient
   hashes, difPy's MSE) degrades hard on it. Blockhash is slightly better.
   This is the axis where img-fp can separate itself, so the label vocabulary
   needs a CROP relation from day one.
2. **Formats are a real failure mode.** HEIC, AVIF, JPEG XL and camera RAW
   (CR2/NEF/ARW) are where most of these tools simply skip files. A
   parity check like `bench_parity.py` is not optional here — a tool that
   silently enumerates 60% of the corpus will otherwise post great precision.
3. **Resized/re-encoded copies are the common case**, far more so than in
   video: the same photo at full res, as a messaging-app downscale, and as a
   thumbnail. Cheap for every tool to find, so it will inflate every recall
   number the way the 19-file needle cluster did in the video bench. Plan the
   corpus so one such family isn't a third of the positives.
