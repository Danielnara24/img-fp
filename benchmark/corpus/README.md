# The generated corpus

Every file in this corpus was made from a known seed, which is what lets the
benchmark report **one F1** with nothing hand-labelled.

```
/home/daniel/Documents/IMGS
├── beach, document1.jpeg, logo.jpg, selfie1.avif, …    54 seeds
├── archive/                                             8 seeds
│   bridge.webp  excel-sheet.webp  excel-sheet2.png  galaxy.jpeg
│   infographic.jpeg  painting.jpg  underwater.jpg  windows-xp.png
└── derived/
    ├── Downloads/ Pictures/ WhatsApp Images/ Backup/old-phone/ …
    │                      5,576 derived files, scattered
    ├── manifest.csv       every derived file: seed, transform, region,
    │                      coverage, scramble, seed_key
    ├── pairs.csv          every in-family pair with its relation
    └── summary.json
```

5,638 files, 2.3 GB, from 62 seeds through 90 transformations. Seeds are found
recursively, so where a photograph is filed no longer decides anything — it
used to pick which of two catalogues it faced.

## Ground truth in both directions, for free

| | how it is known | count |
|---|---|---:|
| **positive** | same seed, regions say SAME or CROP | 232,880 |
| **negative** | *different seeds* — they were never the same photograph | ~15.6 M |
| **negative** | same seed, disjoint regions or a different scramble | 16,561 |
| **excluded** | same seed, regions overlap but neither contains the other | 4,089 |

The second row is the point. An earlier version of this benchmark bolted on
9,285 found butterfly photographs to supply hard negatives, which meant
precision could only be had by hand-labelling what the tools proposed. On a
single-species corpus that is not a judgement a person can make honestly —
1,008 labelled pairs yielded 9 positives and a great deal of doubt, and what
was being measured was the labeller. Generated negatives have none of that
problem, and there are five million of them.

They are harder than "different photographs" sounds, because several seeds are
deliberate near-misses: two Excel screenshots sharing every pixel of UI
chrome, three black-and-white photographs, three panoramas, two shots of the
same product on white. A tool that matches on layout or palette instead of
content is caught by exactly the pairs that should catch it.

## One catalogue, and every amount drawn per seed

90 transformations, applied to all 62 seeds. It was three catalogues over three
sets of seeds, which meant a tool's score depended on which folder a photograph
was filed in.

**No transformation is a single fixed point.** The old catalogues put one
number through every image — `scale_50` was exactly 0.50 for all 38 of them,
`jpeg_q30` exactly quality 30 — so a transform's 38 measurements were 38
samples of one point on its axis, and a threshold could settle just past a
value it would never be asked to straddle. Now the amount is drawn per seed
from a range that straddles what a fixed catalogue would have chosen:

| | a fixed catalogue would say | this one draws from |
|---|---|---|
| `scale_small` | 0.18 | 0.09 - 0.27 |
| `jpeg_low` quality | 30 | 7 - 26 |
| `rotate_small` | 3 deg | 0.6 - 3.4 deg |
| `thumbnail` long edge | 128 px | 72 - 190 px |
| `crop_quarter` | one rectangle | four edges, each +/-0.06 |
| `contact_sheet` | a ninth of the page | a ninth or a sixteenth |

That is why 90 entries replace 196 and still cover more of every axis: one
ranged entry does the work of `jpeg_q90` through `jpeg_q08`.

Two properties keep it honest, and any edit here has to preserve both:

- **Still deterministic.** The amount comes from `seed_key_of`, a hash of the
  seed's own 16x16 grey thumbnail, recorded as `seed_key` in the manifest.
  Same seeds in, same bytes out. Nothing is random at call time.
- **The ground truth cannot drift from the pixels.** Where an amount changes
  what survives — a crop rectangle, a rotation angle, how much of a host canvas
  the photograph fills — the transform *and* its region or coverage come from
  one factory over one key (`_crop_jit`, `_rotate_jit`, `_scales`). Check it by
  cropping each original by the region the manifest records and correlating
  against the derived file; it is 1.000.

  Read the key *before* `_fit`, never after. Hosts cap their input so that
  `embed_tiny` on a 13-megapixel seed does not try to build a third of a
  gigapixel, and hashing a downscaled thumbnail gives a different key —
  `contact_sheet` spent one revision declaring a ninth of the page while
  building a sixteenth.

### What it covers

