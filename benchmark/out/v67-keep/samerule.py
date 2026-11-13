#!/usr/bin/env python3
"""samerule.py DUMP MANIFEST: candidate definitions of "one frame" against the generated truth."""
import sys, csv, json, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
man = {r['path']: r for r in csv.DictReader(open(sys.argv[2]))}
def frame(r):
    """The file's frame, as far as the truth says: the seed's region it shows, and whether it shows it plainly."""
    reg = tuple(json.loads(r['region']))
    plain = not r['scramble'] and r['geometry'] in ('none', 'rot90', 'rot180', 'rot270') and float(r['coverage']) >= 0.999
    return reg, plain
idx = {}
for l in open(sys.argv[1], errors='replace'):
    if l.startswith('IDX '): i, p = l[4:].rstrip('\n').split(' ', 1); idx[int(i)] = p
R = 12.0
rules = {
  'corners within FIT_ERROR (HEAD)':            lambda a, b, p, wf, we: max(a, b) * p <= 0.7,
  'pixels, corners within FIT_ERROR_MOST':      lambda a, b, p, wf, we: max(a, b) * p <= 4.6 and we <= R,
  'pixels and corners within FIT_ERROR':        lambda a, b, p, wf, we: max(a, b) * p <= 0.7 and we <= R,
  'pixels; past FIT_ERROR only if exact<=fit':  lambda a, b, p, wf, we: we <= R and (max(a, b) * p <= 0.7 or (max(a, b) * p <= 4.6 and we <= wf)),
  'pixels, exact<=fitted, within MOST':         lambda a, b, p, wf, we: max(a, b) * p <= 4.6 and we <= R and we <= wf,
}
tot = collections.Counter(); acc = {k: collections.Counter() for k in rules}; ex = {k: collections.Counter() for k in rules}
for l in open(sys.argv[1], errors='replace'):
    if not l.startswith('SIDE '): continue
    w = l.split(); f, g = idx[int(w[1])], idx[int(w[2])]
    if f not in man or g not in man: continue
    pts, n_in, blk, blkmin, sx, sy, a_by, b_by, wf1, wf2, we1, we2 = [float(x) for x in w[3:]]
    (rf, pf), (rg, pg) = frame(man[f]), frame(man[g])
    if man[f]['seed'] != man[g]['seed']: lab = 'other seed'
    elif pf and pg and rf == rg: lab = 'one frame'
    elif pf and pg: 
        d = max(abs(x - y) for x, y in zip(rf, rg)); lab = 'frames differ <2%' if d < .02 else 'frames differ 2-5%' if d < .05 else 'frames differ >5%'
    else: lab = 'warp, composite or rearranged'
    tot[lab] += 1
    for k, rule in rules.items():
        if rule(a_by, b_by, pts, max(wf1, wf2), max(we1, we2)): acc[k][lab] += 1; ex[k][(lab, tuple(sorted((man[f]['variant'], man[g]['variant']))))] += 1
labs = ['one frame', 'frames differ <2%', 'frames differ 2-5%', 'frames differ >5%', 'warp, composite or rearranged', 'other seed']
print(f"{'':44s}" + ''.join(f"{l[:18]:>20s}" for l in labs)); print(f"{'pairs':44s}" + ''.join(f"{tot[l]:20d}" for l in labs))
for k in rules: print(f"{k:44s}" + ''.join(f"{acc[k][l]:20d}" for l in labs))
for k in rules:
    bad = [(v, c) for (lab, v), c in ex[k].items() if lab in ('frames differ 2-5%', 'frames differ >5%', 'warp, composite or rearranged', 'other seed')]
    print('\n' + k, sorted(bad, key=lambda x: -x[1])[:10])
