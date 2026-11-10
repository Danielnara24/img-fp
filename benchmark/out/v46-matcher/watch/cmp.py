import sys, re
runs = sys.argv[1:]; sys.argv=['x']
exec(open('watch/cross.py').read().split("for t in ('all', 'i5'):")[0])
def sc(rs):
    v=[(float(re.search(r'score ([\d.]+)', '\t'.join(x)).group(1)), int(x[1])) for x in rs if x[1].isdigit()]
    return max(v) if v else None
F = {r: fate(r) for r in runs}
base = F[runs[0]]
keys = [k for k in base if base[k][0] in ('stated', 'claim graph: stands, not stated')]
print(f"{'pair':36}" + ''.join(f'{r:>26}' for r in runs))
for k in keys:
    row = f"{k[0].split('/')[-1][:17]:17} {k[1].split('/')[-1][:17]:17} "
    for r in runs:
        s = sc(F[r][k][3]); w = F[r][k][0]
        tag = {'stated':'S','claim graph: stands, not stated':'G','verdict: rejected':'V'}.get(w, 'R')
        row += f"{(f'{s[0]:.4f} #{s[1]}' if s else '-'):>22} {tag:>3}"
    print(row)
import collections
for r in runs:
    print(r, dict(collections.Counter(F[r][k][0] for k in keys)))
