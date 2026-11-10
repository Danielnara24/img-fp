"""ambig.py DUMP: with A, every lone file the weak pass adds to a cluster,
and whether it also had a weak anchor into another cluster that closes a
cycle at that moment; split by whether its join and its rival are its own seed."""
import sys
from collections import Counter, defaultdict
from clus import load, seed_of, variant_of
from replay2 import bridges_drop
best = load(sys.argv[1])
S = [k for k, v in best.items() if v[3] >= 0.85]
W = [k for k, v in best.items() if v[3] < 0.85]
drop = bridges_drop(S); W += [S[e] for e in drop]
W.sort(key=lambda k: (-best[k][1], -best[k][2], k))
par, mem, cyc = {}, {}, {}
def f(x):
    if x not in par: par[x] = x; mem[x] = [x]; cyc[x] = False
    while par[x] != x: par[x] = par[par[x]]; x = par[x]
    return x
def u(a, b):
    ra, rb = f(a), f(b)
    if ra == rb: return
    if len(mem[ra]) > len(mem[rb]): ra, rb = rb, ra
    par[ra] = rb; mem[rb] += mem.pop(ra); cyc[rb] |= cyc.pop(ra)
for e, (a, b) in enumerate(S):
    if e in drop: continue
    if f(a) == f(b): cyc[f(a)] = True
    u(a, b)
near = defaultdict(list)
for a, b in W: near[a].append((b, best[(a, b) if (a, b) in best else (b, a)])); near[b].append((a, best[(a, b) if (a, b) in best else (b, a)]))
seedof = lambda r: Counter(seed_of[x] for x in mem[r]).most_common(1)[0][0]
stats = Counter(); ex = []
refused = set()
while True:
    joined = False
    for a, b in W:
        ra, rb = f(a), f(b)
        if ra == rb: continue
        small, big = (ra, rb) if len(mem[ra]) <= len(mem[rb]) else (rb, ra)
        if (small, big) in refused: continue
        cov = all(any(f(y) == big for y, _ in near[x]) for x in mem[small])
        ok = cov and (not (cyc[ra] and cyc[rb]) or any(sum(1 for z, _ in near[y] if f(z) == small) == len(mem[small]) for y in mem[big]))
        if ok:
            if len(mem[small]) == 1:
                x = mem[small][0]
                rivals = {f(y) for y, _ in near[x]} - {big, small}
                rivals = {r for r in rivals if cyc[r] and len(mem[r]) > 2}
                own = seedof(big) == seed_of[x]
                if rivals:
                    rown = any(seedof(r) == seed_of[x] for r in rivals)
                    stats[("ambiguous", "joined own" if own else "joined other", "a rival is own" if rown else "rivals other")] += 1
                    if not own: ex.append((seed_of[x], variant_of.get(x), seedof(big)))
                else:
                    stats[("clear", "joined own" if own else "joined other")] += 1
            u(small, big); refused.clear(); joined = True
        else:
            refused.add((small, big))
    if not joined: break
for k, v in sorted(stats.items()): print(v, k)
for e in ex: print("  ", e)
