"""replay2.py: like replay.py, with each file's first-window null (A i len max second p90).
Rules: fixed budget W (best peak first), or every candidate whose peak beats the null.
CPU estimated from the sweep's own costs: scoring 1.5 us, verifying 89 us, a deep query 48 CPU-s at 3000."""
import csv, json, os, sys
X = '/home/daniel/Documents/DISC21'; K = 150
SCORE, VERIFY, QUERY = 1.5e-6, 89e-6, 48.0
ident = lambda p: os.path.splitext(os.path.basename(p.strip()))[0]
E = __import__('os').environ
names = [ident(l) for l in open(E.get('NAMES', 'sweep.names'))]
refs = {l.strip() for l in open(f"{X}/{E.get('REFS', 'refs0')}.ids")}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
def key(a, b):
    a, b = names[a], names[b]; return (a, b) if a[0] == 'Q' else (b, a)
base = {0.5: set(), 0.7: set()}
for p in json.load(open(E.get('BASE', 'disc-trace') + '/report.json'))['pairs']:
    a, b = ident(p['a']), ident(p['b']); k = (a, b) if a[0] == 'Q' else (b, a)
    for bar in base:
        if p['pixel_correlation'] >= bar: base[bar].add(k)
askers = []
for l in open(E.get('LOG', 'sweep.log')):
    f = l.split()
    if f[0] == 'A':
        i = int(f[1]); cur = (int(f[2]), int(f[3]), int(f[4]), int(f[5]), []); askers.append(cur); continue
    j, r, peak, st, nin, blk = int(f[0]), int(f[1]), int(f[2]), int(f[3]), int(f[4]), float(f[5])
    cur[4].append((r, peak, st, blk, key(i, j)))
def run(M, pick):
    scored = sum(max(0, min(n, M) - K) for n, *_ in askers); ver = 0; g5 = set(); g7 = set(); fa = 0
    for n, mx, sec, p90, c in askers:
        take = pick(sorted((x for x in c if x[0] < M - K), key=lambda x: (-x[1], x[0])), mx, sec, p90)
        ver += len(take)
        for r, peak, st, blk, k in take:
            if st >= 2 and {k[0][0], k[1][0]} == {'Q', 'R'}:
                if k in gt:
                    if k not in base[0.5]: g5.add(k)
                    if blk >= 0.7 and k not in base[0.7]: g7.add(k)
                else: fa += 1
    cpu = scored * SCORE + ver * VERIFY + QUERY * M / 3000
    return scored, ver, len(g5), len(g7), fa, cpu
rules = {f'W{w}': (lambda w: lambda c, mx, sec, p90: c[:w])(w) for w in (3, 5, 10, 20, 50)}
rules['>max'] = lambda c, mx, sec, p90: [x for x in c if x[1] > mx]
rules['>=max'] = lambda c, mx, sec, p90: [x for x in c if x[1] >= mx]
rules['>2nd'] = lambda c, mx, sec, p90: [x for x in c if x[1] > sec]
rules['>p90'] = lambda c, mx, sec, p90: [x for x in c if x[1] > p90]
rules['all'] = lambda c, mx, sec, p90: c
print(f'{len(askers)} askers; null max: median {sorted(a[1] for a in askers)[len(askers)//2]}')
print('rule   depth | verified | gain .5 .7 | false Q-R anchors | est. CPU-s (base 2,623)')
for name, pick in rules.items():
    for M in (500, 750, 1000, 1500, 2000, 3000):
        s, v, a, b, fa, cpu = run(M, pick)
        print(f'{name:6s} {M:5d} | {v:8d} | {a:3d} {b:3d} | {fa:4d} | {cpu:6.0f} (+{100*cpu/2623:.1f}%)')
