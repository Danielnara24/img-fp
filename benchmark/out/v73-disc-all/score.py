#!/usr/bin/env python3
"""score.py: the whole dev set from shard0..19.json (or those present):
every query-reference pair, the best per pair over shards, against the
ground truth limited to the shards present. Ranked by aligned points (the
earlier whole-dev figures' way, DISC21/score_shards.py) and by aligned points
+ correlation / 2 (validate.py's); at the run's bar and 0.7 read off it."""
import csv, glob, json, os, re
H = os.path.dirname(os.path.abspath(__file__)); X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
ns = sorted(int(re.search(r'shard(\d+)\.json$', f).group(1)) for f in glob.glob(f'{H}/shard*.json'))
refs = set()
for n in ns:
    refs |= {l.strip() for l in open(f'{X}/refs{n}.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
best = {}
for n in ns:
    for p in json.load(open(f'{H}/shard{n}.json'))['pairs']:
        a, b = ident(p['a']), ident(p['b'])
        if {a[0], b[0]} != {'Q', 'R'}:
            continue
        k = (a, b) if a[0] == 'Q' else (b, a)
        v = (p['aligned_points'] or 0, p['pixel_correlation'])
        best[k] = max(best.get(k, v), v)
print(f'shards {len(ns)} ({ns[0]}-{ns[-1]}), {len(refs):,} references, {len(gt):,} true copies in reach, {len(best):,} pairs stated')
for name, rank in (('points', lambda v: v[0]), ('points + corr/2', lambda v: v[0] + v[1] / 2)):
    for lo in (0.0, 0.7):
        sel = sorted(((rank(v), k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
        tp = ap = r90 = 0
        for i, (_, t) in enumerate(sel, 1):
            if t:
                tp += 1; ap += tp / i
            if tp / i >= 0.9:
                r90 = max(r90, tp / len(gt))
        print(f'  {name:16s} {"run bar" if lo == 0 else "bar 0.7":8s}: true {tp:5d} false {len(sel) - tp:6d}  uAP {100 * ap / len(gt):5.2f}  recall {100 * tp / len(gt):5.2f}  R@P90 {100 * r90:5.2f}')
