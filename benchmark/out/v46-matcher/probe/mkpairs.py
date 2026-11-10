#!/usr/bin/env python3
"""pairs.tsv: sib (originals of one near-miss set, sets by name stem), true (an original
and 2 random SAME/CROP variants, traps out), other (an original and a random other set's)."""
import csv, re, random, os, collections, itertools
C = {"all": "/home/daniel/Documents/IMGS-ALL", "i5": "/home/daniel/Documents/IMGS5"}
TRAPS = {'column_roll', 'tile_shuffle', 'quadrant_rotate'}
def stem(seed):
    s = os.path.splitext(seed)[0]
    s = re.sub(r'[_ -]*\(?\d+\)?$', '', s)
    return s.lower().replace('-', '_')
rng = random.Random(46)
out = []
for tag, c in C.items():
    orig = {}
    for r in csv.DictReader(open(f"{c}/derived/manifest.csv")):
        if r["variant"] == "ORIGINAL": orig[r["seed"]] = r["path"]
    pos = collections.defaultdict(list)
    for r in csv.DictReader(open(f"{c}/derived/pairs.csv")):
        if r["variant_a"] == "ORIGINAL" and r["relation"] in ("SAME", "CROP") and r["variant_b"] not in TRAPS and r["variant_b"] != "ORIGINAL":
            pos[r["seed"]].append((r["variant_b"], r["b"]))
    sets = collections.defaultdict(list)
    for s in sorted(orig): sets[stem(s)].append(s)
    for st, ss in sets.items():
        for a, b in itertools.combinations(ss, 2):
            out.append((f"sib-{tag}", orig[a], orig[b]))
    seeds = sorted(orig)
    for s in seeds:
        for v, p in rng.sample(sorted(pos[s]), min(2, len(pos[s]))):
            out.append((f"true-{tag}:{v}", orig[s], p))
        o = rng.choice([x for x in seeds if stem(x) != stem(s)])
        out.append((f"other-{tag}", orig[s], orig[o]))
open("pairs.tsv", "w").write("".join("\t".join(r) + "\n" for r in out))
print(collections.Counter(r[0].split(':')[0] for r in out))
