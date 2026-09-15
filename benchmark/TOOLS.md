# The competing tools, installed and running

Everything in this file is vendored under `vendor/`, which is gitignored. The
corpus is `/home/daniel/Documents/IMGS` — 9,285 JPEGs, one Kaggle butterfly
classification set, every image exactly 224x224 RGB, 245 MB total.

Nothing here needed root. `vendor/` can be deleted and rebuilt from the
commands in each section.

## What runs

| tool | version | how it's installed | wrapper |
|---|---|---|---|
| Czkawka | 12.0.2 | release binary, HEIF/RAW/AVIF build | `run_czkawka.py` |
| imgdupes | 0.1.3 | `vendor/venv-imgdupes` | `run_imgdupes.py` |
| imagededup | 0.3.3.post2 | `vendor/venv-imagededup` (torch 2.14 CPU) | `run_imagededup.py` |
| difPy | 4.2.1 | `vendor/venv` | `run_difpy.py` |
| PDQ | pdqhash 0.2.8 | `vendor/venv` | `run_pdq.py` |
| findimagedupes | 2.20.1 | unpacked .debs, no root | `run_findimagedupes.py` |
| dupeGuru | 4.3.1 | source + built C extensions | `run_dupeguru.py` |
| SSCD | disc_mixup | TorchScript weights | `run_sscd.py` |
| fclones | 0.35.0 | `cargo install --root vendor` | (exact-byte baseline) |
| digiKam | 9.1.0 | `flatpak --user` | GUI, run by hand |

Every wrapper writes the same JSON shape, so one scorer can read all of them:

```json
{"tool": "...", "config": {...}, "runtime_seconds": 0.0,
 "groups": [["/path/a", "/path/b"], ...],
 "pairs":  [{"a": "/path/a", "b": "/path/b"}, ...]}
```

`groups` is the union-find closure of `pairs`. Tools that natively report
groups (czkawka, imgdupes, findimagedupes) get `pairs` as the group closure
instead, which is the same claim: a tool that puts A, B and C in one group is
asserting A–C as much as A–B.

## Smoke run, all tools at their own defaults

These are **not** benchmark numbers. Several runs overlapped on an 8-core
machine, so the timings are contaminated; they are here to show each tool
works and to show how far apart the defaults are.

| tool | configuration | groups | pairs | wall |
|---|---|---:|---:|---:|
| fclones | byte-identical | 5 | 6 | 0.4 s |
| czkawka | `-s 5 -g Gradient -c 16` | 7 | 9 | 13 s |
| PDQ | distance 31 / 256 | 31 | 38 | 68 s |
| imgdupes | `phash 4`, 64 bits | 60 | 73 | 42 s |
| difPy | `-s 0 -px 50`, same dimensions | 74 | 75 | 19 min |
| findimagedupes | `-t 90%` | 122 | **59,941** | 5 m 32 s |
| imagededup | `phash`, distance 10 | 306 | 946 | 2 m 02 s |
| dupeGuru | threshold 95, same dimensions | 21 | 28 | 4 m 52 s |
| imagededup | `cnn`, cosine 0.9 | 626 | **10,850** | 4 m 35 s |
| SSCD | cosine 0.6 | 786 | 2,449 | 57 min |

Three things that table says before any ground truth exists:

- **The defaults are nowhere near each other.** Czkawka proposes 9 pairs,
  imagededup 946, both calling it "the default". Any comparison that quotes
  one number per tool is comparing threshold choices, not tools. The benchmark
  needs a sweep per tool, and a matched-precision comparison, the way the
  vid-fp one does.
- **findimagedupes at its default collapses.** Its 122 groups include one
  group of 345 files, which alone is 59,340 of those pairs. A transitive
  closure over a loose threshold is how a similar-image finder fails on a
  corpus of visually related photographs, and this corpus — 9,285 photos of
  butterflies on flowers — is exactly that. It needs a tighter `-t` to be
  worth scoring.
