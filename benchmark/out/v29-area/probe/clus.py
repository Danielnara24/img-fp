"""clus.py DUMP: replay clean anchors, the bridge test and admit_anchors (with
the cycle rule) and print, per cluster holding two seeds, which files cross
and by which join. Seeds IMGS-ALL-wide."""
import csv, sys, pickle, os
from collections import defaultdict, Counter
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from replay2 import bridges_drop
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import analyse
analyse.CORPORA = ["/home/daniel/Documents/IMGS-ALL"]
m, p = analyse.load_truth(); t = analyse.build(m, p)
seed_of, variant_of = t[1], t[2]

def load(dump):
    best = {}
    for r in csv.DictReader(open(dump)):
        if r["kind"] not in ("direct", "variant"): continue
        n_in, sc = int(r["n_in"]), float(r["scale"])
        if not (n_in >= 10 and r["centred"] == "1" and max(float(r["ov_a"]), float(r["ov_b"])) >= 0.85
                and float(r["blk"]) >= 0.6 and 1 / 16 <= sc <= 16): continue
        k = (min(r["a"], r["b"]), max(r["a"], r["b"]))
        v = (r["kind"] == "direct", n_in, float(r["blk"]), float(r["blk_min"]))
        if k not in best or v > best[k]: best[k] = v   # the direct verdict kept where there are two
    return best

def cluster(best, demote=False, order='n_in'):
    S = [k for k, v in best.items() if v[3] >= 0.85]
    W = [k for k, v in best.items() if v[3] < 0.85]
    drop = bridges_drop(S)
    if demote:
        W += [S[e] for e in drop]
    key = {'n_in': lambda k: (-best[k][1], -best[k][2], k),
           'blk_min': lambda k: (-best[k][3], -best[k][1], k)}[order]
    W.sort(key=key)
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
    for a, b in W: near[a].append(b); near[b].append(a)
    joins = []
    refused = set()
    while True:
        joined = False
        for a, b in W:
            ra, rb = f(a), f(b)
            if ra == rb: continue
            small, big = (ra, rb) if len(mem[ra]) <= len(mem[rb]) else (rb, ra)
            if (small, big) in refused: continue
            cov = all(any(f(y) == big for y in near[x]) for x in mem[small])
            ok = cov and (not (cyc[ra] and cyc[rb]) or any(sum(1 for z in near[y] if f(z) == small) == len(mem[small]) for y in mem[big]))
            if ok:
                joins.append((list(mem[small]), Counter(seed_of[x] for x in mem[big]).most_common(1)[0][0], (a, b), best[(a, b)]))
                u(small, big); refused.clear(); joined = True
            else:
                refused.add((small, big))
        if not joined: break
    return f, mem, joins

if __name__ == "__main__":
    best = load(sys.argv[1])
    f, mem, joins = cluster(best)
    for r, ms in mem.items():
        c = Counter(seed_of.get(x, "?") for x in ms)
        if len(c) > 1:
            top = c.most_common()
            print(f"cluster {len(ms)}: {top[:4]}")
    for small, bigseed, e, v in joins:
        ss = Counter(seed_of[x] for x in small)
        if bigseed not in ss or len(ss) > 1:
            print("JOIN", [(seed_of[x], variant_of.get(x)) for x in small][:6], "->", bigseed, "via", variant_of.get(e[0]), variant_of.get(e[1]), v)
