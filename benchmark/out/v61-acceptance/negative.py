#!/usr/bin/env python3
"""Is there negative evidence? Per seed pair (cross-family) and per seed
(same family), count positive verdicts (anchor or better) and verdicts
whose geometry stands (points, centred, overlap) but whose pixels disagree.

negative.py CORPUS DUMP.csv
"""
import csv, sys
from collections import Counter, defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score

corpus, dump = sys.argv[1:3]
manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
seed = {r["path"]: r["seed"] for r in manifest}
NEG_BLK = float(sys.argv[3]) if len(sys.argv) > 3 else 0.5

posc, negc = Counter(), Counter()
negtrue = Counter()
with open(dump) as fh:
    for r in csv.DictReader(fh):
        if r["kind"] == "propagated":
            continue
        n, blk, bmin = int(r["n_in"]), float(r["blk"]), float(r["blk_min"])
        ov = max(float(r["ov_a"]), float(r["ov_b"]))
        geo = n >= 10 and r["centred"] == "1" and ov >= 0.85
        sa, sb = seed[r["a"]], seed[r["b"]]
        g = (sa, sb) if sa != sb else (sa,)
        if sa != sb:
            g = tuple(sorted(g))
        if geo and blk >= 0.7:
            posc[g] += 1
        elif geo and blk < NEG_BLK:
            negc[g] += 1
            if score.key(r["a"], r["b"]) in pos:
                negtrue[g] += 1

cross = [g for g in posc if len(g) == 2]
fam = [g for g in posc if len(g) == 1]
print(f"negatives: geometry stands, blk < {NEG_BLK}")
print("cross-family seed pairs with any positive:", len(cross))
big = sorted(cross, key=lambda g: -posc[g])[:30]
print(f"{'seed pair':55s} {'pos':>6s} {'neg':>6s}")
for g in big:
    print(f"{' | '.join(g):55s} {posc[g]:6d} {negc[g]:6d}")
fneg = sorted(fam, key=lambda g: -negc[g])
tot_true_neg = sum(negtrue.values())
print()
print("same-family: seeds", len(fam), " positives", sum(posc[g] for g in fam), " negatives", sum(negc[g] for g in fam), " of which true pairs (missed copies)", tot_true_neg)
print("families with most negatives:")
for g in fneg[:12]:
    print(f"  {g[0]:40s} pos {posc[g]:6d} neg {negc[g]:5d} (true {negtrue[g]})")
