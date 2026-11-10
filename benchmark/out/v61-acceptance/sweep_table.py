#!/usr/bin/env python3
"""Does a higher --min-pixel-correlation remove every sibling join? For each
bar: recall, unrelated false pairs, and per sibling set the false pairs, in
the small families (summed over n0, n1 x3, n2 x3) and the full corpus.

sweep_table.py CORPUS SWEEPDIR   (fam-<bar>/all2-*.json, full-<bar>.json)
"""
import glob, json, os, re, sys
from collections import Counter, defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score
from families import group_of

corpus, d = sys.argv[1:3]
manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
seed = {r["path"]: r["seed"] for r in manifest}


def tally(jsons):
    tp = fn = unrel = 0
    sib = Counter()
    for js in jsons:
        found = score.load_tool(js)[1]
        files = set(open(js[:-5] + ".list").read().split("\n")) - {""} if os.path.exists(js[:-5] + ".list") else None
        p = {k for k in pos if files is None or (k[0] in files and k[1] in files)}
        tp += len(found & p)
        fn += len(p - found)
        for k in found - pos - skipped:
            sa, sb = seed[k[0]], seed[k[1]]
            if sa == sb:
                continue
            if group_of(sa) == group_of(sb):
                sib[tuple(sorted((sa, sb)))] += 1
            else:
                unrel += 1
    return tp / max(1, tp + fn), unrel, sib


bars = sorted({re.sub(r".*-(0\.\d+)(\.json)?$", r"\1", p) for p in glob.glob(os.path.join(d, "fam-*")) + glob.glob(os.path.join(d, "full-*.json"))}, key=float)
fam, full = {}, {}
for b in bars:
    fj = sorted(glob.glob(os.path.join(d, f"fam-{b}", "all2-n*.json")))
    if fj:
        fam[b] = tally(fj)
    if os.path.exists(os.path.join(d, f"full-{b}.json")):
        full[b] = tally([os.path.join(d, f"full-{b}.json")])
extra = {}
for b, path in [a.split("=") for a in sys.argv[3:]]:
    full[b] = tally([path])
bars = sorted(set(fam) | set(full), key=float)
for name, t in (("small families (7 subsets)", fam), ("full corpus", full)):
    print(f"== {name}")
    print(f"{'bar':>5} {'recall':>8} {'unrel FP':>9} {'sib sets':>9} {'sib FP':>8}")
    for b in bars:
        if b in t:
            r, u, s = t[b]
            print(f"{b:>5} {100*r:7.2f}% {u:9,d} {len(s):9d} {sum(s.values()):8,d}")
    sets = sorted({k for b in t for k in t[b][2]}, key=lambda k: -max(t[b][2].get(k, 0) for b in t))
    print(f"\n{'sibling set':52s} " + " ".join(f"{b:>6}" for b in bars if b in t) + "  gone from")
    for k in sets:
        row = [t[b][2].get(k, 0) for b in bars if b in t]
        gone = next((b for b in [b for b in bars if b in t] if all(t[c][2].get(k, 0) == 0 for c in bars if c in t and float(c) >= float(b))), "never")
        print(f"{' | '.join(k)[:52]:52s} " + " ".join(f"{v:6d}" for v in row) + f"  {gone}")
    print()
