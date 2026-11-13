#!/usr/bin/env python3
"""lostwhy.py BASE.json NEW.json REFDUMP MANIFEST: for each deletion the new rule gives up, why the best-placed kept file refused it."""
import json, csv, sys, os, collections, re
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
man = {r['path']: r for r in csv.DictReader(open(sys.argv[4]))}
G = ss.build({p: {'path': p} for p in man}, man)
def acts(f):
    a = {}
    for g in json.load(open(f))['groups']:
        for x in g['files']: a[x['path']] = x.get('action')
    return a
b, n = acts(sys.argv[1]), acts(sys.argv[2])
idx = {}; ref = collections.defaultdict(list)
for l in open(sys.argv[3], errors='replace'):
    if l.startswith('IDX '): i, p = l[4:].rstrip('\n').split(' ', 1); idx[int(i)] = p
for l in open(sys.argv[3], errors='replace'):
    if l.startswith('REF '):
        y, x, rest = l.split(' ', 3)[1:]
        kv = dict(t.split('=') for t in rest.split()[1:]); kv['r'] = rest.split()[0]
        ref[idx[int(y)]].append((idx[int(x)], kv))
why = collections.Counter(); ex = collections.defaultdict(collections.Counter)
for p in b:
    if p not in man or b[p] != 'DELETE' or n.get(p) == 'DELETE': continue
    d = G[p]
    # kept files that truly hold it, by the ground truth
    holders = [(k, kv) for k, kv in ref[p] if k in G and ss.holds(G[k], d) and not G[k]['host'] and G[k]['var'] not in ss.GEOM]
    if not holders: w = 'no kept true holder is paired with it'
    else:
        def reason(kv):
            if kv['r'] != 'Some(No)': return kv['r']
            if kv['holds'] == 'false': return 'holds: frame' if kv['f_in'] == 'false' else 'holds: detail (extra)'
            if kv['same'] == 'false' and kv['strict_in'] == 'false': return 'neither one frame nor strictly inside' + (' (corner limbo%s)' % (', made' if 'made' in kv else '') if kv['past_f'] == 'false' and kv['in_f'] == 'false' else ' (pixels)')
            if float(kv['whole']) > 12: return 'whole-frame residual'
            return 'composite rule'
        rs = [reason(kv) for k, kv in holders]
        order = ['Some(Undecided)', 'neither one frame nor strictly inside (corner limbo, made)', 'Some(Held)', 'Some(Softer)', 'whole-frame residual', 'neither one frame nor strictly inside (pixels)', 'neither one frame nor strictly inside (corner limbo)', 'holds: detail (extra)', 'holds: frame', 'composite rule']
        w = min(rs, key=lambda r: order.index(r) if r in order else 99)
    why[w] += 1; ex[w][('edit ' if d['edited'] else '') + d['var']] += 1
for w, c in why.most_common(): print(f"{c:5d} {w}\n        {ex[w].most_common(12)}")
