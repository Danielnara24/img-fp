#!/usr/bin/env python3
"""Replay the anchor tier and drop_weak_bridges off one --dump, per CLAUDE.md.

The output table cannot show the margin behind the precision plateau: what
matters is how many cross-family *anchors* the bridge test has to absorb, and
the test survives exactly one false edge. That count needs the anchors before
the test, which only a replay gives.
"""
import csv, sys
from collections import defaultdict

CORPUS = "/home/daniel/Documents/IMGS"
MAN = CORPUS + "/derived/manifest.csv"
MAX_SCALE = 16.0


def seeds():
    with open(MAN) as fh:
        return {r["path"]: r["seed"] for r in csv.DictReader(fh)}


def accepted(row, k, min_ov, min_blk, centred_required):
    sc = float(row["scale"])
    return (sc == sc and abs(sc) != float("inf")
            and sc >= 1.0 / MAX_SCALE and sc <= MAX_SCALE
            and int(row["n_in"]) >= k
            and (not centred_required or row["centred"] == "1")
            and max(float(row["ov_a"]), float(row["ov_b"])) >= min_ov
            and float(row["blk"]) >= min_blk)


def bridges(edges, nodes):
    """Iterative Tarjan; returns the set of edge indices to drop (far side >= 2)."""
    adj = defaultdict(list)
    for e, (a, b) in enumerate(edges):
        adj[a].append((b, e))
        adj[b].append((a, e))
    disc, low, size = {}, {}, defaultdict(lambda: 1)
    timer = 0
    drop = set()
    for s in nodes:
        if s in disc or not adj[s]:
            continue
        # component size, for the far-side comparison
        seen, mark, i = [s], {s}, 0
        while i < len(seen):
            u = seen[i]; i += 1
            for v, _ in adj[u]:
                if v not in mark:
                    mark.add(v); seen.append(v)
        total = len(seen)
        disc[s] = low[s] = timer; timer += 1
        size[s] = 1
        stack = [(s, -1, 0)]
        while stack:
            u, pe, k = stack[-1]
            if k < len(adj[u]):
                stack[-1] = (u, pe, k + 1)
                v, e = adj[u][k]
                if e == pe:
                    continue
                if v not in disc:
                    disc[v] = low[v] = timer; timer += 1
                    size[v] = 1
                    stack.append((v, e, 0))
                else:
                    low[u] = min(low[u], disc[v])
            else:
                stack.pop()
                if stack:
                    p = stack[-1][0]
                    low[p] = min(low[p], low[u])
                    size[p] += size[u]
                    if low[u] > disc[p] and pe != -1:
                        if min(size[u], total - size[u]) >= 2:
                            drop.add(pe)
    return drop


def merges(pairs):
    parent = {}
    def find(x):
        parent.setdefault(x, x)
        while parent[x] != x:
            parent[x] = parent[parent[x]]; x = parent[x]
        return x
    m = 0
    for a, b in pairs:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[ra] = rb; m += 1
    return m


def main(dump, bars, min_ov=0.85, min_blk=0.50):
    seed_of = seeds()
    rows = []
    with open(dump) as fh:
        for r in csv.DictReader(fh):
            if r["kind"] in ("direct", "variant"):
                rows.append(r)
    print(f"{len(rows):,} direct+variant verdicts in the dump")
    print(f"{'bar':>4} {'anchors':>9} {'cross anchors':>14} {'after bridges':>14} "
          f"{'families merged':>16}")
    for k in bars:
        edges, cross_pre = [], []
        for r in rows:
            if accepted(r, k, min_ov, min_blk, True):
                a, b = r["a"], r["b"]
                edges.append((a, b))
                if seed_of[a] != seed_of[b]:
                    cross_pre.append((a, b))
        nodes = {x for e in edges for x in e}
        drop = bridges(edges, nodes)
        kept = [e for i, e in enumerate(edges) if i not in drop]
        cross_post = [(a, b) for a, b in kept if seed_of[a] != seed_of[b]]
        sp = {tuple(sorted((seed_of[a], seed_of[b]))) for a, b in cross_post}
        print(f"{k:>4} {len(edges):>9,} {len(cross_pre):>14} {len(cross_post):>14} "
              f"{merges(sp):>16}")


if __name__ == "__main__":
    main(sys.argv[1], [int(x) for x in sys.argv[2].split(",")],
         *(float(x) for x in sys.argv[3:]))
