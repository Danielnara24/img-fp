#!/usr/bin/env python3
"""compare.py: shard 0 runs at bars 0.3 (b03), 0.5 (b05), 0.65 (../shard0.json),
each at its own bar and read off at every stricter one; ranked by aligned points."""
import csv, json, os
H = os.path.dirname(os.path.abspath(__file__)); X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
def load(f):
    best = {}
    for p in json.load(open(f))['pairs']:
        a, b = ident(p['a']), ident(p['b'])
        if {a[0], b[0]} != {'Q', 'R'}: continue
        k = (a, b) if a[0] == 'Q' else (b, a)
        v = (p['aligned_points'] or 0, p['pixel_correlation'])
        best[k] = max(best.get(k, v), v)
    return best
def score(best, lo):
    sel = sorted(((v[0], k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
    tp = ap = r90 = 0
    for i, (_, t) in enumerate(sel, 1):
        if t: tp += 1; ap += tp / i
        if tp / i >= 0.9: r90 = max(r90, tp / len(gt))
    return tp, len(sel) - tp, 100 * ap / len(gt), 100 * r90
runs = [(0.3, load(f'{H}/shard0.json')), (0.5, load(f'{H}/../b05/shard0.json')), (0.65, load(f'{H}/../shard0.json'))]
print(f'shard 0: {len(gt)} true copies; ranked by aligned points')
for bar, best in runs:
    for lo in (0.3, 0.4, 0.5, 0.6, 0.65, 0.7):
        if lo < bar: continue
        tp, fp, ap, r90 = score(best, lo if lo > bar else 0)
        print(f'  run {bar:4} at {lo:4}: true {tp:4d} false {fp:6d}  uAP {ap:5.2f}  R@P90 {r90:5.2f}')
t = {bar: {k for k in b if k in gt} for bar, b in runs}
print(f'true only at 0.3 vs 0.5: {len(t[0.3] - t[0.5])}, only at 0.5 vs 0.3: {len(t[0.5] - t[0.3])}')
