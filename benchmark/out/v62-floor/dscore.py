#!/usr/bin/env python3
"""dscore.py REPORT.json ...: DISC21 shard 0 (references_0) from each report's
pairs. At each bar, the pairs whose stated correlation meets it, ranked by
aligned points + correlation / 2 (score_shards.py's way): true and false
copies, uAP, recall, recall at 90% precision."""
import csv, json, os, sys
X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
def qr(a, b):
    a, b = ident(a), ident(b)
    if {a[0], b[0]} != {'Q', 'R'}: return None
    return (a, b) if a[0] == 'Q' else (b, a)
print(f'{len(gt)} true copies in reach')
for f in sys.argv[1:]:
    best = {}
    for p in json.load(open(f))['pairs']:
        k = qr(p['a'], p['b'])
        if k:
            v = (p['aligned_points'] or 0, p['pixel_correlation']); best[k] = max(best.get(k, v), v)
    print(f'== {f}')
    for lo in (0.4, 0.5, 0.6, 0.7, 0.8, 0.9):
        sel = sorted(((v[0] + v[1] / 2, k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
        tp = ap = r90 = 0
        for i, (_, t) in enumerate(sel, 1):
            if t: tp += 1; ap += tp / i
            if tp / i >= 0.9: r90 = max(r90, tp / len(gt))
        print(f'  bar {lo:.1f}: true {tp:4d} false {len(sel)-tp:5d}  uAP {100*ap/len(gt):5.2f}  recall {100*tp/len(gt):5.2f}  R@P90 {100*r90:5.2f}')
