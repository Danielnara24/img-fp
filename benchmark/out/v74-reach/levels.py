#!/usr/bin/env python3
"""levels.py: the extended trace (trace.err) by vocabulary level: for the pairs
stated alone and the rest, seeds and mutual nearest seeds (q->r), and whether
the geometric peak at that level beats the null within the first 1,000 / 5,000
of the leaf ranking (either direction)."""
import re, sys
from statistics import median
H = '/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out'
alone = {}
for l in open(f'{H}/v73-disc-all/why/classify.out'):
    m = re.match(r'^(Q\d+) (R\d+) (.*?) (None|\(.*?\)) (True|False|None) (None|\(.*\))$', l.strip())
    if m:
        alone[m.group(1)] = m.group(5) == 'True'
rows = []
for l in open(sys.argv[1] if len(sys.argv) > 1 else 'trace.err'):
    if not l.startswith('TRACE Q'):
        continue
    q = l.split()[1]
    dirs = []
    for part in l.split('|')[1:]:
        d = {'rank': int(re.search(r'rank=(-?\d+)', part).group(1))}
        for u, body in re.findall(r'up(\d) (seeds=\S+ nn=\S+ peak=\S+ null=\S+ pr1k=\S+ pr5k=\S+)', part):
            d[int(u)] = {k: int(v) for k, v in re.findall(r'(\w+)=(\d+)', body)}
        dirs.append(d)
    rows.append((q, alone.get(q, False), dirs))

for title, sel in (('stated alone', [r for r in rows if r[1]]), ('not stated alone', [r for r in rows if not r[1]])):
    print(f'\n{title}: {len(sel)}')
    rk = lambda r: min(x if x >= 0 else 10**9 for x in (r[2][0]['rank'], r[2][1]['rank']))
    near = [r for r in sel if rk(r) < 150]
    print(f'  ranked < 150 either way: {len(near)}; < 1000: {sum(rk(r) < 1000 for r in sel)}; < 5000: {sum(rk(r) < 5000 for r in sel)}')
    print('  level  seeds(med)  nn(med)  nn>=3  nn>=3 among <150   geo beats null: <1k  <5k   (top of 1k / 5k by peak)')
    for u in range(4):
        qr = [r[2][0][u] for r in sel]
        nn3 = sum(x['nn'] >= 3 for x in qr)
        nn3n = sum(r[2][0][u]['nn'] >= 3 for r in near)
        beat = lambda d, cap: d['peak'] > d['null'] and d['peak'] >= 3 and (d['pr1k'] == 0 if cap == 1 else d['pr5k'] == 0)
        def reach(r, cap):
            return any(beat(r[2][s][u], cap) and 0 <= r[2][s]['rank'] < (1000 if cap == 1 else 5000) for s in (0, 1))
        def within(r, cap):
            # within the depth and above the null: what a geometric look at that depth would verify
            return any(r[2][s][u]['peak'] > r[2][s][u]['null'] and r[2][s][u]['peak'] >= 3 and 0 <= r[2][s]['rank'] < (1000 if cap == 1 else 5000) for s in (0, 1))
        print(f'  up{u}   {median(x["seeds"] for x in qr):8.0f}  {median(x["nn"] for x in qr):7.0f}  {nn3:5d}  {nn3n:5d}              {sum(within(r, 1) for r in sel):4d} {sum(within(r, 5) for r in sel):4d}   ({sum(reach(r, 1) for r in sel)} / {sum(reach(r, 5) for r in sel)})')
