#!/usr/bin/env python3
"""noupscale.py REPORT MANIFEST: deletions held only by an `upscale` (which IMGS-ALL2 makes from a small copy; the
scorer reads it as full detail)."""
import json, csv, sys, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
man = {r['path']: r for r in csv.DictReader(open(sys.argv[2]))}
G = ss.build({p: {'path': p} for p in man}, man)
act = {}
for g in json.load(open(sys.argv[1]))['groups']:
    for f in g['files']:
        if f['path'] in G: act[f['path']] = f.get('action')
by = collections.defaultdict(list)
for p, a in act.items():
    if a != 'DELETE': by[G[p]['seed']].append(p)
n = collections.Counter()
for p, a in act.items():
    if a != 'DELETE': continue
    d = G[p]; hs = [k for k in by[d['seed']] if ss.holds(G[k], d)]
    if hs and all(G[k]['var'] == 'upscale' for k in hs): n[d['var']] += 1
print(sys.argv[1], 'deletions held only by an upscale:', sum(n.values()), n.most_common(8))