- **Cost spans three orders of magnitude.** 0.4 s to 19 minutes for the same
  9,285 files. The vid-fp benchmark's cost protocol (cold cache every run, a
  cooldown between runs, peak RSS summed over the process tree) transfers
  directly and matters more here.
- **imagededup's CNN mode groups by subject, not by copy.** At its default
  cosine 0.9 it proposes 10,850 pairs, and its largest group holds **453
  files** — on a corpus of 9,285 butterfly photographs, MobileNet features at
  that threshold are finding *the same species*, not the same image. This is
  the failure mode that makes a general-purpose embedding the wrong oracle and
  SSCD, trained for copy detection specifically, the right one. Score the CNN
  mode on a sweep or not at all; its default number means nothing here.

## Per tool

### Czkawka 12.0.2 — `vendor/bin/czkawka_cli`

```bash
curl -L -o vendor/bin/czkawka_cli \
  https://github.com/qarmin/czkawka/releases/download/12.0.2/linux_czkawka_cli_heif_raw_avif_x86_64
chmod +x vendor/bin/czkawka_cli
```

The `heif_raw_avif` build, not the plain one, so format coverage is a fair
comparison later.

Three flags the wrapper sets that its own defaults get wrong for a benchmark:

- **`-m 1`.** Czkawka skips files under `--minimal-file-size`, which defaults
  to **16384 bytes**. 315 of this corpus's 9,285 images are smaller than that
  and would be silently dropped — a 3.4% enumeration gap that would show up as
  free precision.
- **`-H`.** Czkawka caches hashes between runs. A second run measures a
  database read, not a scan.
- **`-W`.** Czkawka exits **11**, not 0, whenever it finds anything. Without
  this a successful scan looks like a crash to any script checking the code.

### imgdupes 0.1.3 — `vendor/venv-imgdupes`

```bash
python3 -m venv vendor/venv-imgdupes
vendor/venv-imgdupes/bin/pip install imgdupes
```

Wraps `ImageHash`: `ahash`, `phash`, `dhash`, `whash`, `phash_org`, with
`--hash-bits` (64, 144, 256) and optional NGT/hnsw approximate search. The
wrapper passes `--dry-run --no-cache` so a run is a real scan and deletes
nothing. Output is fdupes-style text; there is no machine-readable mode, so
`run_imgdupes.py` parses stdout and validates every token is a file it passed in.

### imagededup 0.3.3.post2 — `vendor/venv-imagededup`

```bash
python3 -m venv vendor/venv-imagededup
vendor/venv-imagededup/bin/pip install \
  --extra-index-url https://download.pytorch.org/whl/cpu imagededup
```

No CLI at all; `run_imagededup.py` is the CLI. `--method` covers `phash`,
`dhash`, `whash`, `ahash` and `cnn` (MobileNet embeddings + cosine), which is
the only learned matcher in a mainstream packaged dedup tool.

**A trap worth knowing:** `encode_images(image_dir=...)` is not recursive and
keys its results by **basename**. This corpus has `archive/train/Image_1000.jpg`
*and* `archive/test/Image_1000.jpg`; called the obvious way the library would
silently collapse them and lose half the corpus. The wrapper builds a flat farm
of uniquely-named symlinks and points the library at that instead.

### difPy 4.2.1 — `vendor/venv`

Not a hash: mean squared error between images downscaled to 50x50. The only
non-hash matcher in the pool, so it fails in different places from the rest —
which is the point of pooling it.

Two defaults do most of the work:

- **`-s 0`** means MSE 0, near-pixel equality after the downscale. Much
  stricter than "similarity 0" sounds.
- **`-dim True`** means images of different dimensions are *never compared*.
  On a corpus of resized copies that single flag decides its recall.

It is also the slowest thing here by a wide margin: 19 minutes of O(n²)
comparison against czkawka's 13 seconds.

### PDQ — `pdqhash` 0.2.8 in `vendor/venv`

