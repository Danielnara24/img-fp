#!/usr/bin/env python3
"""agree.py: verification's first step (leaf seeds, best transform's agreeing
count) as a ranking of the first 5,000 candidates: for the missed copies, the
true pair's agreeing count against the null (the most among the first k), and
how many candidates past k beat that null (what verifying them would cost)."""
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
    ds = []
    for part in l.split('|')[1:]:
        rank = int(re.search(r'rank=(-?\d+)', part).group(1))
        a = dict((k, float(v)) for k, v in re.findall(r'(\w+)=([\d.]+)', re.search(r'AG (.*?) up0', part).group(1)))
        ds.append((rank, a))
    rows.append((q, alone.get(q, False), ds))
us = [d[1]['us'] for r in rows for d in r[2]]
print(f'per candidate: median {median(us):.1f} us; candidates past k beating the null, median per query: '
      f'to 1,000 {median(d[1]["above1k"] for r in rows for d in r[2][:1]):.0f}, to 5,000 {median(d[1]["above5k"] for r in rows for d in r[2][:1]):.0f}')
for title, sel in (('stated alone', [r for r in rows if r[1]]), ('not stated alone', [r for r in rows if not r[1]])):
    print(f'\n{title}: {len(sel)}')
    def found(r, depth):
        return any(0 <= rank < depth and (rank < 150 or (a['mine'] > a['null'] and a['mine'] >= 3)) for rank, a in r[2])
    for depth in (150, 1000, 5000):
        print(f'  within {depth}: ranked {sum(any(0 <= rank < depth for rank, _ in r[2]) for r in sel)}, reached (rank < 150 or agreeing > null) {sum(found(r, depth) for r in sel)}')
    print('  pair    rank q->r  mine null r5k | rank r->q  mine null r5k')
    for q, _, ds in sorted(sel, key=lambda r: min(x[0] if x[0] >= 0 else 10**9 for x in r[2])):
        if min(x[0] if x[0] >= 0 else 10**9 for x in ds) >= 5000:
            continue
        print(f'  {q} ' + ' | '.join(f'{rank:8d} {int(a["mine"]):5d} {int(a["null"]):4d} {int(a["r5k"]):4d}' for rank, a in ds))
