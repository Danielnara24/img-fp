#!/usr/bin/env python3
"""Offline: "same picture" as an equivalence of two-way clean links, parts
attached and never joining. Upper bound of what propagation could state
(every pair inside a class, a part with every member of a class it is in;
propagation's own pixel check would drop some), against the shipped report.

sim.py CORPUS DUMP.csv REPORT.json [lone]
  lone: a lone file joins a class through any anchor (as `admit` lets it)
"""
import csv, json, sys
from collections import Counter, defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score
corpus, dump, report = sys.argv[1:4]
LONE = "lone" in sys.argv[4:]
manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
seed = {r["path"]: r["seed"] for r in manifest}

par = {}
def find(x):
    par.setdefault(x, x)
    while par[x] != x:
        par[x] = par[par[x]]
        x = par[x]
    return x
def join(a, b):
    a, b = find(a), find(b)
    if a != b:
        par[a] = b

direct = {}   # pair -> (two_way, clean)
with open(dump) as fh:
    for r in csv.DictReader(fh):
        if r["kind"] == "propagated":
            continue
        n, blk, bmin = int(r["n_in"]), float(r["blk"]), float(r["blk_min"])
        oa, ob = float(r["ov_a"]), float(r["ov_b"])
        if not (n >= 10 and r["centred"] == "1" and max(oa, ob) >= 0.85 and blk >= 0.7):
            continue
        k = score.key(r["a"], r["b"])
        two, clean = min(oa, ob) >= 0.85, bmin >= 0.9
        old = direct.get(k)
        direct[k] = (two or (old or (0, 0))[0], clean or (old or (0, 0))[1]) if old else (two, clean)
rep = json.load(open(report))
reported = {score.key(p["a"], p["b"]) for p in rep["pairs"]}
in_dump = set()
with open(dump) as fh:
    for r in csv.DictReader(fh):
        in_dump.add(score.key(r["a"], r["b"]))
# Identity: two-way clean, and pairs the report states with no verdict (same bytes / pixels).
for k, (two, clean) in direct.items():
    find(k[0]); find(k[1])
    if two and clean:
        join(*k)
for k in reported - in_dump:
    join(*k)
members = defaultdict(set)
for x in list(par):
    members[find(x)].add(x)
if LONE:
    for k, (two, clean) in direct.items():
        a, b = find(k[0]), find(k[1])
        if a != b and (len(members[a]) == 1 or len(members[b]) == 1):
            s, o = (a, b) if len(members[a]) == 1 else (b, a)
            join(s, o)
    members = defaultdict(set)
    for x in list(par):
        members[find(x)].add(x)

claims = set()
for c in members.values():
    m = sorted(c)
    for i in range(len(m)):
        for j in range(i + 1, len(m)):
            claims.add((m[i], m[j]))
# Parts and every other standing link: the pair, and the part with each member of the other's class.
for k, (two, clean) in direct.items():
    a, b = k
    claims.add(k)
    if find(a) != find(b):
        for x in members[find(b)]:
            claims.add(score.key(a, x))
        for x in members[find(a)]:
            claims.add(score.key(b, x))

def tally(s):
    c = Counter()
    cross = Counter()
    for k in s:
        if k in pos: c["true"] += 1
        elif k in skipped: c["skip"] += 1
        elif seed.get(k[0]) == seed.get(k[1]): c["trap"] += 1
        else:
            c["cross"] += 1
            cross[tuple(sorted((seed[k[0]], seed[k[1]])))] += 1
    tp, fp = c["true"], c["trap"] + c["cross"]
    p, r = tp / max(1, tp + fp), tp / len(pos)
    f1 = 2 * p * r / max(1e-9, p + r)
    big = [(g, n) for g, n in cross.most_common() if n >= 300]
    return f"true {tp:,} trap {c['trap']:,} cross {c['cross']:,}  P {p:.2%} R {r:.2%} F1 {f1:.4f}  merges>=300 {len(big)} {big[:8]}"

cls = [c for c in members.values() if len(c) > 1]
mixed = sum(1 for c in cls if len({seed[x] for x in c}) > 1)
print(f"classes of 2+: {len(cls):,}; holding two seeds: {mixed}")
print("shipped report:", tally(reported))
print("simulated    :", tally(claims))
print("simulated, only reported or in-class:", tally({k for k in claims if find(k[0]) == find(k[1]) or k in direct}))
