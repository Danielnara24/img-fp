"""replay.py: from sweep.log (every candidate a lonely file scored, depth 3000, all verified),
what each (depth, budget) would add and cost. Gains: true Q-R pairs standing as an anchor
(at bar 0.5; at 0.7 also correlation >= 0.7) that base's report does not state at that bar."""
import csv, json, os, sys
from collections import defaultdict
X = '/home/daniel/Documents/DISC21'; K = 150
ident = lambda p: os.path.splitext(os.path.basename(p.strip()))[0]
names = [ident(l) for l in open('sweep.names')]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
def key(a, b):
    a, b = names[a], names[b]; return (a, b) if a[0] == 'Q' else (b, a)
base = {0.5: set(), 0.7: set()}
for p in json.load(open('disc-trace/report.json'))['pairs']:
    a, b = ident(p['a']), ident(p['b']); k = (a, b) if a[0] == 'Q' else (b, a)
    for bar in base:
        if p['pixel_correlation'] >= bar: base[bar].add(k)
askers = []  # (ranked_len, [(r, peak, st, corr, key)])
cur = None
for l in open('sweep.log'):
    f = l.split()
    if f[0] == 'A':
        cur = (int(f[2]), []); askers.append(cur); i = int(f[1]); continue
    j, r, peak, st, nin, blk = int(f[0]), int(f[1]), int(f[2]), int(f[3]), int(f[4]), float(f[5])
    cur[1].append((r, peak, st, blk, key(i, j)))
print(f'{len(askers)} askers, {sum(len(c) for _, c in askers)} candidates with peak >= 3')
depths = [300, 500, 750, 1000, 1500, 2000, 3000]
budgets = [1, 2, 3, 5, 10, 20, 50, 100, 10**6]
print('depth budget | scored verified | gain@0.5 gain@0.7 | false-anchors(Q-R, not true)')
rows = []
for M in depths:
    for W in budgets:
        scored = sum(max(0, min(n, M) - K) for n, _ in askers)
        ver = 0; g5 = set(); g7 = set(); fa = 0
        for n, c in askers:
            take = sorted((x for x in c if x[0] < M - K), key=lambda x: (-x[1], x[0]))[:W]
            ver += len(take)
            for r, peak, st, blk, k in take:
                if st >= 2 and {k[0][0], k[1][0]} == {'Q', 'R'}:
                    if k in gt:
                        if k not in base[0.5]: g5.add(k)
                        if blk >= 0.7 and k not in base[0.7]: g7.add(k)
                    else: fa += 1
        rows.append((M, W, scored, ver, len(g5), len(g7), fa))
        print(f'{M:5d} {W:7d} | {scored:9d} {ver:8d} | {len(g5):3d} {len(g7):3d} | {fa}')
json.dump(rows, open('replay.json', 'w'))
