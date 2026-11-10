"""Each true DISC21 pair: where it is lost. Reads trace.txt.out, the report, alone.out."""
import json, os, re, sys
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
rep = set()
for p in json.load(open(sys.argv[1] if len(sys.argv) > 1 else 'disc-trace/report.json'))['pairs']:
    a, b = sorted((ident(p['a']), ident(p['b']))); rep.add((a, b))
alone = {l.split()[0]: 'stated=(' in l for l in open('alone.out')}
rows = []
for l in open('trace.txt.out'):
    q, r = l.split()[:2]
    g = lambda k: re.search(k + r' (\S+)', l).group(1)
    rq = re.search(r'rank_q (Some\((\d+)\)|None)/(\d+)', l); rr = re.search(r'rank_r (Some\((\d+)\)|None)/(\d+)', l)
    rq_ = int(rq.group(2)) if rq.group(2) else None; rr_ = int(rr.group(2)) if rr.group(2) else None
    aq, ar = int(g('asked_q')), int(g('asked_r'))
    reached = (rq_ is not None and rq_ < aq) or (rr_ is not None and rr_ < ar)
    sh, nin = int(g('shared')), int(g('n_in'))
    st = (q, r) in rep
    if st: cat = 'stated'
    elif rq_ is None and rr_ is None: cat = 'no shared word'
    elif not reached: cat = 'ranked past window'
    elif sh < 3: cat = 'asked, <3 seeds'
    else: cat = 'asked, verdict short'
    rows.append((cat, q, r, rq_, aq, rr_, ar, sh, nin, alone.get(q)))
from collections import Counter
c = Counter(x[0] for x in rows); ca = Counter(x[0] for x in rows if x[9])
for k in c: print(f'{k:24s} {c[k]:4d}   of which pass alone {ca.get(k, 0)}')
print()
for x in sorted(rows):
    if x[0] != 'stated': print(*x)
