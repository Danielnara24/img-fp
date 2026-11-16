#!/usr/bin/env python3
"""verdicts.py: per missed pair (trace.err), the verdict verification gives from
each level's seeds (q->r), against the pair scanned alone (classify.out);
counts of verdicts that would pass the run's bars (points 5, correlation 0.5,
overlap 0.85) by level, split by ranked < 150 / < 1000 either way."""
import re, sys
H = '/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out'
alone = {}
for l in open(f'{H}/v73-disc-all/why/classify.out'):
    m = re.match(r'^(Q\d+) (R\d+) (.*?) (None|\(.*?\)) (True|False|None) (None|\(.*\))$', l.strip())
    if m:
        alone[m.group(1)] = (m.group(5) == 'True', m.group(6))
rows = []
for l in open(sys.argv[1] if len(sys.argv) > 1 else 'trace.err'):
    if not l.startswith('TRACE Q'):
        continue
    q = l.split()[1]
    dirs = []
    for part in l.split('|')[1:]:
        d = {'rank': int(re.search(r'rank=(-?\d+)', part).group(1))}
        for u, body in re.findall(r'up(\d) (seeds=.*?vmatch=\d+)', part):
            d[int(u)] = {k: float(v) for k, v in re.findall(r'(\w+)=([\d.]+)', body)}
        dirs.append(d)
    rows.append((q, alone.get(q, (False, None)), dirs))
ok = lambda v: v['vin'] >= 5 and v['vblk'] >= 0.5 and v['vov'] >= 0.85
rk = lambda r: min(x if x >= 0 else 10**9 for x in (r[2][0]['rank'], r[2][1]['rank']))
for title, sel in (('stated alone', [r for r in rows if r[1][0]]), ('not stated alone', [r for r in rows if not r[1][0]])):
    print(f'\n{title}: {len(sel)}')
    for lim in (150, 1000, 10**10):
        s = [r for r in sel if rk(r) < lim]
        print(f'  ranked < {lim if lim < 10**10 else "any"}: {len(s)}; passing the bars from seeds at level  ' +
              '  '.join(f'up{u}: {sum(any(ok(r[2][d][u]) for d in (0, 1)) for r in s)}' for u in range(4)))
    print('  pair            rank  alone                        up0 (in blk ov)    up1               up2               up3')
    for r in sorted(sel, key=rk):
        if rk(r) >= 1000:
            continue
        f = lambda v: f'{int(v["vin"]):3d} {v["vblk"]:.2f} {v["vov"]:.2f}'
        best = lambda u: max((r[2][d][u] for d in (0, 1)), key=lambda v: (ok(v), v['vin']))
        print(f'  {r[0]} {rk(r):6d}  {str(r[1][1]):28s} ' + '   '.join(f(best(u)) for u in range(4)))
