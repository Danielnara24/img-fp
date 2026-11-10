#!/usr/bin/env python3
"""Anchors (and better) by direction: two-way (both frames' overlap past
the bar: the same frame) or one-way (a part of the other), labelled.

oneway.py CORPUS DUMP.csv
"""
import csv, sys
from collections import Counter, defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score
corpus, dump = sys.argv[1:3]
manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
seed = {r["path"]: r["seed"] for r in manifest}
var = {r["path"]: r["variant"] for r in manifest}
t = defaultdict(Counter)
bytrans = defaultdict(Counter)
with open(dump) as fh:
    for r in csv.DictReader(fh):
        if r["kind"] == "propagated":
            continue
        n, blk, bmin = int(r["n_in"]), float(r["blk"]), float(r["blk_min"])
        oa, ob = float(r["ov_a"]), float(r["ov_b"])
        if not (n >= 10 and r["centred"] == "1" and max(oa, ob) >= 0.85 and blk >= 0.7):
            continue
        way = "two-way" if min(oa, ob) >= 0.85 else "one-way"
        tier = "clean" if bmin >= 0.9 else "anchor"
        k = score.key(r["a"], r["b"])
        lab = "true" if k in pos else "skip" if k in skipped else ("trap" if seed[r["a"]] == seed[r["b"]] else "cross")
        t[(way, tier)][lab] += 1
        if lab == "cross":
            for f in (r["a"], r["b"]):
                bytrans[way][var[f]] += 1
for k, c in sorted(t.items()):
    print(f"{k[0]:8s} {k[1]:7s} true {c['true']:9,d}  trap {c['trap']:6,d}  cross {c['cross']:7,d}  skip {c['skip']:6,d}   cross share {c['cross']/max(1,sum(c.values())):.3%}")
for w, c in bytrans.items():
    print(w, "cross anchors' variants (either end):", c.most_common(14))
