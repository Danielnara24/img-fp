#!/usr/bin/env python3
"""classify.py: where shard 0's missed true copies (bar 0.5, 512; missed.txt)
are lost. Full scan: is the pair in the --dump (verified at all; the dump keeps
verdicts of >= 3 points and >= 0.2 correlation) and, if so, which bar its best
verdict fails. Alone: the two files scanned by themselves, stated or not.
Bars read here: points 5, correlation (blk) 0.5, overlap 0.85 (larger side),
inliers around the centre; approximate, the run's rules (slack, clusters) are not replayed."""
import csv, json, os, sys
from collections import Counter
H = os.path.dirname(os.path.abspath(__file__))
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
miss = [tuple(l.split()) for l in open(f'{H}/missed.txt')]
want = set(miss)

def rows(f, keep=None):
    best = {}
    with open(f) as fh:
        for r in csv.DictReader(fh):
            a, b = ident(r['a']), ident(r['b'])
            k = (a, b) if a[0] == 'Q' else (b, a)
            if keep is not None and k not in keep:
                continue
            v = (int(r['n_in']), float(r['blk']), max(float(r['ov_a']), float(r['ov_b'])), r['centred'] == '1')
            if k not in best or v[:2] > best[k][:2]:
                best[k] = v
    return best

def why(v):
    if v is None:
        return 'not verified'
    n, c, o, cen = v
    fails = [w for w, bad in (('points', n < 5), ('correlation', c < 0.5), ('overlap', o < 0.85), ('centre', not cen)) if bad]
    return 'fails ' + '+'.join(fails) if fails else 'passes the bars, not stated'

full = rows(f'{H}/dump.csv', want)
out = []
for q, r in miss:
    d = f'{H}/alone/{q}'
    stated = bool(json.load(open(f'{d}/out.json')).get('pairs')) if os.path.exists(f'{d}/out.json') else None
    al = rows(f'{d}/dump.csv').get((q, r)) if os.path.exists(f'{d}/dump.csv') else None
    out.append((q, r, why(full.get((q, r))), full.get((q, r)), stated, al))

c = Counter(x[2] for x in out)
ca = Counter(x[2] for x in out if x[4])
print(f'{len(miss)} missed true copies; full scan  ->  of which stated when scanned alone')
for k, n in c.most_common():
    print(f'  {k:40s} {n:4d}  {ca.get(k, 0):4d}')
print(f'stated alone: {sum(bool(x[4]) for x in out)}; alone, no verdict of 3 points at all: {sum(x[5] is None for x in out)}')
if '-v' in sys.argv:
    for x in out:
        print(*x)
