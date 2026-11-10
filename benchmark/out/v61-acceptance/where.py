#!/usr/bin/env python3
"""Where do false pairs enter? Every verdict of a --dump labelled by the
generated truth, by kind and by the standing the shipped rule gives it, and
the report's false pairs by how they were claimed.

where.py CORPUS DUMP.csv REPORT.json [--bars points,overlap,corr]
"""
import csv, json, sys
from collections import Counter, defaultdict

sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score

corpus, dump, report = sys.argv[1:4]
PTS, OV, CORR = 10, 0.85, 0.7
CLEAN, SLACK = 0.9, 0.1

manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
seed = {r["path"]: r["seed"] for r in manifest}
var = {r["path"]: r["variant"] for r in manifest}


def label(a, b):
    k = score.key(a, b)
    if k in pos:
        return "true"
    if k in skipped:
        return "skip"
    return "trap" if seed.get(a) == seed.get(b) else "cross"


def standing(r):
    n, blk, bmin = int(r["n_in"]), float(r["blk"]), float(r["blk_min"])
    ov = max(float(r["ov_a"]), float(r["ov_b"]))
    centred = r["centred"] == "1"
    if ov < OV:  # frames(): overlap bar (scale bound already applied in the dump)
        return "rejected"
    points = n >= PTS
    if points and centred and blk >= CORR and bmin >= CLEAN:
        return "clean"
    # `whole` needs `framed`, not in the dump; counted under the points bar
    if not points:
        return "rejected"
    if centred and blk >= CORR:
        return "anchor"
    if blk >= CORR - SLACK:
        return "corroborating"
    return "rejected"


tab = defaultdict(Counter)
best = {}  # pair -> best (rank, kind:standing) it has in the dump
RANK = {"clean": 4, "anchor": 3, "corroborating": 2, "composed": 1, "rejected": 0}
cross_pairs = defaultdict(Counter)  # standing -> seed pair counts (cross only)
with open(dump) as fh:
    for r in csv.DictReader(fh):
        lab = label(r["a"], r["b"])
        kind = r["kind"]
        st = standing(r) if kind != "propagated" else ("composed" if float(r["blk"]) >= CORR and max(float(r["ov_a"]), float(r["ov_b"])) >= OV else "rejected")
        tab[(kind, st)][lab] += 1
        k = score.key(r["a"], r["b"])
        if RANK[st] > best.get(k, (-1, ""))[0]:
            best[k] = (RANK[st], f"{kind}:{st}")
        if lab == "cross" and st != "rejected":
            cross_pairs[(kind, st)][tuple(sorted((seed[r["a"]], seed[r["b"]])))] += 1

print(f"{'kind':10s} {'standing':14s} {'true':>10s} {'trap':>8s} {'cross':>8s} {'skip':>8s}")
for (kind, st), c in sorted(tab.items()):
    print(f"{kind:10s} {st:14s} {c['true']:10,d} {c['trap']:8,d} {c['cross']:8,d} {c['skip']:8,d}")
print()
for k, c in sorted(cross_pairs.items()):
    print(k, "seed pairs:", len(c), "top:", c.most_common(6))

# The report: its pairs by relation.
rep = json.load(open(report))
rel = defaultdict(Counter)
for p in rep["pairs"]:
    k = score.key(p["a"], p["b"])
    rel[best.get(k, (0, "no verdict (same bytes/pixels)"))[1]][label(p["a"], p["b"])] += 1
print()
print("report pairs by relation:")
for k, c in sorted(rel.items()):
    print(f"  {k:14s} true {c['true']:,}  trap {c['trap']:,}  cross {c['cross']:,}  skip {c['skip']:,}")
