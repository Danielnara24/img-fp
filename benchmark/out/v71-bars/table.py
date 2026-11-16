#!/usr/bin/env python3
"""v71: each bar swept alone about the defaults, from batch.sh's outputs.
table.py   (run in v71-bars/)"""
import json, os, re
def an(stem):
    a = open(stem + ".an").read()
    f1, p, r = re.search(r"F1 ([\d.]+)\s+precision ([\d.]+)%\s+recall ([\d.]+)%", a).groups()
    tp = re.search(r"TP ([\d,]+)", a).group(1)
    trap, cross, sp, mrg = re.search(r"FP: trap ([\d,]+)\s+cross-family ([\d,]+)\s+seed-pairs (\d+)\s+merges (\d+)", a).groups()
    big = [(m.group(1), int(m.group(2).replace(",", ""))) for m in re.finditer(r"cross (.*): ([\d,]+) pairs", a)]
    big = [b for b in big if b[1] >= 300]
    perf = re.search(r"perfect transforms: (\d+)", a).group(1)
    return f1, p, r, tp, trap, cross, mrg, len(big), perf
SETS = [("correlation", "c", ["0.6", "0.65", "0.7", "0.75", "0.8"], "c0.7"),
        ("overlap", "o", ["0.75", "0.8", "0.85", "0.9"], "o0.85"),
        ("points", "p", [str(i) for i in range(3, 15)], "p10")]
for name, k, vals, dflt in SETS:
    print(f"== {name}")
    print(f"{'':>6} | " + " | ".join(f"{c:^58}" for c in ("IMGS-ALL", "IMGS-ALL2", "IMGS-ALL3")) + " | DISC21 shard 0 (true / false / uAP / R@P90)")
    print(f"{'':>6} | " + " | ".join(f"{'F1':>6} {'P%':>6} {'R%':>6} {'TP':>10} {'traps':>6} {'cross':>7} {'300':>3} {'pt':>3}" + " " * 3 for _ in range(3)) + " |")
    f1sum = {}
    for v in vals:
        t = k + v
        if t == dflt: t = "c0.7"
        cells = []
        for c in ("all", "all2", "all3"):
            f1, p, r, tp, trap, cross, mrg, big, perf = an(f"sweep/{c}-{t}")
            cells.append(f"{f1} {p:>6} {r:>6} {tp:>10} {trap:>6} {cross:>7} {big:>3} {perf:>3}   ")
            f1sum[v] = f1sum.get(v, 0) + float(f1)
        d = json.load(open(f"disc/{t}.score")) if os.path.exists(f"disc/{t}.score") else None
        ds = f"{d['true']} / {d['false']} / {d['uap']:.2f} / {d['r90']:.2f}" if d else "-"
        print(f"{v:>6} | " + " | ".join(cells) + f" | {ds}   mean F1 {f1sum[v] / 3:.4f}" + ("  <- default" if k + v == dflt else ""))
    print()
