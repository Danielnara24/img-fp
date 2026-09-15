# The generated half of the corpus

The corpus now has two halves that answer different questions.

**The found half** — 2,786 Kaggle butterfly photographs in `archive/test/`,
every one 224x224 JPEG. Thousands of near-identical images of the same species
by construction, which makes it a precision stress test and a distractor pool.
It cannot measure recall: there are no resizes, no format changes and no crops
in it.

**The generated half** — 38 seed photographs and 2,659 files derived from them
by `make_variants.py`, under `derived/`. This is the recall half, and because it is
*made* rather than found, its positive ground truth is exact rather than
adjudicated.

```
/home/daniel/Documents/IMGS
├── archive/test/          2,786 butterflies, the distractor pool
├── beach, document1.jpeg, logo.jpg, selfie1.avif, …
│                              38 seeds, untouched
└── derived/
    ├── Downloads/ Pictures/ WhatsApp Images/ Desktop/ …
    │                      2,659 derived files, scattered
    ├── manifest.csv        every derived file: seed, transform, region
    ├── pairs.csv           every in-family pair with its relation
    └── summary.json
```

5,483 files, 1.0 GB.

## Regenerating

```bash
cd benchmark/corpus
../../vendor/venv/bin/python make_variants.py --clean
```

Deterministic: same seeds in, same bytes and same filenames out. Adding a seed
image to the corpus root and re-running extends the corpus without disturbing
the rest.

## What the filenames do not tell you

Derived files are called things like `WhatsApp Images/DSC07149.jpg` and
`Backup/old-phone/2144.heic`. Nothing in a name says which seed it came from or
what was done to it, and the variants of one seed are scattered across nine
folders. `manifest.csv` is the only link back.

None of the ten tools currently in the pool looks at filenames, so this buys
nothing today. It is here so that the corpus stays honest for a tool that does
— including ours.

## The 70 transforms

| group | variants |
|---|---|
| container, pixels untouched | `copy_exact` `strip_metadata` `to_png` `webp_lossless` `to_tiff` |
| lossy re-encode | `jpeg_q90` `q75` `q50` `q30` `q15` `q08` `jpeg_444` `jpeg_progressive` `jpeg_gen3` `to_webp_q80` `to_webp_q40` `to_avif_q50` `to_heif_q60` `to_jxl_q85` `to_gif` |
| resolution | `scale_75` `scale_50` `scale_25` `scale_12` `thumb_320` `thumb_128` `upscale_150` `scale_nearest_50` `squash_aspect` |
| crop | `crop_center_90` `_75` `_50` `_25` `crop_square` `crop_16_9` `crop_9_16` `crop_50_upscaled` `quadrant_tl` `quadrant_br` |
| additive framing | `watermark` `caption_bar` `letterbox_16_9` `screenshot_chrome` |
| colour and tone | `bright_up` `bright_down` `contrast_up` `contrast_down` `greyscale` `saturation_up` `warm_shift` `cool_shift` `gamma_up` `sepia` `autocontrast` `posterize` |
| geometry | `flip_h` `rot90` `rot180` `exif_rot90` `rotate_3deg` `perspective` |
| degradation | `noise` `noise_heavy` `blur` `sharpen` |
| composite chains | `whatsapp` `instagram` `photo_of_screen` `print_scan` `heavy_chain` |

The 38 seeds cover the content types that break perceptual hashes in different
ways: ordinary outdoor and animal photographs, but also documents, a logo,
selfies, black-and-white, low light, panoramas, noisy high-ISO shots, a UI
screenshot, products on white, and three 200x150 images that give the
downscale chain a floor. One seed, `beach`, has no file extension.

Formats written: JPEG, PNG, WebP (lossy and lossless), AVIF, HEIC, JPEG XL,
TIFF, GIF. 2,659 of 2,660 wrote and read back. The one failure is
`strip_metadata` on an AVIF seed, where exiftool rewrites the file into
something no decoder will open; the generator deletes such an output rather
than leaving an unreadable file in the corpus that no manifest mentions.

## How the ground truth is computed

Every derived file carries a **region**: the rectangle of the original that
survives, in normalised coordinates. A re-encode or a colour shift keeps
`(0,0,1,1)`; `crop_center_50` keeps `(.25,.25,.75,.75)`. `pairs.csv` then
derives each in-family pair's relation from the two rectangles:

| relation | rule | count |
|---|---|---:|
| **SAME** | IoU ≥ 0.95 | 57,155 |
| **CROP** | one contains ≥ 95% of the other | 35,330 |
| **PARTIAL** | they overlap but neither contains the other | 1,837 |
| **DIFFERENT** | they do not overlap at all | 38 |

PARTIAL pairs are **excluded from scoring**, the same way the vid-fp benchmark
excludes SKIP. `crop_square` and `quadrant_tl` genuinely share some content and
genuinely are not duplicates; a wrong label there is worse than no label.

The 38 DIFFERENT pairs are `quadrant_tl` against `quadrant_br` — two disjoint
corners of one photograph. Each is a real CROP of the original, and against
each other they share nothing. That is the trap that separates *containment*
from *descended from the same file*, and a tool that links them is wrong.

`geometry` is recorded separately (`mirror`, `rot90`, `rotate`, `perspective`),
so a score can say "found it, but only because the tool looks for mirrors"
rather than folding that into one number.

