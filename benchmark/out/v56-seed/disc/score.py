"""disc/score.py: DISC21 shard 0 for each build. (1) From the report (pairs as the run states them at
bar 0.5), each higher bar read off pixel_correlation, ranked by aligned points (score_shards.py's way).
(2) From the dump: direct and turned verdicts that stand as an anchor at each correlation step
(points >= 10, enclosing the centre, overlap >= 0.85, each direction within 1/16-16), ranked by
points + correlation / 2."""
import csv, json, math, os, sys
X = '/home/daniel/Documents/DISC21'; O = os.path.dirname(os.path.abspath(__file__))
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
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
    return f'true {tp:5d} false {len(sel)-tp:6d}  uAP {100*ap/len(gt):5.2f}  recall {100*tp/len(gt):5.2f}  R@P90 {100*r90:5.2f}'
def axes(m):
    a, b, c, d = m
    p = (a*a + b*b + c*c + d*d) / 2; q = math.sqrt(max(p*p - (a*d - b*c)**2, 0))
    return math.sqrt(max(p - q, 0)), math.sqrt(p + q)
print(f'{len(gt)} true copies in reach')
for b in sys.argv[1:] or ['head', 'ship']:
    rep = {}
    for p in json.load(open(f'{O}/{b}/report.json'))['pairs']:
        k = qr(p['a'], p['b'])
        if k: v = (p['aligned_points'] or 0, p['pixel_correlation']); rep[k] = max(rep.get(k, v), v)
    dump = {}
    for r in csv.DictReader(open(f'{O}/{b}/dump.csv')):
        if r['kind'] not in ('direct', 'variant'): continue
        k = qr(r['a'], r['b'])
        if not k: continue
        n = int(r['n_in']); lo, hi = axes([float(r[f'm{i}']) for i in (0, 1, 3, 4)])
        if n < 10 or r['centred'] != '1' or max(float(r['ov_a']), float(r['ov_b'])) < 0.85 or lo < 1/16 or hi > 16: continue
        dump.setdefault(k, []).append((n, float(r['blk'])))
    print(f'== {b}')
    for lo in (0.5, 0.6, 0.7, 0.8): print(f'  report bar {lo}: {score(rep, lo)}')
    for lo in (0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9): print(f'  dump   bar {lo}: {score({k: max((x for x in vs if x[1] >= lo), default=(0, -1)) for k, vs in dump.items()}, lo)}')