| group | transforms |
|---|---|
| container | `copy_exact` `strip_metadata` `to_png` `to_tiff` |
| lossy re-encode | `jpeg_high/mid/low` `jpeg_magick` `jpeg_ffmpeg` `jpeg_generations` `webp_lossy` `webp_lossless` `to_avif` `to_heif` `to_jxl` |
| resolution | `scale_large/mid/small` `scale_nearest` `upscale` `thumbnail` `aspect_distort` |
| crop | `crop_slight/half/quarter/micro` `crop_off_centre` `crop_strip_top` `crop_square` `crop_panoramic` |
| containment | `embed_small` `embed_tiny` `slide_deck` `phone_screenshot` `collage_cell` `blur_fill` `pdf_page` `magazine_spread` `contact_sheet` `browser_window` `picture_in_picture` `wall_poster` |
| occlusion | `watermark_corner` `tiled_watermark` `caption_bar` `redaction_bars` `blur_patch` `qr_sticker` |
| colour | `brightness` `contrast` `saturation` `greyscale` `temperature` `gamma` `hue_rotate` `invert` `posterize` `cinematic_grade` `duotone` `channel_swap` |
| geometry | `flip_h` `rot90` `rot180` `transpose_diag` `mirror_crop` `rotate_small/large` `perspective_top` `keystone_side` `shear` `barrel_distort` `pincushion` `swirl` `wave_vertical` |
| degradation | `noise` `blur` `motion_blur` `sharpen` `halftone` `chromatic_aberration` `vignette` |
| rearrangement | `tile_shuffle` `quadrant_rotate` `column_roll` |
| alpha | `alpha_on_white` `alpha_on_black` |
| pipelines | `messaging_app` `print_scan` `photo_of_screen` `video_call_frame` |

Three encoders write the JPEGs — Pillow, ImageMagick and ffmpeg — so "robust to
JPEG" cannot quietly mean "robust to *Pillow's* JPEG".

## How a pair's relation is decided

Each derived file records three things about what it did to its seed:

- **region** — the rectangle of the original that survives, normalised.
- **coverage** — how much of the *derived* file is original content. A slide
  holding a photograph keeps 100% of the region at 35% coverage.
- **scramble** — a key naming the rearrangement, if any.

`pair_relation` applies them in that order of authority:

1. **Different scramble keys → DIFFERENT**, whatever the regions say. This is
   the only rule that overrides a perfect region match, and it is the trap for
   anything that reduces an image to a global colour summary: `tile_shuffle_2x2`
   has the original's exact histogram and is not the original.
2. **Region** → SAME at IoU ≥ 0.95, CROP when one contains ≥ 95% of the other,
   DIFFERENT when they do not overlap, PARTIAL otherwise.
3. **Coverage** downgrades SAME to CROP when one side's coverage is under 90%
   of the other's. Both hold the whole original, but one is mostly something
   else — that is containment, not equality. Without this rule "the image" and
   "a poster with the image on it" would be called the same file.

PARTIAL pairs are excluded from scoring at both ends, the way the vid-fp
benchmark excludes SKIP. They genuinely share content and genuinely are not
duplicates; a wrong call there is worse than no call.

## Regenerating

```bash
cd benchmark/corpus
../../vendor/venv/bin/python make_variants.py --clean          # ~13 min
../../vendor/venv/bin/python make_variants.py --only crop_micro,swirl
```

Deterministic: same seeds in, same bytes and same filenames out. Dropping an
image anywhere under the corpus extends it, and it gets the whole catalogue.

5,576 of 5,580 files wrote and read back. The four failures are
`strip_metadata` on the AVIF seeds, where exiftool rewrites the file into
something no decoder will open; the generator deletes such an output rather
than leaving an unreadable file in the corpus that no manifest mentions.

## What the filenames do not tell you

Derived files are called things like `WhatsApp Images/DSC07149.jpg` and
`Backup/old-phone/2144.heic`. Nothing in a name says which seed it came from
or what was done to it, and the variants of one seed are scattered across nine
folders. `manifest.csv` is the only link back. None of the tools in the pool
looks at filenames, so this buys nothing today; it is here so the corpus stays
honest for one that does — including ours.

## Two caveats

- **`strip_metadata` is degenerate for seeds that carried no metadata.**
  exiftool rewrote them byte-for-byte identically, so the variant is a second
  `copy_exact`. `pairs.csv` flags this in `identical_bytes`.
- **`exif_rot90` has two defensible right answers.** Its pixels match the
  original; its EXIF says to display it rotated, so as rendered it matches
  `rot90` instead. The manifest calls it SAME as the original, a tool that
  honours orientation will match it to `rot90`, and `score.py` excludes it by
  default. `invert` is arguably similar and is *not* excluded — it is
  trivially reversible and unambiguously derived — but `--exclude-variants`
  takes it if you disagree.
