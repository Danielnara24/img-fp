"""cmp.py A B: true DISC21 pairs, rank (best of both ends) and stated, run A (trace + report) against run B."""
import json, os, re, sys
from collections import Counter
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
def load(d):
    tr = d + '/trace.txt.out' if os.path.exists(d + '/trace.txt.out') else 'trace.txt.out'
    rep = set()
    for p in json.load(open(d + '/report.json'))['pairs']:
        rep.add(tuple(sorted((ident(p['a']), ident(p['b'])))))
    out = {}
    for l in open(tr):
        q, r = l.split()[:2]
        rq = re.search(r'rank_q (?:Some\((\d+)\)|None)', l).group(1); rr = re.search(r'rank_r (?:Some\((\d+)\)|None)', l).group(1)
        aq = int(re.search(r'asked_q (\d+)', l).group(1)); ar = int(re.search(r'asked_r (\d+)', l).group(1))
        ranks = [int(x) for x in (rq, rr) if x is not None]
        reached = (rq is not None and int(rq) < aq) or (rr is not None and int(rr) < ar)
        n_in = int(re.search(r'n_in (\d+)', l).group(1)); sh = int(re.search(r'shared (\d+)', l).group(1))
        out[(q, r)] = (min(ranks) if ranks else 10**9, reached, (q, r) in rep, sh, n_in)
    return out
A, B = load(sys.argv[1]), load(sys.argv[2])
print(f'{sys.argv[1]}: stated {sum(v[2] for v in A.values())}, reached {sum(v[1] for v in A.values())}')
print(f'{sys.argv[2]}: stated {sum(v[2] for v in B.values())}, reached {sum(v[1] for v in B.values())}')
c = Counter((A[k][2], B[k][2]) for k in A)
print('stated A/B:', dict(c))
c = Counter((A[k][1], B[k][1]) for k in A)
print('reached A/B:', dict(c))
bands = [0, 10, 50, 150, 500, 2000, 10000, 10**9 + 1]
def band(r): return next(f'<{b}' for b in bands[1:] if r < b)
print('rank bands A:', Counter(band(v[0]) for v in A.values()))
print('rank bands B:', Counter(band(v[0]) for v in B.values()))
print('\nstated in one only:')
for k in sorted(A):
    if A[k][2] != B[k][2]: print(k, 'A', A[k], 'B', B[k])
