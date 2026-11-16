#!/usr/bin/env python3
"""oracle.py [oracle.err]: for the missed copies stated alone, where the true
reference ranks when files are scored by the query keypoints that have a
descriptor within T in the same cell at level <= L of the tree (ORACLE lines),
and what verification gives from those seeds."""
import re, sys
H = '/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out'
alone = {}
for l in open(f'{H}/v73-disc-all/why/classify.out'):
    m = re.match(r'^(Q\d+) (R\d+) (.*?) (None|\(.*?\)) (True|False|None) (None|\(.*\))$', l.strip())
    if m:
        alone[m.group(1)] = m.group(5) == 'True'
rows = []
for l in open(sys.argv[1] if len(sys.argv) > 1 else f'{H}/v74-reach/oracle.err'):
    if not l.startswith('ORACLE Q'):
        continue
    w = l.split()
    q = w[1]
    sc = {k: tuple(int(x) for x in v.split('/')) for k, v in (x.split('=') for x in w[3:] if x[0] == 'L')}
    ver = {k[1:]: tuple(float(x) for x in v.split('/')) for k, v in (x.split('=') for x in w[3:] if x[0] == 'V')}
    rows.append((q, sc, ver))
for title, sel in (('stated alone', [r for r in rows if alone.get(r[0])]), ('not stated alone', [r for r in rows if not alone.get(r[0])])):
    print(f'\n{title}: {len(sel)}')
    print('  score       true(med)  top(med)  first  <10  <150  <1000   (rank = files above + ties - 1)')
    for k in sel[0][1]:
        rk = [s[k][1] + max(s[k][2] - 1, 0) if s[k][0] > 0 else 10**9 for _, s, _ in sel]
        tc = sorted(s[k][0] for _, s, _ in sel)
        tp = sorted(s[k][3] for _, s, _ in sel)
        print(f'  {k:10s} {tc[len(tc)//2]:8d} {tp[len(tp)//2]:9d} {sum(r == 0 for r in rk):6d} {sum(r < 10 for r in rk):4d} {sum(r < 150 for r in rk):5d} {sum(r < 1000 for r in rk):6d}')
    print('  seeds       seeds(med)  passes (>= 5 points, correlation >= 0.5, overlap >= 0.5)')
    for k in sel[0][2]:
        ok = sum(v[k][1] >= 5 and v[k][2] >= 0.5 and v[k][3] >= 0.5 for _, _, v in sel)
        sd = sorted(v[k][0] for _, _, v in sel)
        print(f'  {k:10s} {int(sd[len(sd)//2]):8d} {ok:6d}')
if '-v' in sys.argv:
    for q, s, v in rows:
        if alone.get(q):
            print(q, ' '.join(f'{k}={"/".join(map(str, x))}' for k, x in s.items() if k.endswith('T320')), ' '.join(f'{k}={x[1]:.0f}/{x[2]:.2f}' for k, x in v.items() if '320' in k))
