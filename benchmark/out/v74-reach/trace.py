#!/usr/bin/env python3
"""trace.py: the trace lines of trace.err (or argv[1]) joined with whether each
pair is stated alone (../v73-disc-all/why/classify.out): where the missed
copies rank, by shared words and by geometric peak, from each end."""
import re, sys
from collections import Counter
H = '/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out'
alone = {}
for l in open(f'{H}/v73-disc-all/why/classify.out'):
    m = re.match(r'^(Q\d+) (R\d+) (.*?) (None|\(.*?\)) (True|False|None) (None|\(.*\))$', l.strip())
    if m:
        alone[m.group(1)] = m.group(5) == 'True'
rows = []
for l in open(sys.argv[1] if len(sys.argv) > 1 else 'trace.err'):
    if not l.startswith('TRACE '):
        continue
    q, r = l.split()[1:3]
    parts = l.split('|')
    kv = lambda part: {k: v for k, v in re.findall(r'(\w+)=([-\w.]+)', part)} | {'touched': (re.findall(r'of (\d+)', part) or ['0'])[0]}
    rows.append((q, r, kv(parts[0]), kv(parts[1]), kv(parts[2])))

def show(sel, title):
    print(f'\n{title}: {len(sel)}')
    print(f'{"pair":15s} ask   q->r rank  touched  score    kth    peak null pr1k   prall | r->q rank  peak null pr1k')
    for q, r, h, a, b in sel:
        print(f'{q} {r[1:]} {h["asker"][0]}  {int(a["rank"]):8d} {int(a["touched"]):8d} {float(a["score"]):.4f} {float(a["kth"]):.4f} {a["peak"]:>4s} {a["null"]:>4s} {a["peakrank1000"]:>5s} {a["peakrank"]:>6s} | {int(b["rank"]):8d} {b["peak"]:>4s} {b["null"]:>4s} {b["peakrank1000"]:>5s}')

def rk(x):
    x = int(x)
    return x if x >= 0 else 10**9

for title, sel in (('stated alone', [x for x in rows if alone.get(x[0])]), ('not stated alone', [x for x in rows if not alone.get(x[0])])):
    show(sel, title)
    bins = Counter()
    for q, r, h, a, b in sel:
        best = min(rk(a['rank']), rk(b['rank']))
        bins['<150' if best < 150 else '<1000' if best < 1000 else '<5000' if best < 5000 else '>=5000'] += 1
    print(f'  best word rank either way: {dict(bins)}')
    print(f'  askers: {sum(h["asker"] == "true" for _, _, h, _, _ in sel)}; q->r peak > null and >= 3: {sum(int(a["peak"]) > int(a["null"]) and int(a["peak"]) >= 3 for _, _, _, a, _ in sel)}'
          f'; peak first among all touched: q->r {sum(a["peakrank"] == "0" for _, _, _, a, _ in sel)}, r->q {sum(b["peakrank"] == "0" for _, _, _, _, b in sel)}'
          f'; peak in top 10 of all touched q->r: {sum(int(a["peakrank"]) < 10 for _, _, _, a, _ in sel)}')