### What this does not give you for free

The generated labels cover pairs **within** a seed family. They say nothing
about a derived file matching a butterfly, or two butterflies matching each
other. Those still have to be pooled and labelled by hand, and they are where
precision is actually decided. Pool every tool before labelling.

### Two honest caveats

- **`strip_metadata` is degenerate for the seeds that carried no metadata.** Those files carried
  no metadata, so exiftool rewrote them byte-for-byte identically and the
  variant is a second `copy_exact`. `pairs.csv` flags this in
  `identical_bytes`; do not read those as evidence of anything.
- **`exif_rot90` has two defensible right answers.** Its pixels are identical
  to the original, but its EXIF says "display this rotated", so as rendered it
  is identical to `rot90` instead. The manifest calls it SAME as the original.
  A tool that honours EXIF orientation — czkawka does — will match it to
  `rot90` and score as having missed it. That is not a tool error, and the
  scorer should treat this variant separately.

## First result: the corpus works, and it is hard

Czkawka over the whole corpus, scored by `../score.py`. `original <-> variant`
is out of 2,621 — 38 seeds x 69 transforms, with `exif_rot90` excluded for the
reason above and one `strip_metadata` that never generated:

| configuration | proposed | original ↔ variant | all pairs | precision | wall |
|---|---:|---:|---:|---:|---:|
| `-s 5 -z Nearest` (its default) | 1,777 | **248 / 2,621** (9.5%) | 2.0% | 100.0% | 17 s |
| `-s 5 -z Lanczos3` | 9,013 | **582 / 2,621** (22.2%) | 10.0% | 100.0% | 19 s |
| `-s 20 -z Lanczos3` | 10,514 | 617 / 2,621 (23.5%) | 11.7% | 100.0% | 20 s |
| `-z Lanczos3 --geometric-invariance mirror-flip-rotate90` | 41,498 | **1,704 / 2,621** (65.0%) | 46.2% | 100.0% | 140 s |
| `fclones` (byte-identical) | 79 | 50 / 2,621 (1.9%) | 0.1% | 100.0% | <1 s |

Precision is 100.0% in every row: across 41,498 proposals czkawka nominates
exactly **two** pairs outside the seed families, both butterfly-to-butterfly
and probably real. Whatever else is true, it does not guess.

**Its defaults cost it a factor of seven.** 248 pairs at the defaults, 1,704
with two flags changed and the same precision. Two findings sit inside that,
each checked in isolation before being believed:

*The default image filter cripples it on resizes.* `--image-filter` defaults to
`Nearest`, and downsampling a photograph to a 16x16 hash grid by
nearest-neighbour samples single pixels, so any resize of the source lands
somewhere else entirely. Pairwise, at the *strictest* tolerance `-s 5`:

| pair | `-z Nearest` | `-z Lanczos3` |
|---|---|---|
| original ↔ `scale_50` | no match | match |
| original ↔ `scale_25` | no match | match |
| original ↔ `thumb_320` | no match | match |
| original ↔ `whatsapp` | no match | match |

Per transform over the corpus, that flag alone moves `scale_50` from 0/38 to
16/38, `thumb_320` 0 to 15, `upscale_150` 0 to 18, `whatsapp` 0 to 14 and
`squash_aspect` 0 to 12.

*Geometry is off by default.* `--geometric-invariance` defaults to `off`, which
puts `flip_h`, `rot90` and `rot180` at **0/38** each. Switching it to
`mirror-flip-rotate90` takes all three to **37/38** — and, less obviously,
lifts everything else too, because the extra matches merge groups that were
fragmenting. It costs 7x the runtime, 19 s to 140 s.

**Czkawka puts each file in exactly one group, so families fragment.** The
original and its `jpeg_q90` re-encode match when they are alone in a directory,
and do not when the other 69 variants of the same photograph are present:
`jpeg_q90` gets claimed by a group whose reference is nearer, and the pair
spanning the two groups is never reported. This is deterministic, not noise —
the `dog2` family scored 20/70 alone and 20/70 inside the full corpus. It is a
limitation of what the tool's output can express rather than of its hash, and
it is why no transform is ever found for all 38 seeds.

**The extensionless seed is invisible to it.** `copy_exact` scores 37/38 and
the miss is always `beach`, a JPEG with no file extension: czkawka's folder
walk is extension-driven, so it never opens the file. `fclones`, which works on
bytes, scores 38/38 on the same transform. That is the enumeration gap a parity
check has to catch, now in the corpus on purpose.

**Crops are wide open.** Every crop transform scores 0-2/38 in every
configuration — `crop_center_90`, a 10% trim, is 0/38. So are `caption_bar`,
`screenshot_chrome`, `letterbox_16_9`, `heavy_chain`, `print_scan`,
`perspective` and `rotate_3deg`. A global perceptual hash has no way to express
containment, and this is the axis img-fp has to win on.

## Reading a score from this corpus

Use **original ↔ variant, broken out per transform** as the primary number.
There are 12 trials per transform, one per seed, and the resulting table says
which transformations a tool actually survives — far more useful than one
recall figure.

Do not lead with aggregate pair recall. The families are near-cliques, so the
29,228 in-family positives are dominated by variant-to-variant pairs of wildly
differing difficulty, and a single easy family moves the headline. This is the
same trap the vid-fp benchmark documents, where one 19-file cluster was 37% of
the positives.
