#!/usr/bin/env python3
"""table2.py CORPUS PATTERN... : rows for r/<list>@<suffix>.json, truth loaded once."""
import csv, json, sys, os, glob
from collections import defaultdict
corpus = sys.argv[1]
def key(a, b): return (a, b) if a <= b else (b, a)
seed_of, var = {}, {}
for r in csv.DictReader(open(f"{corpus}/derived/manifest.csv")):
    seed_of[r["path"]] = r["seed"]; var[r["path"]] = r["variant"]
rel = {}
for p in csv.DictReader(open(f"{corpus}/derived/pairs.csv")):
    rel[key(p["a"], p["b"])] = p["relation"]
lists = {}
def score(t):
    l = t.split("@")[0]
    if l not in lists:
        files = set(open(f"lists/{l}.txt").read().split()) if False else set(x for x in open(f"lists/{l}.txt").read().split("\n") if x)
        pos = sum(1 for k, v in rel.items() if v in ("SAME", "CROP") and k[0] in files and k[1] in files)
        lists[l] = (files, pos)
    files, npos = lists[l]
    d = json.load(open(f"r/{t}.json"))
    found = {key(p["a"], p["b"]) for p in d["pairs"]}
    found = {k for k in found if k[0] in seed_of and k[1] in seed_of and rel.get(k) != "PARTIAL"}
    tp = sum(1 for k in found if rel.get(k) in ("SAME", "CROP"))
    fp = [k for k in found if rel.get(k) not in ("SAME", "CROP")]
    cross = [k for k in fp if seed_of[k[0]] != seed_of[k[1]]]
    by = defaultdict(int); orig = set()
    for a, b in cross:
        s = key(seed_of[a], seed_of[b]); by[s] += 1
        if var[a] == "ORIGINAL" and var[b] == "ORIGINAL": orig.add(s)
    P = tp / len(found) if found else 1; R = tp / npos if npos else 1
    F = 2*P*R/(P+R) if P+R else 0
    big = sum(1 for v in by.values() if v >= 300)
    return dict(t=t, n=len(files), f1=F, p=P, r=R, trap=len(fp)-len(cross), cross=len(cross), sp=len(by), orig=len(orig), big=big, by=by, origs=orig)
if __name__ == "__main__":
    print(f"{'run':<22} {'files':>6} {'F1':>7} {'P%':>6} {'R%':>6} {'trap':>5} {'cross':>6} {'seedp':>5} {'orig':>4} {'>=300':>5}  top")
    for pat in sys.argv[2:]:
        for f in sorted(glob.glob(f"r/{pat}.json"), key=lambda f: (f.split('@')[0], len(f), f)):
            t = os.path.basename(f)[:-5]; r = score(t)
            top = ", ".join(f"{a.split('.')[0].split('/')[-1]}|{b.split('.')[0].split('/')[-1]}:{n}{'*' if (a,b) in r['origs'] else ''}" for (a, b), n in sorted(r["by"].items(), key=lambda x: -x[1])[:5])
            print(f"{t:<22} {r['n']:>6} {r['f1']:>7.4f} {100*r['p']:>6.2f} {100*r['r']:>6.2f} {r['trap']:>5} {r['cross']:>6} {r['sp']:>5} {r['orig']:>4} {r['big']:>5}  {top}")
