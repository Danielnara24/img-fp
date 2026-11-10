import sys
exec(open('replay2.py').read().split("rules = {")[0])
pick = lambda c, mx, sec, p90: [x for x in c if x[1] > mx]
for M in (300, 400, 450, 500, 550, 600, 650, 700, 750, 800, 900, 1000, 1250, 1500, 2000, 3000):
    s, v, a, b, fa, cpu = run(M, pick)
    print(f'>max {M:5d} | verified {v:7d} | gain {a:3d} {b:3d} | false {fa:3d} | est +{100*cpu/2623:.1f}%')
# rank of each true copy the null rule finds at depth 3000, and every true copy any rule finds
got = {}
for n, mx, sec, p90, c in askers:
    for r, peak, st, blk, k in c:
        if st >= 2 and k in gt and k not in base[0.5]:
            got.setdefault(k, []).append((r + K, peak, mx, round(blk, 2)))
print('true copies standing past the window (rank, peak, file null, corr):')
for k, v in sorted(got.items(), key=lambda x: min(y[0] for y in x[1])): print(' ', k, sorted(v)[:2], 'beats null' if any(y[1] > y[2] for y in v) else '')