Meta's production perceptual hash, 256 bits, pHash lineage. It ships a hasher
and a faiss matcher, not a dedup tool, so `run_pdq.py` supplies the grouping:
hash everything, take every pair within `--distance`, union-find the pairs.
Default distance 31 of 256, at the top of the range Meta's own docs call a
match. Hashing 9,285 images takes 21 s; the brute-force matching takes another
47 s and is the obvious thing to swap for faiss if the corpus grows.

### findimagedupes 2.20.1 — `vendor/findimagedupes`, `vendor/bin/findimagedupes`

The twenty-year-old Linux baseline: a 16x16 monochrome fingerprint compared by
bit overlap. **It does not need root**, contrary to first appearances. `apt-get
download` works unprivileged, and `dpkg-deb -x` unpacks a package tree that
Perl can be pointed at:

```bash
cd /tmp/fid
apt-get download findimagedupes libgraphicsmagick-q16-3t64 libyaml-libyaml-perl
for p in libgraphics-magick-perl libinline-perl libinline-c-perl \
         libparse-recdescent-perl libfile-sharedir-perl libclass-inspector-perl \
         libparams-util-perl libpegex-perl libfile-slurp-tiny-perl \
         libfile-copy-recursive-perl; do
  u=$(apt-get download --print-uris $p | awk '{print $1}' | tr -d "'" \
      | sed -E 's#https?://[^/]+/ubuntu#http://archive.ubuntu.com/ubuntu#')
  curl -sfL -O "$u"
done
for d in *.deb; do dpkg-deb -x "$d" root/; done
cp -a root/. <repo>/vendor/findimagedupes/
```

`vendor/bin/findimagedupes` is a launcher that sets `PERL5LIB` and
`LD_LIBRARY_PATH` at that tree. (If the configured apt mirror is flaky, as it
was here, rewriting the host to `archive.ubuntu.com` is what makes this work.)

The wrapper feeds it the file list on **stdin** rather than letting it walk the
directory, so it enumerates exactly what every other tool does. Its output is
space-separated paths, one group per line, with no NUL mode — so a path
containing a space is genuinely unparseable and `run_findimagedupes.py` refuses
to guess.

### dupeGuru 4.3.1 — `vendor/dupeguru`

dupeGuru ships as a GUI with no CLI, but **its matcher is a library**, so it
does not have to be the hand-run GUI row the way MFDF was in the vid-fp
benchmark:

```bash
curl -L https://github.com/arsenetar/dupeguru/releases/download/4.3.1/dupeguru_4.3.1.tar.xz \
  | tar xJ -C vendor/dupeguru --strip-components=1
cd vendor/dupeguru && ../venv/bin/python build_pe_modules.py   # needs python3-dev headers
```

`run_dupeguru.py` imports `core.pe.matchblock` directly — the real 15x15
average-colour block matcher backed by the `_block` C extension. The two pieces
the GUI normally supplies are supplied by the wrapper: a PIL-backed `Photo`
subclass in place of the Qt one (so no Qt is needed, and EXIF orientation is
handled the same way), and the threshold, which dupeGuru calls "filter
hardness" and defaults to 95.

`--match-scaled` is dupeGuru's "match pictures of different dimensions", off by
default in the GUI and off here. Like difPy's `-dim`, on a corpus of resized
copies that flag alone decides its recall.

It also needs `semantic_version` at import time, which its tarball does not
declare.

### SSCD — `vendor/models/sscd_disc_mixup.torchscript.pt`

```bash
curl -L -o vendor/models/sscd_disc_mixup.torchscript.pt \
  https://dl.fbaipublicfiles.com/sscd-copy-detection/sscd_disc_mixup.torchscript.pt
```

The pool's independent proposer: a descriptor trained for *copy* detection
rather than semantic similarity, so nothing it finds comes from a perceptual
hash. 512-d L2-normalised output; SSCD's own README puts cosine 0.75 at about
90% precision on DISC2021, so the wrapper defaults to 0.6 — the oracle wants
recall and leaves the rejecting to the human.

