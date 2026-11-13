#!/usr/bin/env python3
"""pr.py MANIFEST REPORT...: the keep rule as a classifier of redundant files, against the generated truth.

A file is redundant when another file holds all of it (`suggest_score.holds`); of files that hold
each other one must stay. So the most that can be deleted is every file but one of each set of
files nothing outside it holds (the source components of the holds graph). Precision: deletions a
surviving file holds, of all deletions. Recall: those, of the most that could be deleted.
`strict`: a composite or a mirrored / turned copy holds nothing (as `suggest_score`'s strict line)."""
import json, csv, sys, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
sys.setrecursionlimit(100000)
man = {r['path']: r for r in csv.DictReader(open(sys.argv[1]))}
G = ss.build({p: {'path': p} for p in man}, man)
seeds = collections.defaultdict(list)
for p, g in G.items(): seeds[g['seed']].append(p)
def H(k, d, strict): return k != d and ss.holds(G[k], G[d]) and not (strict and (G[k]['host'] or G[k]['var'] in ss.GEOM))
def most(strict):
    """Files that can go at most, and the files no other holds."""
    can = 0; alone = set()
    for s, ps in seeds.items():
        held_by = {d: [k for k in ps if H(k, d, strict)] for d in ps}
        # components of mutual holding (Tarjan), then those nothing outside holds
        idx = {}; low = {}; st = []; on = set(); comp = {}; n = [0]
        def visit(v):
            idx[v] = low[v] = n[0]; n[0] += 1; st.append(v); on.add(v)
            for w in held_by[v]:
                if w not in idx: visit(w); low[v] = min(low[v], low[w])
                elif w in on: low[v] = min(low[v], idx[w])
            if low[v] == idx[v]:
                while True:
                    w = st.pop(); on.discard(w); comp[w] = v
                    if w == v: break
        for v in ps:
            if v not in idx: visit(v)
        src = {comp[v] for v in ps} - {comp[d] for d in ps for k in held_by[d] if comp[k] != comp[d]}
        can += len(ps) - len(src)
        alone |= {d for d in ps if not held_by[d]}
    return can, alone
for strict in (False, True):
    can, alone = most(strict)
    print(f"\n{'strict' if strict else 'as scored'}: {len(G)} files, {can} can be deleted at most, {len(alone)} hold something no other file does")
    for rep in sys.argv[2:]:
        act = {}
        for g in json.load(open(rep))['groups']:
            for f in g['files']:
                if f['path'] in G: act[f['path']] = f.get('action')
        surv = collections.defaultdict(list)
        for p in G:
            if act.get(p) != 'DELETE': surv[G[p]['seed']].append(p)
        dels = [p for p, a in act.items() if a == 'DELETE']
        right = sum(any(H(k, d, strict) for k in surv[G[d]['seed']]) for d in dels)
        excl = sum(d in alone for d in dels)
        print(f"  {rep.split('/')[-2]:5s} DELETE {len(dels):6d}  right {right:6d}  wrong {len(dels)-right:3d} (of them exclusive files {excl})  precision {100*right/len(dels):.3f}%  recall {100*right/can:.2f}%")
