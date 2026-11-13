#!/usr/bin/env python3
"""diff2.py BASE NEW MANIFEST: deletions given up and gained, edits apart from unedited versions, and what the given-up became."""
import json, csv, sys, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
man = {r['path']: r for r in csv.DictReader(open(sys.argv[3]))}
def acts(f):
    a = {}
    for g in json.load(open(f))['groups']:
        for x in g['files']: a[x['path']] = x.get('action')
    return a
b, n = acts(sys.argv[1]), acts(sys.argv[2])
kind = lambda v: 'trap' if v in ss.TRAPS else 'composite' if v in ss.HOSTS else 'edit' if v in ss.EDITED else 'unedited'
lost = collections.Counter(); gain = collections.Counter(); to = collections.Counter(); tot = collections.Counter()
for p in b:
    if p not in man: continue
    k = kind(man[p]['variant'])
    if b[p] == 'DELETE': tot[k] += 1
    if b[p] == 'DELETE' and n.get(p) != 'DELETE': lost[k] += 1; to[(k, n.get(p))] += 1
    if b[p] != 'DELETE' and n.get(p) == 'DELETE': gain[k] += 1
for k in ['unedited', 'edit', 'composite', 'trap']:
    print(f"{k:10s} deleted before {tot[k]:6d}  given up {lost[k]:5d} ({100*lost[k]/max(tot[k],1):.1f}%: REVIEW {to[(k,'REVIEW')]}, KEEP {to[(k,'KEEP')]})  gained {gain[k]:5d}  net {gain[k]-lost[k]:+d}")
