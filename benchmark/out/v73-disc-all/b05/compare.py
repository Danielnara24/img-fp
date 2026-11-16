#!/usr/bin/env python3
"""compare.py: shard 0 at bar 0.5 (b05/shard0.json) against the 0.65 run
(../shard0.json), both rankings, each run at its own bar and read off at the
stricter bars."""
import csv, json, os
H = os.path.dirname(os.path.abspath(__file__)); X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}

def load(f):
    best = {}
    for p in json.load(open(f))['pairs']:
        a, b = ident(p['a']), ident(p['b'])
        if {a[0], b[0]} != {'Q', 'R'}:
            continue
        k = (a, b) if a[0] == 'Q' else (b, a)
        v = (p['aligned_points'] or 0, p['pixel_correlation'])
        best[k] = max(best.get(k, v), v)
    return best

def score(best, lo, rank):
    sel = sorted(((rank(v), k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
    tp = ap = r90 = 0
    for i, (_, t) in enumerate(sel, 1):
        if t:
            tp += 1; ap += tp / i
        if tp / i >= 0.9:
            r90 = max(r90, tp / len(gt))
    return tp, len(sel) - tp, 100 * ap / len(gt), 100 * r90

runs = {'0.5 run': (load(f'{H}/shard0.json'), (0.0, 0.65, 0.7)), '0.65 run': (load(f'{H}/../shard0.json'), (0.0, 0.7))}
print(f'shard 0: {len(gt)} true copies')
for name, rank in (('points', lambda v: v[0]), ('points + corr/2', lambda v: v[0] + v[1] / 2)):
    print(name)
    for run, (best, bars) in runs.items():
        for lo in bars:
            tp, fp, ap, r90 = score(best, lo, rank)
            at = 'own bar' if lo == 0 else f'at {lo}'
            print(f'  {run:9s} {at:8s}: true {tp:4d} false {fp:5d}  uAP {ap:5.2f}  R@P90 {r90:5.2f}')
a, b = runs['0.5 run'][0], runs['0.65 run'][0]
ta, tb = {k for k in a if k in gt}, {k for k in b if k in gt}
print(f'true copies only the 0.5 run states: {len(ta - tb)}, only the 0.65 run: {len(tb - ta)}')
