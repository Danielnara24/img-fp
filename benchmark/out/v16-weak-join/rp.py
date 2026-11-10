"""Replay the anchor tier of one --dump with a pluggable weak-anchor join rule.

The dumps are not kept (60-90 MB each): make one with `img-fp CORPUS -r -x "*"
--work-size WS --dump dumps/NAME_WS.csv` beside this file. `cmp.py NAME_WS CORPUS`
writes one replay_*.txt table: shipped, cycles refused, cycles with one
vouching file (what ships).

Nodes are byte-identity groups (sha256), as the run matches copies through
their original. Reports family merges, stray files, the cross-family pairs the
components imply, and reach: positive pairs whose two files share a component.
"""
import csv, os, pickle, sys
from collections import defaultdict, Counter

sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v14-fullsweep")
from replay2 import bridges_drop

S = os.path.dirname(os.path.abspath(__file__))


def load(name, corpora):
    pk = f"{S}/dumps/{name}.pkl"
    if os.path.exists(pk):
        return pickle.load(open(pk, "rb"))
    node, seed, mult = {}, {}, Counter()
    for c in corpora:
        t = os.path.basename(c) + "/"
        for r in csv.DictReader(open(c + "/derived/manifest.csv")):
            node[r["path"]] = r["sha256"]
            seed[r["sha256"]] = t + r["seed"]
            mult[r["sha256"]] += 1
    rows = []
    for r in csv.DictReader(open(f"{S}/dumps/{name}.csv")):
        if r["kind"] not in ("direct", "variant") or r["a"] not in node or r["b"] not in node:
            continue
        sc = float(r["scale"])
        if not (r["centred"] == "1" and 1 / 16 <= sc <= 16):
            continue
        rows.append((int(r["n_in"]), float(r["blk"]), float(r["blk_min"]),
                     max(float(r["ov_a"]), float(r["ov_b"])), node[r["a"]], node[r["b"]]))
    rows.sort(key=lambda r: (-r[0], -r[1], r[4], r[5]))
    pos = set()
    for c in corpora:
        for r in csv.DictReader(open(c + "/derived/pairs.csv")):
            if r["relation"] in ("SAME", "CROP"):
                a, b = node.get(r["a"]), node.get(r["b"])
                if a and b and a != b:
                    pos.add((min(a, b), max(a, b)))
    d = dict(rows=rows, seed=seed, mult=mult, pos=list(pos))
    pickle.dump(d, open(pk, "wb"))
    return d


def edges(d, bar=10, ov=0.85, corr=0.6, clean=0.85):
    seen = set(); Sx = []; W = []
    for n_in, blk, bmin, o, a, b in d["rows"]:
        if n_in < bar or o < ov or blk < corr or a == b:
            continue
        k = (min(a, b), max(a, b))
        if k in seen:
            continue
        seen.add(k)
        (Sx if bmin >= clean else W).append((a, b, n_in, blk, bmin))
    drop = bridges_drop([(a, b) for a, b, *_ in Sx])
    Sk = [e for i, e in enumerate(Sx) if i not in drop]
    return Sk, W


class DSU:
    def __init__(self, nodes):
        self.p = {x: x for x in nodes}; self.m = {x: {x} for x in nodes}

    def f(self, x):
        p = self.p
        while p[x] != x:
            p[x] = p[p[x]]; x = p[x]
        return x

    def u(self, a, b):
        ra, rb = self.f(a), self.f(b)
        if ra == rb:
            return rb
        if len(self.m[ra]) > len(self.m[rb]):
            ra, rb = rb, ra
        self.p[ra] = rb; self.m[rb] |= self.m.pop(ra)
        return rb


def shipped(Sk, W, nodes):
    """The rule as it ships: joins in weak-anchor order, repeated."""
    D = DSU(nodes)
    for a, b, *_ in Sk:
        D.u(a, b)
    adj = defaultdict(list)
    for a, b, *_ in W:
        adj[a].append(b); adj[b].append(a)
    changed = True
    while changed:
        changed = False
        for a, b, *_ in W:
            ra, rb = D.f(a), D.f(b)
            if ra == rb:
                continue
            small, big = (ra, rb) if len(D.m[ra]) <= len(D.m[rb]) else (rb, ra)
            if all(any(D.f(y) == big for y in adj[x]) for x in D.m[small]):
                D.u(ra, rb); changed = True
    return D


