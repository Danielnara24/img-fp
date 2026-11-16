#!/usr/bin/env python3
"""pershard.py: each shard scored alone (its own refs), ranked by points + corr/2, at the run bar."""
import csv, glob, json, os, re
H = os.path.dirname(os.path.abspath(__file__)); X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
allgt = [(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1]]
for n in range(20):
    refs = {l.strip() for l in open(f'{X}/refs{n}.ids')}
    gt = {g for g in allgt if g[1] in refs}
    best = {}
    for p in json.load(open(f'{H}/shard{n}.json'))['pairs']:
        a, b = ident(p['a']), ident(p['b'])
        if {a[0], b[0]} != {'Q', 'R'}: continue
        k = (a, b) if a[0] == 'Q' else (b, a)
        v = p['aligned_points'] or 0
        v = v + p['pixel_correlation'] / 2
        best[k] = max(best.get(k, v), v)
    sel = sorted(((v, k in gt) for k, v in best.items()), reverse=True)
    tp = ap = 0
    for i, (_, t) in enumerate(sel, 1):
        if t: tp += 1; ap += tp / i
    print(f'shard {n:2d}: copies {len(gt):3d} true {tp:3d} false {len(sel)-tp:4d} uAP {100*ap/len(gt):5.2f}')
