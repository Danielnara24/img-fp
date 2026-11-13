#!/usr/bin/env python3
"""psnr.py PAIRSDIR: for the scale transforms, how well the smaller file, enlarged, reproduces the original (PSNR of
luma, dB), where the original was deleted for it and where it was kept."""
import sys, os, json, csv, collections
import numpy as np
from PIL import Image
Image.MAX_IMAGE_PIXELS = None
man = {r['path']: r for r in csv.DictReader(open(os.path.expanduser('~/Documents/IMGS-ALL/derived/manifest.csv')))}
def luma(p, size=None):
    im = Image.open(p).convert('L')
    if size and im.size != size: im = im.resize(size, Image.BICUBIC)
    return np.asarray(im, dtype=np.float32)
rows = []
for v in ['scale_large', 'scale_mid', 'scale_small', 'scale_nearest', 'thumbnail']:
    rep = json.load(open(os.path.join(sys.argv[1], v + '.json')))
    act = {}
    for g in rep['groups']:
        for f in g['files']: act[f['path']] = f.get('action')
    by_seed = collections.defaultdict(dict)
    for p, a in act.items():
        if p in man: by_seed[man[p]['seed']][man[p]['variant']] = (p, a)
    for seed, d in by_seed.items():
        if 'ORIGINAL' not in d or v not in d: continue
        (po, ao), (pv, av) = d['ORIGINAL'], d[v]
        if not (ao == 'DELETE' and av != 'DELETE') and len(rows) > 400 and hash(seed) % 6: continue
        o = Image.open(po); 
        if o.size[0] * o.size[1] > 30e6: continue
        a = luma(po); b = luma(pv, o.size)
        mse = float(np.mean((a - b) ** 2)); ps = 99.0 if mse == 0 else 10 * np.log10(255 ** 2 / mse)
        rows.append((ao == 'DELETE' and av != 'DELETE', ps, v, seed, int(man[pv]['width']) / o.size[0]))
lost = sorted(r for r in rows if r[0]); kept = sorted(r[1] for r in rows if not r[0])
print('originals deleted for their smaller copy:')
for r in lost: print(f"   {r[1]:5.1f} dB  {r[2]:14s} x{r[4]:.2f}  {r[3]}")
q = lambda xs, p: xs[min(len(xs) - 1, int(p * len(xs)))]
print(f"originals not deleted for it, n={len(kept)}: p10 {q(kept,.1):.1f}  p50 {q(kept,.5):.1f}  p90 {q(kept,.9):.1f}  p99 {q(kept,.99):.1f} dB")
