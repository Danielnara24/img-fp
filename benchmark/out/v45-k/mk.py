#!/usr/bin/env python3
"""lists/<tag>-n<N>-d<D>.txt: every seed's original + N of its variants drawn
at random (draw D); lists/<tag>-f<M>-d<D>.txt: the originals + M found-corpus
photos drawn at random."""
import csv, random, os, glob
C = {"i5": "/home/daniel/Documents/IMGS5", "all": "/home/daniel/Documents/IMGS-ALL"}
found = sorted(glob.glob("/home/daniel/Downloads/archive/**/*.*", recursive=True))
for tag, corpus in C.items():
    fam = {}
    for r in csv.DictReader(open(f"{corpus}/derived/manifest.csv")):
        fam.setdefault(r["seed"], [None, []])
        if r["variant"] == "ORIGINAL": fam[r["seed"]][0] = r["path"]
        else: fam[r["seed"]][1].append(r["path"])
    for n in (0, 1, 2, 5, 10, 30, 90):
        for d in ((0,) if n in (0, 90) else (1, 2, 3)):
            rng = random.Random(f"{tag}-{n}-{d}")
            out = []
            for s in sorted(fam):
                o, vs = fam[s]
                out.append(o); out += rng.sample(sorted(vs), min(n, len(vs)))
            open(f"lists/{tag}-n{n}-d{d}.txt", "w").write("\n".join(out) + "\n")
    for m in (300, 1000, 3000, 9285):
        for d in ((0,) if m == 9285 else (1, 2)):
            rng = random.Random(f"{tag}-f{m}-{d}")
            out = [fam[s][0] for s in sorted(fam)] + rng.sample(found, m)
            open(f"lists/{tag}-f{m}-d{d}.txt", "w").write("\n".join(out) + "\n")
