#!/usr/bin/env python3
"""v43-flat06 cliffs (I, correlation 0.6), beside v42's baseline at the shipped 0.7."""
import os, re, sys
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "v42-flat-cliffs"))
D = os.path.dirname(os.path.abspath(__file__))
V42 = os.path.join(D, "..", "v42-flat-cliffs")
STEPS = [("default", "default")] + [(f"points{p}", f"points {p}") for p in (8, 6, 5, 4, 3)] \
    + [(f"overlap{o}", f"overlap {o}") for o in ("0.75", "0.65", "0.6", "0.5", "0.4", "0.3", "0.25")] \
    + [(f"k{k}", f"k {k}") for k in (100, 50, 25, 10)]

def parse(p):
    if not os.path.exists(p):
        return None
    t = open(p).read()
    m = re.search(r"F1 ([\d.]+)\s+precision ([\d.]+)%\s+recall ([\d.]+)%", t)
    f = re.search(r"FP: trap ([\d,]+)\s+cross-family ([\d,]+)", t)
    cross = [(int(c.replace(",", "")), a, b) for a, b, c in re.findall(r"cross (\S+(?: \S+)*?) <-> (\S+(?: \S+)*?): ([\d,]+) pairs", t)]
    big = sorted([c for c in cross if c[0] >= 300], reverse=True)
    return dict(f1=float(m[1]), p=float(m[2]), r=float(m[3]), trap=int(f[1].replace(",", "")), cross=int(f[2].replace(",", "")),
                merges=len(big), top=max(cross)[0] if cross else 0, names=[f"{os.path.basename(a).split('.')[0]}|{os.path.basename(b).split('.')[0]}" for _, a, b in big])

for tag in sys.argv[1:] or ["i5", "all"]:
    print(f"\n== {tag}\n{'step':<13}| {'0.71.0 (corr 0.7): F1   P%     R%    trap  cross  †  top':<58}| I + corr 0.6: F1   P%     R%    trap  cross  †  top")
    for n, label in STEPS:
        base = parse(os.path.join(V42, f"{tag}-base-{n}.txt"))
        new = parse(os.path.join(D, f"{tag}.analyse.txt" if n == "default" else f"c-{tag}-{n}.txt"))
        cells = []
        for r in (base, new):
            cells.append("-" * 56 if r is None else f"{r['f1']:.4f} {r['p']:6.2f} {r['r']:6.2f} {r['trap']:6d} {r['cross']:6d} {r['merges']:2d} {r['top']:6d}".ljust(56))
        print(f"{label:<13}| " + "| ".join(cells))
    print("merges:")
    for n, label in STEPS:
        new = parse(os.path.join(D, f"{tag}.analyse.txt" if n == "default" else f"c-{tag}-{n}.txt"))
        if new and new["names"]:
            print(f"  {label:<13} {', '.join(new['names'])}")
