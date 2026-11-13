#!/usr/bin/env python3
"""givenup.py BASE NEW MANIFEST: of the deletions the new rule gives up, which the truth says were right, and on what."""
import json, csv, sys, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
man = {r['path']: r for r in csv.DictReader(open(sys.argv[3]))}
G = ss.build({p: {'path': p} for p in man}, man)
def acts(f):
    a = {}
    for g in json.load(open(f))['groups']:
        for x in g['files']:
            if x['path'] in G: a[x['path']] = x.get('action')
    return a
b, n = acts(sys.argv[1]), acts(sys.argv[2])
stay = collections.defaultdict(list)   # what stayed at HEAD, by seed
for p, a in b.items():
    if a != 'DELETE': stay[G[p]['seed']].append(p)
c = collections.Counter(); ex = collections.defaultdict(collections.Counter)
for p, a in b.items():
    if a != 'DELETE' or n.get(p) == 'DELETE': continue
    d = G[p]; hs = [k for k in stay[d['seed']] if ss.holds(G[k], d)]
    plain = [k for k in hs if not G[k]['host'] and G[k]['var'] not in ss.GEOM]
    same_kind = [k for k in plain if G[k]['edited'] == d['edited']]
    if not hs: w = 'wrong: nothing that stayed held it'
    elif not plain: w = 'held only by a composite or a turned copy'
    elif d['edited']: w = 'right: an edit, the plain picture stayed'
    else: w = 'right: an unedited copy, an unedited holder stayed'
    c[w] += 1; ex[w][d['var']] += 1
tot = sum(c.values())
for w, k in c.most_common(): print(f"{k:5d} ({100*k/tot:4.1f}%) {w}   {ex[w].most_common(6)}")
