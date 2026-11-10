#!/usr/bin/env python3
"""Score an img-fp report on a subset of a generated corpus.

    kscore.py REPORT.json --corpus=DIR [--list=FILES.txt] [--quiet]

Truth is analyse.py's (SAME/CROP positive, PARTIAL skipped), restricted to
pairs whose two files are both in the list (default: every file of the corpus).
Also: cross-family pairs by seed pair, which of them join the two ORIGINALS,
and groups holding files of more than one seed.
"""
import csv, json, os, sys
from collections import defaultdict

args = [a for a in sys.argv[1:] if not a.startswith("--")]
opt = dict(a[2:].split("=", 1) for a in sys.argv[1:] if a.startswith("--") and "=" in a)
quiet = "--quiet" in sys.argv
corpora = [a.split("=", 1)[1] for a in sys.argv[1:] if a.startswith("--corpus=")]

def key(a, b): return (a, b) if a <= b else (b, a)

seed_of, variant_of, positives, skipped = {}, {}, set(), set()
for c in corpora:
    tag = os.path.basename(os.path.normpath(c)) + "/" if len(corpora) > 1 else ""
    for r in csv.DictReader(open(os.path.join(c, "derived", "manifest.csv"))):
        seed_of[r["path"]] = tag + r["seed"]; variant_of[r["path"]] = r["variant"]
    pairs_csv = os.path.join(c, "derived", "pairs.csv")
    files = set(open(opt["list"]).read().split("\n")) - {""} if "list" in opt else None
    for p in csv.DictReader(open(pairs_csv)):
        if files is not None and (p["a"] not in files or p["b"] not in files):
            continue
        k = key(p["a"], p["b"])
        if p["relation"] == "PARTIAL": skipped.add(k)
        elif p["relation"] in ("SAME", "CROP"): positives.add(k)
files = set(open(opt["list"]).read().split("\n")) - {""} if "list" in opt else set(seed_of)

out = []
for path in args:
    d = json.load(open(path))
    found = {key(p["a"], p["b"]) for p in d["pairs"]}
    unknown = {k for k in found if k[0] not in files or k[1] not in files
               or k[0] not in seed_of or k[1] not in seed_of}
    found -= unknown | skipped
    tp = len(found & positives); fp = found - positives
    trap = {k for k in fp if seed_of[k[0]] == seed_of[k[1]]}
    cross = fp - trap
    by = defaultdict(int); orig = set()
    for a, b in cross:
        sp = key(seed_of[a], seed_of[b]); by[sp] += 1
        if variant_of[a] == "ORIGINAL" and variant_of[b] == "ORIGINAL": orig.add(sp)
    mixed = 0
    for g in d["groups"]:
        if len({seed_of.get(f["path"]) for f in g["files"]}) > 1: mixed += 1
    P = tp / len(found) if found else 1.0
    R = tp / len(positives) if positives else 1.0
    F = 2 * P * R / (P + R) if P + R else 0
    row = dict(file=os.path.basename(path), n=len(files), pos=len(positives), found=len(found), tp=tp,
               f1=F, p=P, r=R, trap=len(trap), cross=len(cross), seedpairs=len(by),
               origpairs=len(orig), groups=len(d["groups"]), mixed=mixed,
               by=dict(sorted(((f"{a}|{b}", n) for (a, b), n in by.items()), key=lambda x: -x[1])),
               orig=sorted(f"{a}|{b}" for a, b in orig), unknown=len(unknown))
    out.append(row)
    if not quiet:
        print(f"{row['file']}: files {row['n']} F1 {F:.4f} P {100*P:.2f}% R {100*R:.2f}% "
              f"TP {tp} trap {len(trap)} cross {len(cross)} seed-pairs {len(by)} "
              f"(originals {len(orig)}) groups {row['groups']} mixed {mixed}")
        for sp, n in list(row["by"].items())[:30]:
            print(f"    {sp}: {n}{'  [originals]' if sp in row['orig'] else ''}")
if "json" in opt:
    json.dump(out, open(opt["json"], "w"), indent=1)
