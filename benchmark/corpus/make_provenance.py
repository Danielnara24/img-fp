#!/usr/bin/env python3
"""Build IMGS-PROVENANCE: families with an original and lossy or edited copies only.

Every IMGS-ALL family also holds lossless copies of its original (`to_png`, `to_tiff`,
`webp_lossless`, `copy_exact`, `strip_metadata`), and the keep rule ranks lossless first, so
`suggest_score.py`'s "keeps its original or a lossless copy" cannot fail on which of several
*lossy* copies is kept. Here nothing is lossless: each family is a JPEG seed and

  reencode_q85 / q90 / q92 / q97   the seed decoded and saved again
  noise                            Gaussian noise, sigma 4, saved at q92
  sharpen                          unsharp mask, saved at q92
  stripped                         the seed's own bytes less its APPn (EXIF, XMP, ICC) and COM
                                   segments: the same pixels, no metadata
  crop90                           the top-left 90% of each side, at q90
  half                             half the size, at q90

Two trees, the same files:

  dated/     the original written first and each copy a minute after the one before, as
             copies are made after what they are made from
  same-time/ every file with the original's modification time, as `cp -p` and `rsync -a`
             leave them

Ground truth is `manifest.csv` (tree, family, file, variant); `provenance_score.py` scores a
report against it. Seeds are the top-level JPEGs of IMGS, IMGS2, IMGS3 and IMGS4.
Deterministic: noise is seeded per family.

Usage: make_provenance.py [--out ~/Documents/IMGS-PROVENANCE] [--clean]
"""
import argparse, csv, hashlib, io, os, shutil, sys
import numpy as np
from PIL import Image, ImageFilter, ImageOps

CORPORA = ['IMGS', 'IMGS2', 'IMGS3', 'IMGS4']
BASE = 1_600_000_000  # the original's modification time, seconds


def strip(b: bytes) -> bytes:
    """A JPEG's bytes less its APP1..APP15 and COM segments; the image data untouched."""
    assert b[:2] == b'\xff\xd8'
    out, i = bytearray(b[:2]), 2
    while i + 4 <= len(b):
        if b[i] != 0xFF:
            raise ValueError('not at a marker')
        m = b[i + 1]
        if m == 0xDA:  # start of scan: the rest is image data
            out += b[i:]
            return bytes(out)
        n = int.from_bytes(b[i + 2:i + 4], 'big')
        if not (0xE1 <= m <= 0xEF or m == 0xFE):
            out += b[i:i + 2 + n]
        i += 2 + n
    raise ValueError('no image data')


def jpeg(im: Image.Image, q: int) -> bytes:
    buf = io.BytesIO()
    im.save(buf, 'JPEG', quality=q)
    return buf.getvalue()


def variants(path: str):
    raw = open(path, 'rb').read()
    turned = Image.open(io.BytesIO(raw)).getexif().get(0x0112, 1) not in (0, 1)
    im = ImageOps.exif_transpose(Image.open(io.BytesIO(raw))).convert('RGB')
    seed = int.from_bytes(hashlib.sha256(raw).digest()[:4], 'little')
    a = np.asarray(im).astype(np.float32)
    noisy = Image.fromarray(np.clip(a + np.random.default_rng(seed).normal(0, 4, a.shape), 0, 255).astype(np.uint8))
    w, h = im.size
    out = [('original', raw)]
    out += [(f'reencode_q{q}', jpeg(im, q)) for q in (85, 90, 92, 97)]
    out += [('noise', jpeg(noisy, 92)), ('sharpen', jpeg(im.filter(ImageFilter.UnsharpMask(2, 80, 0)), 92))]
    # Not of a seed its EXIF turns: without the tag its pixels show another way up.
    try:
        if not turned:
            out.append(('stripped', strip(raw)))
    except (ValueError, AssertionError):
        pass
    out.append(('crop90', jpeg(im.crop((0, 0, w * 9 // 10, h * 9 // 10)), 90)))
    out.append(('half', jpeg(im.resize((max(w // 2, 1), max(h // 2, 1)), Image.LANCZOS), 90)))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--out', default=os.path.expanduser('~/Documents/IMGS-PROVENANCE'))
    ap.add_argument('--clean', action='store_true')
    args = ap.parse_args()
    if args.clean and os.path.exists(args.out):
        shutil.rmtree(args.out)
    os.makedirs(args.out, exist_ok=True)
    seeds = []
    for c in CORPORA:
        d = os.path.expanduser(f'~/Documents/{c}')
        for f in sorted(os.listdir(d)):
            p = os.path.join(d, f)
            if os.path.isfile(p) and open(p, 'rb').read(3) == b'\xff\xd8\xff':
                seeds.append((c, p))
    rows = []
    for n, (corpus, p) in enumerate(seeds):
        family = f'{corpus}_{os.path.splitext(os.path.basename(p))[0]}'.replace(' ', '_').replace('&', '_')
        made = variants(p)
        for tree in ('dated', 'same-time'):
            fam = os.path.join(args.out, tree, family)
            os.makedirs(fam, exist_ok=True)
            for k, (v, data) in enumerate(made):
                f = os.path.join(fam, f'{v}.jpg')
                with open(f, 'wb') as fh:
                    fh.write(data)
                t = BASE + (60 * k if tree == 'dated' else 0)
                os.utime(f, ns=(t * 10**9, t * 10**9))
                rows.append((tree, family, os.path.abspath(f), v))
        print(f'{n + 1}/{len(seeds)} {family}', file=sys.stderr)
    with open(os.path.join(args.out, 'manifest.csv'), 'w', newline='') as fh:
        w = csv.writer(fh)
        w.writerow(['tree', 'family', 'path', 'variant'])
        w.writerows(rows)


if __name__ == '__main__':
    main()
