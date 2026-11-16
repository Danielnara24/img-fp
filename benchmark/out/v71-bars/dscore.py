#!/usr/bin/env python3
"""dscore.py REPORT.json: DISC21 shard 0 from a run's stated pairs, all of
them (the run's own bars decide what is stated), ranked by aligned points +
correlation / 2 (validate.py's `disc_score`): true and false copies, uAP,
recall at 90% precision."""
import csv, json, os, sys
X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
best = {}
for p in json.load(open(sys.argv[1]))['pairs']:
    a, b = ident(p['a']), ident(p['b'])
    if {a[0], b[0]} != {'Q', 'R'}:
        continue
    k = (a, b) if a[0] == 'Q' else (b, a)
    v = (p['aligned_points'] or 0, p['pixel_correlation'])
    best[k] = max(best.get(k, v), v)
ranked = sorted(((v[0] + v[1] / 2, k in gt) for k, v in best.items()), reverse=True)
tp = ap = r90 = 0
for i, (_, t) in enumerate(ranked, 1):
    if t:
        tp += 1; ap += tp / i
    if tp / i >= 0.9:
        r90 = max(r90, tp / len(gt))
print(json.dumps({'copies': len(gt), 'true': tp, 'false': len(ranked) - tp, 'uap': round(100 * ap / len(gt), 2), 'r90': round(100 * r90, 2)}))
