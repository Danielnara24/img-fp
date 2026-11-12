"""score.py REPORT.json REFS.ids [QDIR]: true/false copies and uAP at 0.5 and 0.7 off one report; writes REPORT.found (bar 0.5)."""
import csv, json, os, sys
X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(sys.argv[2])}
qs = {os.path.splitext(f)[0] for f in os.listdir(sys.argv[3])} if len(sys.argv) > 3 else None
gt = {(r[0], r[1]) for r in csv.reader(open(X + '/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs and (qs is None or r[0] in qs)}
best = {}
for p in json.load(open(sys.argv[1]))['pairs']:
    a, b = ident(p['a']), ident(p['b'])
    if {a[0], b[0]} != {'Q', 'R'}: continue
    k = (a, b) if a[0] == 'Q' else (b, a); v = (p['aligned_points'] or 0, p['pixel_correlation'])
    best[k] = max(best.get(k, v), v)
for lo in (0.5, 0.7):
    sel = sorted(((v[0] + v[1] / 2, k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
    tp = ap = 0
    for i, (_, t) in enumerate(sel, 1):
        if t: tp += 1; ap += tp / i
    print(f'  bar {lo}: true {tp} false {len(sel)-tp} uAP {ap/len(gt)*100:.2f} of {len(gt)}')
open(sys.argv[1][:-5] + '.found', 'w').write('\n'.join(sorted('|'.join(k) for k, v in best.items() if k in gt and v[1] >= 0.5)) + '\n')
