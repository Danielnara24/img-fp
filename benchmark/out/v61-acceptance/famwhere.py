#!/usr/bin/env python3
"""Small families: each reported pair by the best verdict the run's --dump
holds for it (kind, tier, one- or two-way), labelled true / trap / cross.

famwhere.py CORPUS FAMDIR [subset-glob]
"""
import csv, glob, json, os, sys
from collections import Counter, defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score
corpus, famdir = sys.argv[1:3]
pat = sys.argv[3] if len(sys.argv) > 3 else "*"
manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
seed = {r["path"]: r["seed"] for r in manifest}
var = {r["path"]: r["variant"] for r in manifest}
RANK = {"clean": 4, "anchor": 3, "corroborating": 2, "composed": 1, "weak": 0}

def tier(r):
    n, blk, bmin = int(r["n_in"]), float(r["blk"]), float(r["blk_min"])
    oa, ob = float(r["ov_a"]), float(r["ov_b"])
    way = "2way" if min(oa, ob) >= 0.85 else "1way"
    if r["kind"] == "propagated":
        return ("composed" if blk >= 0.7 else "weak"), way
    if n >= 10 and r["centred"] == "1" and blk >= 0.7:
        return ("clean" if bmin >= 0.9 else "anchor"), way
    if blk >= 0.6 and n >= 10:
        return "corroborating", way
    return "weak", way

tot = defaultdict(Counter)
examples = defaultdict(list)
for js in sorted(glob.glob(os.path.join(famdir, f"*{pat}*.json"))):
    sub = os.path.basename(js)[:-5]
    if sub.endswith("-f") or sub.endswith("full"):
        continue
    best = {}
    with open(js[:-5] + ".csv") as fh:
        for r in csv.DictReader(fh):
            k = score.key(r["a"], r["b"])
            t, w = tier(r)
            v = (RANK[t], f"{r['kind'][:4]}:{t}:{w}", r)
            if k not in best or v[0] > best[k][0]:
                best[k] = v
    for p in json.load(open(js))["pairs"]:
        k = score.key(p["a"], p["b"])
        lab = "true" if k in pos else "skip" if k in skipped else ("trap" if seed[k[0]] == seed[k[1]] else "cross")
        cls = best[k][1] if k in best else "no-verdict"
        tot[cls][lab] += 1
        if lab == "cross":
            r = best[k][2] if k in best else None
            examples[cls].append((sub, seed[k[0]], var[k[0]], seed[k[1]], var[k[1]],
                                  r and (r["n_in"], r["blk"], r["blk_min"], r["ov_a"], r["ov_b"])))
print(f"{'class':28s} {'true':>8s} {'trap':>6s} {'cross':>6s}")
for c, n in sorted(tot.items(), key=lambda x: -sum(x[1].values())):
    print(f"{c:28s} {n['true']:8,d} {n['trap']:6,d} {n['cross']:6,d}")
for c, ex in examples.items():
    print(f"\n{c}: {len(ex)} cross pairs")
    for e in ex[:40]:
        print("  ", e)
