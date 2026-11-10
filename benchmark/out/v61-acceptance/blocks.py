#!/usr/bin/env python3
"""v61 A: does the pixel check's block map separate sibling photographs from
edits the truth calls copies? Reads dumps with the scaffolding's `blocks` /
`kinds` columns (36 blocks, 6 x 6, row by row).

blocks.py CORPUS DUMP.csv [DUMP.csv ...]
"""
import csv, math, sys
from collections import Counter, defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score

corpus, dumps = sys.argv[1], sys.argv[2:]
manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
seed = {r["path"]: r["seed"] for r in manifest}
var = {r["path"]: r["variant"] for r in manifest}
N = 6

def comps(cells):
    """Sizes of 4-connected components of a set of (row, col)."""
    left, out = set(cells), []
    while left:
        stack = [left.pop()]
        n = 0
        while stack:
            y, x = stack.pop()
            n += 1
            for d in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                c = (y + d[0], x + d[1])
                if c in left:
                    left.remove(c)
                    stack.append(c)
        out.append(n)
    return sorted(out, reverse=True)

rows = []
bad_check = 0
seen = set()
for d in dumps:
    with open(d) as fh:
        for r in csv.DictReader(fh):
            if not r.get("blocks") or r["kind"] != "direct":
                continue
            k = score.key(r["a"], r["b"])
            n, blk, bmin = int(r["n_in"]), float(r["blk"]), float(r["blk_min"])
            oa, ob = float(r["ov_a"]), float(r["ov_b"])
            if not (n >= 10 and r["centred"] == "1" and max(oa, ob) >= 0.85 and blk >= 0.7):
                continue
            vals = [float(x) if x else None for x in r["blocks"].split(";")]
            kinds = r["kinds"]
            scored = [(i, v) for i, v in enumerate(vals) if v is not None and kinds[i] in "nf"]
            if not scored:
                continue
            m = sum(v for _, v in scored) / len(scored)
            if abs(m - blk) > 2e-3 or abs(min(v for _, v in scored) - bmin) > 2e-3:
                bad_check += 1
            if (k, r["kind"]) in seen:
                continue
            seen.add((k, r["kind"]))
            lab = "true" if k in pos else "skip" if k in skipped else ("trap" if seed[k[0]] == seed[k[1]] else "cross")
            if lab == "skip":
                continue
            rs = sorted(v for _, v in scored)
            f = {}
            f["blk"], f["blk_min"], f["nblk"] = blk, bmin, len(scored)
            for t in (0.3, 0.5, 0.7):
                low = [(i // N, i % N) for i, v in scored if v < t]
                f[f"low{t}"] = len(low)
                f[f"frac{t}"] = len(low) / len(scored)
                cs = comps(low)
                f[f"comp{t}"] = cs[0] if cs else 0           # largest connected low region
                f[f"ncomp{t}"] = len(cs)
                f[f"compact{t}"] = (cs[0] / len(low)) if low else 1.0
            hi = [v for _, v in scored if v >= 0.5]
            f["hi_mean"] = sum(hi) / len(hi) if hi else 0.0
            f["med"] = rs[len(rs) // 2]
            f["q25"] = rs[len(rs) // 4]
            f["gap"] = f["med"] - bmin                     # local dip against the rest
            f["oneflat"] = kinds.count("f")
            f["clean"] = bmin >= 0.9
            f["two"] = min(oa, ob) >= 0.85
            t = var[k[0]] if var[k[0]] != "ORIGINAL" else var[k[1]]
            rows.append((lab, t, f, (seed[k[0]], seed[k[1]]), r["kind"]))

print(f"maps checked: {len(rows):,} anchors (deduped); maps disagreeing with blk / blk_min: {bad_check}")
nc = [x for x in rows if not x[2]["clean"]]
cnt = Counter(x[0] for x in nc)
print("non-clean anchors:", dict(cnt))

def q(vals, p):
    vals = sorted(vals)
    return vals[min(len(vals) - 1, int(p * len(vals)))] if vals else float("nan")

feats = ["blk", "blk_min", "med", "q25", "gap", "hi_mean", "low0.3", "frac0.3", "comp0.3", "compact0.3", "low0.5", "frac0.5", "comp0.5", "ncomp0.5", "low0.7", "frac0.7", "oneflat"]
print(f"\n{'feature':12s} {'true p10/p50/p90':>24s} {'cross p10/p50/p90':>24s}  best single cut: cross removed at true kept 95% / 99%")
for fe in feats:
    tv = [x[2][fe] for x in nc if x[0] == "true"]
    cv = [x[2][fe] for x in nc if x[0] == "cross"]
    # either direction: cut that keeps 95% / 99% of true and removes most cross
    res = []
    for keep in (0.95, 0.99):
        lo = q(tv, 1 - keep)   # keep values >= lo
        hi = q(tv, keep)       # keep values <= hi
        rm_lo = sum(1 for v in cv if v < lo) / max(1, len(cv))
        rm_hi = sum(1 for v in cv if v > hi) / max(1, len(cv))
        res.append(f"{max(rm_lo, rm_hi):.0%}{'<' if rm_lo >= rm_hi else '>'}")
    print(f"{fe:12s} {q(tv,.1):7.3f} {q(tv,.5):7.3f} {q(tv,.9):7.3f}  {q(cv,.1):7.3f} {q(cv,.5):7.3f} {q(cv,.9):7.3f}   {res[0]:>6s} / {res[1]:>6s}")

# Which true transforms look most like siblings: by the most separating feature later.
print("\ntrue non-clean anchors by transform (count, median comp0.5, median frac0.5, median blk_min):")
bt = defaultdict(list)
for lab, t, f, s, kind in nc:
    if lab == "true":
        bt[t].append(f)
for t, fs in sorted(bt.items(), key=lambda x: -len(x[1]))[:30]:
    print(f"  {t:24s} {len(fs):6d}  {q([f['comp0.5'] for f in fs], .5):4.0f}  {q([f['frac0.5'] for f in fs], .5):.2f}  {q([f['blk_min'] for f in fs], .5):.3f}")
print("\ncross non-clean anchors by seed pair:")
bs = defaultdict(list)
for lab, t, f, s, kind in nc:
    if lab == "cross":
        bs[tuple(sorted(s))].append(f)
for s, fs in sorted(bs.items(), key=lambda x: -len(x[1]))[:25]:
    print(f"  {' | '.join(s):55s} {len(fs):5d}  {q([f['comp0.5'] for f in fs], .5):4.0f}  {q([f['frac0.5'] for f in fs], .5):.2f}  {q([f['blk_min'] for f in fs], .5):.3f}")

# Two features at once: drop a non-clean anchor when f1 < a or f2 > b (or <),
# best cross removed with 95% / 99% of true kept, over every pair of cuts.
def two(f1, f2, dir2, keep):
    tv = [(x[2][f1], x[2][f2]) for x in nc if x[0] == "true"]
    cv = [(x[2][f1], x[2][f2]) for x in nc if x[0] == "cross"]
    c1 = sorted({v for v, _ in tv})[:: max(1, len(tv) // 60)]
    c2 = sorted({v for _, v in tv})
    c2 = c2[:: max(1, len(c2) // 60)] + [c2[-1]]
    best = (0, None)
    drop = (lambda v, b: v > b) if dir2 == ">" else (lambda v, b: v < b)
    for a in c1:
        for b in c2:
            kt = sum(1 for x, y in tv if not (x < a or drop(y, b))) / len(tv)
            if kt < keep:
                continue
            rc = sum(1 for x, y in cv if x < a or drop(y, b)) / len(cv)
            if rc > best[0]:
                best = (rc, (round(a, 3), round(b, 3)))
    return best

print("\ntwo features (drop when blk < a or f2 beyond b): cross removed at true kept 95% / 99%")
for f2, d in [("frac0.7", ">"), ("ncomp0.5", ">"), ("q25", "<"), ("blk_min", "<"), ("comp0.5", ">"), ("gap", ">")]:
    print(f"  blk + {f2:9s} {two('blk', f2, d, .95)}  {two('blk', f2, d, .99)}")
