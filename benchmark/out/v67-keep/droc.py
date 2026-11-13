#!/usr/bin/env python3
"""droc.py ROWS.json...: siblings passed at equal shares of true pairs kept, for each candidate measure as the bar."""
import json, sys
rows = [r for f in sys.argv[1:] for r in json.load(open(f))]
cand = [r for r in rows if r[1] >= 10 and r[10] >= 0.85 and r[2] > 0]
pos = [r for r in cand if r[0] in ('SAME', 'CROP')]; sib = [r for r in cand if r[0] == 'SIBLING']
print(f"candidates past points and overlap: true {len(pos)}, sibling {len(sib)}; at the default bar (blk >= 0.7): true {sum(r[2]>=.7 for r in pos)}, sibling {sum(r[2]>=.7 for r in sib)}")
M = {'blk (48 grid)': lambda r: r[2], 'blk96': lambda r: r[6], 'min(blk, blk96)': lambda r: min(r[2], r[6]),
     '-residual median': lambda r: -r[4], '-residual p90': lambda r: -r[5],
     'blk96 - residual/2': lambda r: r[6] - r[4] / 2, 'blk - residual/2': lambda r: r[2] - r[4] / 2,
     'worst block 48': lambda r: r[3], 'worst block 96': lambda r: r[7]}
keep = [0.99, 0.97, 0.95, 0.92, 0.90, 0.85, 0.80]
print(f"{'measure':22s}" + ''.join(f"  keep {k:.0%}" for k in keep) + '   (siblings passing, and the bar)')
for name, f in M.items():
    ps = sorted((f(r) for r in pos), reverse=True); out = f"{name:22s}"
    for k in keep:
        t = ps[min(len(ps) - 1, int(k * len(ps)) - 1)]
        out += f"  {sum(f(r) >= t for r in sib):4d}@{t:5.2f}"
    print(out)