def score(d, D, nodes):
    seed, mult = d["seed"], d["mult"]
    fam = strays = 0; xpairs = 0; worst = []
    for r, m in D.m.items():
        cnt = Counter()
        for x in m:
            cnt[seed[x]] += mult[x]
        if len(cnt) < 2:
            continue
        top = cnt.most_common()
        tot = sum(cnt.values())
        xp = (tot * tot - sum(v * v for v in cnt.values())) // 2
        xpairs += xp
        for s, n in top[1:]:
            if n >= 4:
                fam += 1; worst.append((top[0][0], s, n, top[0][1]))
            else:
                strays += n
    inn = set(nodes)
    reach = sum(1 for a, b in d["pos"] if a in inn and b in inn and D.f(a) == D.f(b))
    return dict(merges=fam, strays=strays, xpairs=xpairs, reach=reach / len(d["pos"]),
                comps=sum(1 for m in D.m.values() if len(m) > 1), worst=worst)


def run(d, rule=shipped, **kw):
    Sk, W = edges(d, **kw)
    nodes = {x for e in Sk + W for x in e[:2]}
    D = rule(Sk, W, nodes)
    return score(d, D, nodes)


CORP = {k: f"/home/daniel/Documents/{k}" for k in ("IMGS", "IMGS2", "IMGS3", "IMGS4")}


def trace(d, **kw):
    """The shipped rule, printing every join whose two sides' majority seeds
    differ, with what the two sides were and how each touches the other."""
    seed, mult = d["seed"], d["mult"]
    Sk, W = edges(d, **kw)
    nodes = {x for e in Sk + W for x in e[:2]}
    D = DSU(nodes)
    for a, b, *_ in Sk:
        D.u(a, b)
    clean_root = {x: D.f(x) for x in nodes}
    clean_m = {r: set(m) for r, m in D.m.items()}
    adj = defaultdict(list)
    for a, b, *_ in W:
        adj[a].append(b); adj[b].append(a)

    def desc(r):
        c = Counter()
        for x in D.m[r]:
            c[seed[x].split("/")[-1]] += 1
        return f"{len(D.m[r])}:{dict(c.most_common(3))}"
    changed = True; step = 0
    while changed:
        changed = False
        for a, b, *_ in W:
            ra, rb = D.f(a), D.f(b)
            if ra == rb:
                continue
            small, big = (ra, rb) if len(D.m[ra]) <= len(D.m[rb]) else (rb, ra)
            if all(any(D.f(y) == big for y in adj[x]) for x in D.m[small]):
                ms = Counter(seed[x] for x in D.m[small]).most_common(1)[0][0]
                mb = Counter(seed[x] for x in D.m[big]).most_common(1)[0][0]
                # how many of small's files also touch some *other* cluster
                if ms != mb:
                    others = Counter()
                    for x in D.m[small]:
                        for t in {D.f(y) for y in adj[x]} - {big, small}:
                            others[t] += 1
                    back = sum(1 for x in D.m[big] if any(D.f(y) == small for y in adj[x]))
                    print(f"  step {step}: {desc(small)} -> {desc(big)}  big-files-touching-small={back}"
                          f"  small also covers: {[(desc(t), k) for t, k in others.most_common(3)]}")
                D.u(ra, rb); changed = True; step += 1
    return D


def join_stats(d, **kw):
    """Every join the shipped rule makes where the small side has 2+ files:
    (ok, |small|, |big|, clean edges inside small, weak edges small-big,
     files of big touching small, majority seeds)."""
    seed = d["seed"]
    Sk, W = edges(d, **kw)
    nodes = {x for e in Sk + W for x in e[:2]}
    D = DSU(nodes)
    for a, b, *_ in Sk:
        D.u(a, b)
    cadj = defaultdict(set)
    for a, b, *_ in Sk:
        cadj[a].add(b); cadj[b].add(a)
    adj = defaultdict(list)
    for a, b, *_ in W:
        adj[a].append(b); adj[b].append(a)
    out = []
    changed = True
    while changed:
        changed = False
        for a, b, *_ in W:
            ra, rb = D.f(a), D.f(b)
            if ra == rb:
                continue
            small, big = (ra, rb) if len(D.m[ra]) <= len(D.m[rb]) else (rb, ra)
            if all(any(D.f(y) == big for y in adj[x]) for x in D.m[small]):
                if len(D.m[small]) >= 2:
                    ms = Counter(seed[x] for x in D.m[small]).most_common(1)[0][0]
                    mb = Counter(seed[x] for x in D.m[big]).most_common(1)[0][0]
                    S_, B_ = D.m[small], D.m[big]
                    inner = sum(1 for x in S_ for y in cadj[x] if y in S_) // 2
                    cross = sum(1 for x in S_ for y in adj[x] if y in B_)
                    back = sum(1 for x in B_ if any(y in S_ for y in adj[x]))
                    out.append((ms == mb, len(S_), len(B_), inner, cross, back, ms.split('/')[-1], mb.split('/')[-1]))
                D.u(ra, rb); changed = True
    return out


