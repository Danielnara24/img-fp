#!/usr/bin/env python3
"""dscore.py RUN.json [BASE.json]: shard 0 true / false copies, uAP (ranked by
aligned points) at the run's bar and read at 0.65 / 0.7; with a base, the true
copies each states that the other does not, and CPU from the .err files."""
import csv, json, os, re, sys
X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}

def load(f):
    best = {}
    for p in json.load(open(f))['pairs']:
        a, b = ident(p['a']), ident(p['b'])
        if {a[0], b[0]} != {'Q', 'R'}:
            continue
        k = (a, b) if a[0] == 'Q' else (b, a)
        v = (p['aligned_points'] or 0, p['pixel_correlation'])
        best[k] = max(best.get(k, v), v)
    return best

def score(best, lo):
    sel = sorted(((v[0], k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
    tp = ap = 0
    for i, (_, t) in enumerate(sel, 1):
        if t:
            tp += 1; ap += tp / i
    return tp, len(sel) - tp, 100 * ap / len(gt)

def cpu(f):
    e = f[:-5] + '.err'
    if not os.path.exists(e):
        return ''
    t = open(e).read()
    u = re.search(r'User time \(seconds\): ([\d.]+)', t)
    s = re.search(r'System time \(seconds\): ([\d.]+)', t)
    w = re.search(r'Elapsed .*: ([\d:.]+)', t)
    d = re.search(r'\((\d+) deeper candidates\)', t)
    return f'  cpu {float(u.group(1)) + float(s.group(1)):.0f} s, wall {w.group(1)}' + (f', deeper candidates {d.group(1)}' if d else '') if u else ''

runs = sys.argv[1:]
data = [load(f) for f in runs]
for f, b in zip(runs, data):
    print(f'{os.path.basename(f)}:' + cpu(f))
    for lo in (0.0, 0.65, 0.7):
        tp, fp, ap = score(b, lo)
        print(f'  {"own bar" if lo == 0 else f"at {lo}":8s} true {tp:4d} false {fp:5d}  uAP {ap:5.2f}')
if len(data) == 2:
    t0, t1 = ({k for k in d if k in gt} for d in data)
    print(f'true only in {os.path.basename(runs[0])}: {sorted(q for q, _ in t0 - t1)}')
    print(f'true only in {os.path.basename(runs[1])}: {sorted(q for q, _ in t1 - t0)}')
