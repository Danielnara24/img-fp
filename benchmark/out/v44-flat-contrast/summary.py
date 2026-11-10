#!/usr/bin/env python3
"""v44: the one-sided block scored by |r| x contrast term, at 0.7, beside
0.71.0 (v42 base) and I as r = 0 (v42 flat), same steps."""
import os, re, sys
D = os.path.dirname(os.path.abspath(__file__)); V42 = os.path.join(D, "..", "v42-flat-cliffs")
STEPS = [("default", "default")] + [(f"points{p}", f"points {p}") for p in (8, 6, 5, 4, 3)] \
    + [(f"overlap{o}", f"overlap {o}") for o in ("0.75", "0.65", "0.6", "0.5", "0.4", "0.3", "0.25")] \
    + [(f"corr{c}", f"corr {c}") for c in ("0.9", "0.8", "0.6", "0.5", "0.4", "0.3", "0.2")] \
    + [(f"k{k}", f"k {k}") for k in (100, 50, 25, 10)]
def parse(p):
    if not os.path.exists(p): return None
    t = open(p).read()
    m = re.search(r"F1 ([\d.]+)\s+precision ([\d.]+)%\s+recall ([\d.]+)%", t)
    if not m: return None
    f = re.search(r"FP: trap ([\d,]+)\s+cross-family ([\d,]+)", t)
    cross = [(int(c.replace(",", "")), a, b) for a, b, c in re.findall(r"cross (\S+(?: \S+)*?) <-> (\S+(?: \S+)*?): ([\d,]+) pairs", t)]
    big = sorted([c for c in cross if c[0] >= 300], reverse=True)
    pt = re.search(r"perfect transforms: (\d+)", t)
    return dict(f1=float(m[1]), p=float(m[2]), r=float(m[3]), trap=int(f[1].replace(",", "")), cross=int(f[2].replace(",", "")),
                merges=len(big), top=max(cross)[0] if cross else 0, perfect=int(pt[1]) if pt else -1,
                names=[f"{os.path.basename(a).split('.')[0]}|{os.path.basename(b).split('.')[0]}" for _, a, b in big])
fmt = lambda r: "-" * 52 if r is None else f"{r['f1']:.4f} {r['p']:6.2f} {r['r']:6.2f} {r['trap']:6d} {r['cross']:6d} {r['merges']:2d} {r['top']:5d} {r['perfect']:3d}"
for tag in sys.argv[1:] or ["i5", "all"]:
    print(f"\n== {tag}   (F1  P%  R%  trap  cross  merges  top  perfect)")
    for n, label in STEPS:
        rows = [("0.71.0", parse(f"{V42}/{tag}-base-{n}.txt")), ("I r=0", parse(f"{V42}/{tag}-flat-{n}.txt")), ("I contrast", parse(f"{D}/{tag}-ssim-{n}.txt"))]
        if n == "default":
            rows += [("I raw r", parse(f"{D}/{tag}-raw-{n}.txt")), ("I plane", parse(f"{D}/{tag}-plane-{n}.txt"))]
        for i, (v, r) in enumerate(rows):
            print(f"{label if i == 0 else '':<13} {v:<11} {fmt(r)}")
    print("merges:")
    for n, label in STEPS:
        for v, p in (("0.71.0", f"{V42}/{tag}-base-{n}.txt"), ("I r=0", f"{V42}/{tag}-flat-{n}.txt"), ("contrast", f"{D}/{tag}-ssim-{n}.txt")):
            r = parse(p)
            if r and r["names"]: print(f"  {label:<13} {v:<9} {', '.join(r['names'])}")
