#!/usr/bin/env python3
"""dsep.py CORPUS DUMP...: how the candidate measures separate true pairs from sibling photographs, on direct verdicts."""
import sys, csv, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import families as fm
corpus = sys.argv[1]
rows = []
_, MAN = fm.families(corpus)
for d in sys.argv[2:]:
    R = list(csv.DictReader(open(d, errors='replace')))
    files = sorted({r['a'] for r in R} | {r['b'] for r in R})
    man, rel = MAN, fm.truth(corpus, set(files))
    for r in R:
        if r['kind'] != 'direct' or r['inverted'] != '0': continue
        a, b = r['a'], r['b']
        if a not in man or b not in man: continue
        sa, sb = man[a][0], man[b][0]
        if sa == sb: lab = rel.get(fm.key(a, b), '?')
        else: lab = 'SIBLING' if fm.group_of(sa) == fm.group_of(sb) else 'UNRELATED'
        rows.append((lab, int(r['n_in']), float(r['blk']), float(r['blk_min']), float(r['x0']), float(r['x1']), float(r['x2']), float(r['x3']), man[a][1], man[b][1], max(float(r['ov_a']), float(r['ov_b']))))
def pct(xs, q): xs = sorted(xs); return xs[min(len(xs) - 1, int(q * len(xs)))] if xs else float('nan')
def show(title, sel):
    print(f"\n{title}\n{'label':10s} {'n':>6s} | {'blk p10 p50':>12s} | {'blk_min p10 p50':>15s} | {'res p50: p10 p50 p90':>20s} | {'res p90: p50 p90':>15s} | {'blk96 p10 p50':>13s} | {'min96 p10 p50 p90':>18s}")
    by = collections.defaultdict(list)
    for r in rows:
        if sel(r): by[r[0]].append(r)
    for lab, rs in sorted(by.items(), key=lambda kv: -len(kv[1])):
        c = lambda i: [r[i] for r in rs]
        print(f"{lab:10s} {len(rs):6d} | {pct(c(2),.1):5.2f} {pct(c(2),.5):5.2f} | {pct(c(3),.1):6.2f} {pct(c(3),.5):6.2f} | {pct(c(4),.1):6.2f} {pct(c(4),.5):6.2f} {pct(c(4),.9):6.2f} | {pct(c(5),.5):6.2f} {pct(c(5),.9):6.2f} | {pct(c(6),.1):5.2f} {pct(c(6),.5):5.2f} | {pct(c(7),.1):5.2f} {pct(c(7),.5):5.2f} {pct(c(7),.9):5.2f}")
    return by
show('every direct verdict with a pixel check', lambda r: r[2] > 0)
by = show('anchors at the defaults (10 points, overlap 0.85, correlation 0.7)', lambda r: r[1] >= 10 and r[10] >= 0.85 and r[2] >= 0.7)
show('clean anchors (worst block 0.9)', lambda r: r[1] >= 10 and r[10] >= 0.85 and r[2] >= 0.7 and r[3] >= 0.9)
import json
json.dump(rows, open(os.path.join(os.path.dirname(sys.argv[2]), 'rows-' + os.path.basename(corpus) + '.json'), 'w'))
