"""dscore.py DIR...: DISC21 shard 0 from each run's report (bar 0.5 and 0.7), ranked by points + corr/2."""
import csv, json, os, sys
X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f"{X}/{os.environ.get('REFS', 'refs0')}.ids")}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
def qr(a, b):
    a, b = ident(a), ident(b)
    if {a[0], b[0]} != {'Q', 'R'}: return None
    return (a, b) if a[0] == 'Q' else (b, a)
def score(best, lo):
    sel = sorted(((v[0] + v[1] / 2, k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
    tp = ap = r90 = 0
    for i, (_, t) in enumerate(sel, 1):
        if t: tp += 1; ap += tp / i
        if tp / i >= 0.9: r90 = max(r90, tp / len(gt))
    return f'true {tp:4d} false {len(sel)-tp:5d}  uAP {100*ap/len(gt):5.2f}  R@P90 {100*r90:5.2f}'
for d in sys.argv[1:]:
    rep = {}
    for p in json.load(open(f'{d}/report.json'))['pairs']:
        k = qr(p['a'], p['b'])
        if k: v = (p['aligned_points'] or 0, p['pixel_correlation']); rep[k] = max(rep.get(k, v), v)
    t = open(f'{d}/time.txt').read() if os.path.exists(f'{d}/time.txt') else ''
    import re
    u = re.search(r'User time \(seconds\): (\S+)', t); e = re.search(r'Elapsed .*: (\S+)', t)
    print(f'{d:28s} 0.5: {score(rep, 0.5)} | 0.7: {score(rep, 0.7)} | cpu {u.group(1) if u else "?"} wall {e.group(1) if e else "?"}')
