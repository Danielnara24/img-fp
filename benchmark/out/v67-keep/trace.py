#!/usr/bin/env python3
"""Every deletion that loses content in a report, with the chain of files it rests on.
trace.py REPORT.json MANIFEST.csv"""
import json, csv, sys, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
rep = json.load(open(sys.argv[1])); man = {r['path']: r for r in csv.DictReader(open(sys.argv[2]))}
G = ss.build({p: {'path': p} for p in man}, man)
rows = {}
for g in rep['groups']:
    for f in g['files']: rows.setdefault(f['path'], f)
act = {p: f.get('action') for p, f in rows.items() if p in G}
lo, ex = ss.losses(act, G, None)
by_seed = collections.defaultdict(list)
for p, a in act.items():
    if a != 'DELETE': by_seed[G[p]['seed']].append(p)
pairs = collections.defaultdict(dict)
for pr in rep.get('pairs', []):
    pairs[pr['a']][pr['b']] = pr; pairs[pr['b']][pr['a']] = pr
def short(p): return f"{man[p]['variant']}[{man[p]['seed']}] {man[p]['width']}x{man[p]['height']} {os.path.basename(p)}"
for p, a in act.items():
    if a != 'DELETE': continue
    d = G[p]
    if any(ss.holds(G[k], d) for k in by_seed[d['seed']]): continue
    print('LOST', short(p), 'region', man[p]['region'])
    print('  kept_copy:', rows[p].get('kept_copy') and short(rows[p]['kept_copy']), act.get(rows[p].get('kept_copy')))
    for q, pr in sorted(pairs[p].items(), key=lambda kv: -kv[1].get('pixel_correlation', 0))[:40]:
        print('   pair', act.get(q), short(q), {k: v for k, v in pr.items() if k not in ('a', 'b', 'a_bytes', 'b_bytes')})
