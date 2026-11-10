#!/usr/bin/env python3
"""v62: HEAD against the hard floor, from batch.sh's outputs.
table.py   (run in v62-floor/)"""
import glob, os, re

def read(stem):
    s = open(stem + ".score").read()
    a = open(stem + ".an").read()
    row = [l for l in s.splitlines() if l.count("%") >= 3][0].split()
    tp, fp, fn = (int(x.replace(",", "")) for x in row[2:5])
    sib = int(row[-2].replace(",", "")); punrel = float(row[-1].rstrip("%"))
    m = re.search(r"FP: trap ([\d,]+)\s+cross-family ([\d,]+)\s+seed-pairs (\d+)\s+merges (\d+)", a)
    trap, cross, sp, merges = (int(x.replace(",", "")) for x in m.groups())
    perf = int(re.search(r"perfect transforms: (\d+)", a).group(1))
    big = len(re.findall(r"cross .*: ([\d,]+) pairs", a) and [x for x in re.findall(r"cross .*: ([\d,]+) pairs", a) if int(x.replace(",", "")) >= 300])
    P, R = tp / (tp + fp), tp / (tp + fn)
    return dict(tp=tp, fp=fp, P=100 * P, R=100 * R, F1=2 * P * R / (P + R), trap=trap, cross=cross, sib=sib, punrel=punrel, merges=merges, perf=perf, big=big)

def line(name, d):
    return (f"{name:>8} {d['F1']:.4f} {d['P']:6.2f} {d['R']:6.2f} {d['tp']:>10,} {d['trap']:>6,} {d['cross']:>7,} "
            f"{d['sib']:>7,} {d['punrel']:6.2f} {d['merges']:>4} {d['big']:>3} {d['perf']:>3}")

H = f"{'':>8} {'F1':>6} {'P%':>6} {'R%':>6} {'TP':>10} {'traps':>6} {'cross':>7} {'sibFP':>7} {'Punrel':>6} {'mrg':>4} {'300':>3} {'pt':>3}"
for c in ("all", "all2"):
    print(f"== {c}: correlation sweep (rows: bar, head then floor)\n{H}")
    curve = {}
    bars = sorted({re.sub(rf"sweep/{c}-([\d.]+)-.*", r"\1", p) for p in glob.glob(f"sweep/{c}-*.score")}, key=float)
    for b in bars:
        for v in ("head", "floor"):
            st = f"sweep/{c}-{b}-{v}"
            if os.path.exists(st + ".an"):
                d = read(st); curve.setdefault(v, []).append((float(b), d))
                print(line(f"{b} {v[0]}", d))
    # equal recall: floor's precision interpolated at each head point's recall
    print(f"\n   at HEAD's recall, floor's precision (linear between floor's bars) / traps / cross:")
    fl = sorted(curve["floor"], key=lambda x: x[1]["R"])
    for b, h in curve["head"]:
        r = h["R"]
        for (b0, d0), (b1, d1) in zip(fl, fl[1:]):
            if d0["R"] <= r <= d1["R"]:
                t = (r - d0["R"]) / (d1["R"] - d0["R"]) if d1["R"] > d0["R"] else 0
                ip = lambda k: d0[k] + t * (d1[k] - d0[k])
                print(f"   head {b:.2f} R {r:6.2f}: P head {h['P']:6.3f} floor {ip('P'):6.3f} (floor bar ~{b0 + t*(b1-b0):.3f}); "
                      f"traps {h['trap']:,} / {ip('trap'):,.0f}; cross {h['cross']:,} / {ip('cross'):,.0f}; sibFP {h['sib']:,} / {ip('sib'):,.0f}; perfect {h['perf']} / {ip('perf'):.1f}")
                break
        else:
            print(f"   head {b:.2f} R {r:6.2f}: outside floor's range")
    print()
    print(f"== {c}: cliffs\n{H}")
    for st in sorted({p[:-6] for p in glob.glob(f"cliffs/{c}-*.score")}):
        if st.endswith("head") and os.path.exists(st[:-4] + "floor.an"):
            for v in ("head", "floor"):
                s2 = st[:-4] + v
                print(line("", read(s2)), s2.split("/")[1])
    print()
