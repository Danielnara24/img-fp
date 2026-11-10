#!/usr/bin/env python3
"""Tables of v42-flat-cliffs: each bar's steps, base beside flat."""
import os, re, sys
D = os.path.dirname(os.path.abspath(__file__))
STEPS = [("default", "default")] + [(f"points{p}", f"points {p}") for p in (8, 6, 5, 4, 3)] \
    + [(f"overlap{o}", f"overlap {o}") for o in ("0.75", "0.65", "0.6", "0.5", "0.4", "0.3", "0.25")] \
    + [(f"corr{c}", f"corr {c}") for c in ("0.9", "0.8", "0.6", "0.5", "0.4", "0.3", "0.2")] \
    + [(f"k{k}", f"k {k}") for k in (100, 50, 25, 10)]

def read(tag, v, n):
    p = os.path.join(D, f"{tag}-{v}-{n}.txt")
    if not os.path.exists(p):
        return None
    t = open(p).read()
    m = re.search(r"F1 ([\d.]+)\s+precision ([\d.]+)%\s+recall ([\d.]+)%", t)
    f = re.search(r"FP: trap ([\d,]+)\s+cross-family ([\d,]+)", t)
    cross = [(int(c.replace(",", "")), a, b) for a, b, c in re.findall(r"cross (\S+(?: \S+)*?) <-> (\S+(?: \S+)*?): ([\d,]+) pairs", t)]
    perfect = re.search(r"perfect transforms: (\d+)", t)
    big = [c for c in cross if c[0] >= 300]
    top = max(cross) if cross else (0, "", "")
    return dict(f1=float(m[1]), p=float(m[2]), r=float(m[3]), trap=int(f[1].replace(",", "")), cross=int(f[2].replace(",", "")),
                merges=len(big), top=top, perfect=int(perfect[1]) if perfect else -1, names=[f"{os.path.basename(a).split('.')[0]}|{os.path.basename(b).split('.')[0]}" for _, a, b in sorted(big, reverse=True)])

for tag in sys.argv[1:] or ["i5", "all"]:
    print(f"\n== {tag}\n{'step':<13}| {'base: F1   P%     R%    trap  cross  †  top':<52}| flat: F1   P%     R%    trap  cross  †  top")
    for n, label in STEPS:
        row = [f"{label:<13}"]
        for v in ("base", "flat"):
            r = read(tag, v, n)
            row.append("-" * 50 if r is None else f"{r['f1']:.4f} {r['p']:6.2f} {r['r']:6.2f} {r['trap']:6d} {r['cross']:6d} {r['merges']:2d} {r['top'][0]:6d}".ljust(50))
        print("| ".join(row))
    print("\nmerges (seed pairs >= 300 false pairs):")
    for n, label in STEPS:
        for v in ("base", "flat"):
            r = read(tag, v, n)
            if r and r["names"]:
                print(f"  {label:<13} {v}: {', '.join(r['names'])}")
