#!/usr/bin/env python3
"""Which true pairs does --detail 1024 add over 512, and did 512 leave a trace
of them (a verdict in its --dump)? And how many files would a rule "describe
the finer level only of files a 512 verdict points at" touch?

gains.py CORPUS R512.json R1024.json DUMP512.csv
"""
import csv, json, math, sys
from collections import Counter, defaultdict

corpus, r512, r1024, d512 = sys.argv[1:5]
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score

manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
dims = {r["path"]: (int(r["width"] or 0), int(r["height"] or 0)) for r in manifest}
variant = {r["path"]: r["variant"] for r in manifest}


def area_factor(w, h):
    lo, hi = min(w, h), max(w, h)
    if lo == 0:
        return 1.0
    r = hi / lo
    return math.sqrt(0.75 * r) if r > 4 / 3 else 1.0


def finer(path, work=512, detail=1024):
    w, h = dims.get(path, (0, 0))
    af = area_factor(w, h)
    side = lambda g: round(g * af)
    return max(w, h) > side(work)


def found(p):
    return score.load_tool(p)[1]


f512, f1024 = found(r512), found(r1024)
gain = {k for k in (f1024 - f512) if k in pos}
lost = {k for k in (f512 - f1024) if k in pos}
fp_gain = {k for k in (f1024 - f512) if k not in pos and k not in skipped}
print(f"true pairs 512 {len(f512 & pos):,}  1024 {len(f1024 & pos):,}  gained {len(gain):,}  lost {len(lost):,}  false gained {len(fp_gain):,}")

# 512's verdicts: best per pair, with its scale a->b (working planes).
verd = {}
with open(d512) as fh:
    for r in csv.DictReader(fh):
        k = score.key(r["a"], r["b"])
        n = int(r["n_in"])
        s = float(r["scale"])
        if r["a"] != k[0]:
            s = 1 / s if s else s
        if k not in verd or n > verd[k][0]:
            verd[k] = (n, s, r["kind"])

finer_files = {p for p in dims if finer(p)}
print(f"files with a finer level at 1024: {len(finer_files):,} of {len(dims):,}")

# Which side of a gained pair has a finer level.
sides = Counter()
for a, b in gain:
    sides[(a in finer_files, b in finer_files)] += 1
print("gained pairs by (a finer, b finer):", dict(sides))

has = Counter()
by_t = defaultdict(Counter)
for k in gain:
    v = verd.get(k)
    t = variant.get(k[0]) if variant.get(k[0]) != "ORIGINAL" else variant.get(k[1])
    if v is None:
        has["no verdict"] += 1
        by_t[t]["none"] += 1
    else:
        n, s, kind = v
        tag = f"{kind} n_in>={10 if n >= 10 else 3}"
        has[tag] += 1
        by_t[t]["some"] += 1
print("512 trace of gained pairs:", dict(has))
print("by transform (top 25):")
for t, c in sorted(by_t.items(), key=lambda x: -sum(x[1].values()))[:25]:
    print(f"  {t:28s} {dict(c)}")

# Cost proxy: files a hint rule would give a finer level.
# Rule M: for every 512 verdict (n_in >= 3), the side whose content the other
# shows magnified (scale a->b > 1: b sees a's region larger, so a's finer
# level may help), if that side has a finer level at all.
def hinted(min_n, slack):
    out = set()
    for (a, b), (n, s, kind) in verd.items():
        if n < min_n or kind == "propagated":
            continue
        if s > 1 / slack and a in finer_files:
            out.add(a)
        if s < slack and b in finer_files:
            out.add(b)
    return out


for min_n, slack in [(3, 1.0), (3, 1.25), (3, 1.5), (10, 1.0)]:
    h = hinted(min_n, slack)
    cov = sum(1 for (a, b) in gain if (a in h and a in finer_files) or (b in h and b in finer_files))
    print(f"hint n_in>={min_n} magnified>{1/slack:.2f}: {len(h):,} files ({len(h)/max(1,len(finer_files)):.1%} of finer), covers {cov:,}/{len(gain):,} gained pairs (a hinted side has a level)")
