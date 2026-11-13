#!/usr/bin/env python3
"""diff.py BASE.json NEW.json MANIFEST: deletions gained and lost, by variant, and whether each was held (suggest_score)."""
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
lost = collections.Counter(); gained = collections.Counter(); to = collections.Counter()
for p in b:
    if p not in man: continue
    if b[p] == 'DELETE' and n.get(p) != 'DELETE': lost[man[p]['variant']] += 1; to[n.get(p)] += 1
    if b[p] != 'DELETE' and n.get(p) == 'DELETE': gained[man[p]['variant']] += 1
print('deletions lost', sum(lost.values()), 'became', dict(to), 'gained', sum(gained.values()))
print(' lost  :', lost.most_common(100))
print(' gained:', gained.most_common(30))
