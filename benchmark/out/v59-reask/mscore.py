"""mscore.py REPORT...: MIRFAM, per seed: turned pairs (o-side to m-side, of 9) stated,
seeds with all 9, with none; same-side pairs; pairs across seeds."""
import csv, json, sys
t = {r['path']: (r['seed'], r['side'], r['turn']) for r in csv.DictReader(open('/home/daniel/Documents/MIRFAM-truth.csv'))}
for f in sys.argv[1:]:
    d = json.load(open(f)); cross = {}; same = other = 0
    for p in d['pairs']:
        a, b = t[p['a']], t[p['b']]
        if a[0] != b[0]: other += 1
        elif a[1] == b[1]: same += 1
        else: cross[a[0]] = cross.get(a[0], 0) + 1
    seeds = {v[0] for v in t.values()}
    full = sum(1 for s in seeds if cross.get(s, 0) == 9); none = sum(1 for s in seeds if s not in cross)
    print(f'{f:28s} turned {sum(cross.values()):5d}/{9*len(seeds)}  seeds all {full} none {none}  same-side {same}  across seeds {other}  {d["runtime_seconds"]:.0f}s')