Runs on the `vendor/venv-imagededup` interpreter, reusing the torch that is
already there. On this machine — 8 cores, no GPU — embedding the corpus took
**57 minutes** and is the long pole of the whole pool, so `--embeddings`
caches the descriptors to an `.npz`. Every sweep point after that is seconds:

| cosine | groups | pairs | largest group |
|---:|---:|---:|---:|
| 0.90 | 344 | 395 | 4 |
| 0.85 | 486 | 563 | 4 |
| 0.80 | 582 | 685 | 10 |
| 0.75 | 639 | 815 | 22 |
| 0.70 | 700 | 1110 | 30 |
| 0.65 | 736 | 1561 | 52 |
| 0.60 | 786 | 2449 | 59 |

Read that the way the vid-fp benchmark reads its `-d` ladder. Between 0.90 and
0.75 each rung buys groups roughly as fast as it buys pairs — new *findings*.
Below 0.75 that reverses: 0.75 to 0.60 triples the pairs (815 to 2,449) while
adding only 147 groups, and the largest group grows from 22 files to 59. Those
later pairs are mostly thickening clusters that already exist, which is what
threshold noise looks like. 0.75 is also where SSCD's own README puts ~90%
precision on DISC2021, so the two agree. Whatever the oracle finally admits on,
it should be pooled generously and filtered afterwards — `gt.py`'s job in the
vid-fp benchmark — not chosen by picking a number here.

Worth contrasting with imagededup's CNN mode above: at *its* default the
largest group is 453 files. SSCD at its loosest setting here tops out at 59.
That gap is the difference between a descriptor trained for copy detection and
a general-purpose classification backbone, measured on the same corpus.

### fclones 0.35.0 — `vendor/bin/fclones`

```bash
cargo install fclones --root vendor
```

Byte-identical only. It is the floor row: on this corpus it finds **5 groups,
11 files**, in 0.4 seconds. Everything any other tool reports beyond that is
what perceptual matching actually buys.

### digiKam 9.1.0 — flatpak

```bash
flatpak remote-add --user --if-not-exists flathub \
  https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.kde.digikam
flatpak run org.kde.digikam
```

GUI only, and it wants the corpus imported into its own database before
Similarity -> Find Duplicates will run. Haar-wavelet fingerprints, with a
configurable similarity range. This is the hand-run row; it needs a human to
drive it and to export the result.

## Not installed

- **AntiDupl.NET** — Windows only. Would need Wine, and published comparisons
  report both false positives and *missed* cross-format exact duplicates.
- **Commercial Windows finders** (Duplicate Photo Cleaner, Ashisoft, Awesome
  Duplicate Photo Finder) — paid, and at least one bundles adware.
- **DINOv2** — a second learned proposer alongside SSCD. Nothing blocks it; it
  is just another slow CPU embedding pass, and SSCD is the better-targeted
  model for copy detection. Add it if the pool needs more independence.

## What this corpus can and cannot measure

A benchmark on `/home/daniel/Documents/IMGS` alone would answer the wrong
question, and it is worth being explicit about why before any labelling starts:

- **Every image is already 224x224.** There are no resized copies, so the most
  common real-world duplicate — the same photo at full resolution, as a
  messaging-app downscale, and as a thumbnail — does not occur. Worse, the two
  tools whose "compare different dimensions" flag is off by default (dupeGuru,
  difPy) are handed a corpus where that flag costs them nothing.
- **Every image is JPEG.** No HEIC, AVIF, JXL or camera RAW, so the format
  coverage gap — where several of these tools simply skip files — is invisible.
- **There are no crops.** Which is the relation img-fp most needs to be
  measured on, and the one every global perceptual hash here degrades on.
- **It is a classification dataset**, so near-identical images of the same
  species are everywhere by construction. That is useful — it is a genuinely
  hard precision test, and findimagedupes' 345-file group shows it working —
  but it makes the corpus adversarial in one direction and empty in three
  others.

So: fine as the precision half, and fine for shaking the tools out, which is
what it has done. The recall half needs a second corpus with resizes,
re-encodes, format conversions and crops in it.
