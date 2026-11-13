#!/usr/bin/env python3
"""smallwhy.py DIR: each lost deletion in the small-family reports of DIR, with its family."""
import sys, os, csv, json, collections, glob
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
D = os.path.expanduser('~/Documents'); pat = collections.Counter()
for tag, corpus in (('all2', 'IMGS-ALL2'), ('all', 'IMGS-ALL')):
    man = {r['path']: r for r in csv.DictReader(open(f'{D}/{corpus}/derived/manifest.csv'))}
    G = ss.build({p: {'path': p} for p in man}, man)
    for f in sorted(glob.glob(f'{sys.argv[1]}/{tag}-n*.json')):
        rep = json.load(open(f)); rows = {}
        for g in rep['groups']:
            for x in g['files']: rows.setdefault(x['path'], x)
        act = {p: x.get('action') for p, x in rows.items() if p in G}
        fam = collections.defaultdict(list)
        for p in act: fam[G[p]['seed']].append(p)
        pr = {}
        for e in rep['pairs']: pr[(e['a'], e['b'])] = e; pr[(e['b'], e['a'])] = e
        for p, a in act.items():
            if a != 'DELETE' or any(ss.holds(G[k], G[p]) for k in fam[G[p]['seed']] if act[k] != 'DELETE'): continue
            k = rows[p].get('kept_copy'); e = pr.get((p, k), {})
            d = lambda q: f"{man[q]['variant']} {man[q]['width']}x{man[q]['height']} {act.get(q)}"
            others = [d(q) for q in fam[G[p]['seed']] if q != p]
            sz = int(man[k]['width']) / int(man[p]['width']) if k in man else 0
            pat[(man[p]['variant'], man[k]['variant'] if k in man else '?', 'smaller' if sz < 0.71 else 'same size' if sz < 1.41 else 'larger')] += 1
            if '-v' in sys.argv: print(f"{os.path.basename(f)[:-5]:12s} lost {d(p)} for {man[k]['variant'] if k in man else k} x{sz:.2f} pts {e.get('aligned_points')} corr {e.get('pixel_correlation')} | family: {others} | {man[p]['seed']}")
for k, v in pat.most_common(): print(v, k)