def acyclic_side(Sk, W, nodes):
    """The shipped cover rule, and no join between two clusters that each
    hold a cycle of clean anchors."""
    D = DSU(nodes)
    cyc = {x: False for x in nodes}
    for a, b, *_ in Sk:
        ra, rb = D.f(a), D.f(b)
        if ra == rb:
            cyc[ra] = True
        else:
            c = cyc[ra] or cyc[rb]
            cyc[D.u(a, b)] = c
    adj = defaultdict(list)
    for a, b, *_ in W:
        adj[a].append(b); adj[b].append(a)
    changed = True
    while changed:
        changed = False
        for a, b, *_ in W:
            ra, rb = D.f(a), D.f(b)
            if ra == rb or (cyc[ra] and cyc[rb]):
                continue
            small, big = (ra, rb) if len(D.m[ra]) <= len(D.m[rb]) else (rb, ra)
            if all(any(D.f(y) == big for y in adj[x]) for x in D.m[small]):
                c = cyc[ra] or cyc[rb]
                cyc[D.u(ra, rb)] = c; changed = True
    return D


def refusals(d, var={}, **kw):
    """Joins the shipped rule makes between two cyclic clusters."""
    seed = d["seed"]
    Sk, W = edges(d, **kw)
    nodes = {x for e in Sk + W for x in e[:2]}
    D = DSU(nodes); cyc = {x: False for x in nodes}
    for a, b, *_ in Sk:
        ra, rb = D.f(a), D.f(b)
        if ra == rb: cyc[ra] = True
        else:
            c = cyc[ra] or cyc[rb]; cyc[D.u(a, b)] = c
    cadj = defaultdict(set)
    for a, b, *_ in Sk:
        cadj[a].add(b); cadj[b].add(a)
    adj = defaultdict(list)
    for a, b, *_ in W:
        adj[a].append(b); adj[b].append(a)
    changed = True
    while changed:
        changed = False
        for a, b, *_ in W:
            ra, rb = D.f(a), D.f(b)
            if ra == rb: continue
            small, big = (ra, rb) if len(D.m[ra]) <= len(D.m[rb]) else (rb, ra)
            if all(any(D.f(y) == big for y in adj[x]) for x in D.m[small]):
                if cyc[ra] and cyc[rb]:
                    S_, B_ = D.m[small], D.m[big]
                    inner = sum(1 for x in S_ for y in cadj[x] if y in S_) // 2
                    ms = Counter(seed[x].split('/')[-1] for x in S_); mb = Counter(seed[x].split('/')[-1] for x in B_)
                    print("    ", sorted(var.get(x, "?") for x in S_)[:4], "back", sum(1 for x in B_ if any(y in S_ for y in adj[x])), "hub", max(len(set(adj[y]) & S_) for y in B_), "of", len(S_), "| small-hub", max(len(set(adj[x]) & B_) for x in S_), "of", len(B_), "density %.3f" % (sum(1 for x in S_ for y in set(adj[x]) if y in B_) / (len(S_) * len(B_)))); print(f"  {len(S_)} files {inner} clean {dict(ms)} -> {len(B_)} {dict(mb.most_common(2))}")
                c = cyc[ra] or cyc[rb]; cyc[D.u(ra, rb)] = c; changed = True


def hub_side(Sk, W, nodes):
    """As acyclic_side, except that two cyclic clusters still join when one
    file of the larger has a weak anchor to every file of the smaller."""
    D = DSU(nodes)
    cyc = {x: False for x in nodes}
    for a, b, *_ in Sk:
        ra, rb = D.f(a), D.f(b)
        if ra == rb:
            cyc[ra] = True
        else:
            c = cyc[ra] or cyc[rb]
            cyc[D.u(a, b)] = c
    adj = defaultdict(set)
    for a, b, *_ in W:
        adj[a].add(b); adj[b].add(a)
    changed = True
    while changed:
        changed = False
        for a, b, *_ in W:
            ra, rb = D.f(a), D.f(b)
            if ra == rb:
                continue
            small, big = (ra, rb) if len(D.m[ra]) <= len(D.m[rb]) else (rb, ra)
            S_ = D.m[small]
            if not all(any(D.f(y) == big for y in adj[x]) for x in S_):
                continue
            if cyc[ra] and cyc[rb] and not any(S_ <= adj[y] for y in D.m[big]):
                continue
            c = cyc[ra] or cyc[rb]
            cyc[D.u(ra, rb)] = c; changed = True
    return D
