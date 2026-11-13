#!/usr/bin/env python3
"""sides.py DUMP MANIFEST: the whole-frame residual of every strong pair, fitted and frame onto frame, by what the pair is."""
import sys, csv, json, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
man = {r['path']: r for r in csv.DictReader(open(sys.argv[2]))}
def kind(r):
    v = r['variant']; reg = json.loads(r['region']); full = reg == [0.0, 0.0, 1.0, 1.0]
    if r['scramble']: return 'trap', 0
    if r['geometry'] in ('perspective', 'rotate'): return 'warp', 0
    if r['geometry'] == 'mirror': return 'mirror', 0
    if float(r['coverage']) < 0.999: return 'host', 0
    if not full: return 'crop', max(reg[0], reg[1], 1 - reg[2], 1 - reg[3])
    return ('edit' if v in ss.EDITED else 'faith'), 0
idx = {}; S = collections.defaultdict(list)
for l in open(sys.argv[1], errors='replace'):
    if l.startswith('IDX '): i, p = l[4:].rstrip('\n').split(' ', 1); idx[int(i)] = p
for l in open(sys.argv[1], errors='replace'):
    if l.startswith('SIDE '):
        w = l.split(); f, g = idx[int(w[1])], idx[int(w[2])]
        if f not in man or g not in man: continue
        v = [float(x) for x in w[3:]]
        (kf, cf), (kg, cg) = kind(man[f]), kind(man[g])
        if man[f]['seed'] != man[g]['seed']: cat = 'other seed'
        else:
            ks = tuple(sorted((kf, kg)))
            if ks == ('crop', 'faith'):
                c = max(cf, cg); cat = 'faith x crop <1%' if c < .01 else 'faith x crop 1-2%' if c < .02 else 'faith x crop 2-5%' if c < .05 else 'faith x crop 5-10%' if c < .1 else 'faith x crop >10%'
            else: cat = ' x '.join(ks)
        pts, n_in, blk, blkmin, sx, sy, a_by, b_by, wf1, wf2, we1, we2 = v
        S[cat].append((max(wf1, wf2), max(we1, we2), min(we1, we2), max(a_by, b_by) * pts, pts, f, g, man[f]['variant'], man[g]['variant']))
def pct(xs, q): xs = sorted(xs); return xs[min(len(xs) - 1, int(q * len(xs)))]
print(f"{'pair':24s} {'n':>7s} | fitted p50 p90 p99 | exact p10 p50 p90 p99 | corner*pts p50 p99 | exact<=12 fitted<=12")
for cat in sorted(S, key=lambda c: -len(S[c])):
    r = S[cat]; wf = [x[0] for x in r]; we = [x[1] for x in r]; cp = [x[3] for x in r]
    print(f"{cat:24s} {len(r):7d} | {pct(wf,.5):6.1f} {pct(wf,.9):6.1f} {pct(wf,.99):6.1f} | {pct(we,.1):6.1f} {pct(we,.5):6.1f} {pct(we,.9):6.1f} {pct(we,.99):6.1f} | {pct(cp,.5):5.2f} {pct(cp,.99):6.2f} | {sum(x<=12 for x in we)/len(r):.3f} {sum(x<=12 for x in wf)/len(r):.3f}")
if len(sys.argv) > 3:
    for cat in sys.argv[3:]:
        print('\n' + cat)
        bv = collections.defaultdict(list)
        for x in S[cat]: bv[tuple(sorted((x[7], x[8])))].append(x[1])
        for k, v in sorted(bv.items(), key=lambda kv: -pct(kv[1], .9))[:25]: print(f"   {str(k):50s} n={len(v):4d} exact p50 {pct(v,.5):6.1f} p90 {pct(v,.9):6.1f} p99 {pct(v,.99):6.1f}")
