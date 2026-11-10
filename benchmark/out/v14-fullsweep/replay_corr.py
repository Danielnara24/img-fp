#!/usr/bin/env python3
"""Replay the anchor tier of one --dump at every --min-aligned-points bar:
cross-family anchors before anything, clean ones among them, clean ones the
bridge test keeps, and the families the clean-anchor rule then joins.

usage: replay_ap.py DUMP CORPUS..."""
import csv, os, sys
from collections import defaultdict, Counter
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from replay2 import bridges_drop

dump, corpora = sys.argv[1], sys.argv[2:]
seed = {}
for c in corpora:
    t = os.path.basename(c) + "/"
    for r in csv.DictReader(open(c + "/derived/manifest.csv")):
        seed[r["path"]] = t + r["seed"]
rows = []
for r in csv.DictReader(open(dump)):
    if r["kind"] not in ("direct", "variant") or r["a"] not in seed or r["b"] not in seed:
        continue
    n_in, sc = int(r["n_in"]), float(r["scale"])
    if (n_in >= 10 and r["centred"] == "1" and max(float(r["ov_a"]), float(r["ov_b"])) >= 0.85
            and float(r["blk"]) >= 0.2 and 1 / 16 <= sc <= 16):
        rows.append((n_in, float(r["blk"]), float(r["blk_min"]), r["a"], r["b"]))
rows.sort(reverse=True)


def replay(cbar):
    bar = 10
    seen = set(); S = []; W = []
    for n_in, blk, bmin, a, b in rows:
        if blk < cbar:
            continue
        k = (min(a, b), max(a, b))
        if k in seen:
            continue
        seen.add(k)
        (S if bmin >= 0.85 else W).append(k)
    cross = lambda e: seed[e[0]] != seed[e[1]]
    drop = bridges_drop(S)
    par = {}; mem = {}
    for x in {x for e in S + W for x in e}:
        par[x] = x; mem[x] = {x}

    def f(x):
        while par[x] != x:
            par[x] = par[par[x]]; x = par[x]
        return x

    def u(a, b):
        ra, rb = f(a), f(b)
        if ra == rb:
            return
        if len(mem[ra]) > len(mem[rb]):
            ra, rb = rb, ra
        par[ra] = rb; mem[rb] |= mem.pop(ra)
    kept_clean_cross = 0
    for e, (a, b) in enumerate(S):
        if e not in drop:
            u(a, b)
            kept_clean_cross += cross((a, b))
    adj = defaultdict(list)
    for a, b in W:
        adj[a].append(b); adj[b].append(a)
    changed = True
    while changed:
        changed = False
        for a, b in W:
            ra, rb = f(a), f(b)
            if ra == rb:
                continue
            small, big = (ra, rb) if len(mem[ra]) <= len(mem[rb]) else (rb, ra)
            if all(any(f(y) == big for y in adj[x]) for x in mem[small]):
                u(ra, rb); changed = True
    # a family merge is a component holding four or more files of a second
    # seed; fewer is a stray file joining its sibling's family
    fam = strays = 0
    for m in mem.values():
        cnt = Counter(seed[x] for x in m)
        if len(cnt) < 2:
            continue
        top = cnt.most_common()
        for s, n in top[1:]:
            if n >= 4:
                fam += 1
            else:
                strays += n
    return dict(bar=cbar, anchors=len(S) + len(W), clean=len(S),
                cross=sum(map(cross, S + W)), clean_cross=sum(map(cross, S)),
                clean_cross_kept=kept_clean_cross, fam_merges=fam, stray_files=strays)


print(f"{'corr':>4} {'anchors':>8} {'clean':>8} {'cross':>6} {'clean-x':>7} {'kept-x':>6} {'merges':>6} {'strays':>6}")
for bar in (0.5, 0.55, 0.57, 0.58, 0.59, 0.6, 0.62, 0.65, 0.68, 0.69, 0.7, 0.75, 0.8):
    r = replay(bar)
    print(f"{r['bar']:>4} {r['anchors']:>8,} {r['clean']:>8,} {r['cross']:>6} {r['clean_cross']:>7} "
          f"{r['clean_cross_kept']:>6} {r['fam_merges']:>6} {r['stray_files']:>6}", flush=True)
