#!/usr/bin/env python3
"""results.jsonl -> cliffs.txt tables (F1, cross-family pairs; † = a seed pair >= 300)."""
import json, os
S = os.path.dirname(os.path.abspath(__file__))
R = {r["key"]: r for r in map(json.loads, open(f"{S}/results.jsonl"))}
C = ["imgs", "imgs2", "imgs3", "imgs4", "all"]
DEF = dict(ap=10, ov=0.85, corr=0.6, k=150)
SW = dict(ap=[3, 5, 6, 7, 8, 9, 10, 12], ov=[0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.85],
          corr=[0.2, 0.4, 0.5, 0.55, 0.6, 0.65, 0.7, 0.8], k=[10, 25, 50, 100, 150, 300])
def key(c, ws, p, v):
    return f"cl_{c}_{ws}_def" if v == DEF[p] else f"cl_{c}_{ws}_{p}{v}"
out, merges, strays = [], [], []
for p, vals in SW.items():
    out.append(f"\n== {p}\nval   " + "".join(f"{ws}:{c:<12}" for ws in (512, 640) for c in C))
    for v in vals:
        line = f"{v:<6}"
        for ws in (512, 640):
            for c in C:
                r = R.get(key(c, ws, p, v))
                if not r or "f1" not in r:
                    line += f"{'-':<16}"; continue
                big = {k: n for k, n in r["seedpairs"].items() if n >= 300}
                mid = {k: n for k, n in r["seedpairs"].items() if 50 <= n < 300}
                line += f"{r['f1']:.4f}{'†' if big else ' '}{r['cross']:<9}"
                if big: merges.append(f"{c} {ws} {p}{v} {sorted(big.items(), key=lambda x: -x[1])}")
                if mid: strays.append(f"{c} {ws} {p}{v} {sorted(mid.items(), key=lambda x: -x[1])}")
        out.append(line)
out.append("\n== merges (>=300 between two seeds)"); out += sorted(set(merges))
out.append("\n== stray files (seed pair 50-299)"); out += sorted(set(strays))
out.append("\n== defaults")
for ws in (512, 640):
    for c in C:
        r = R.get(f"cl_{c}_{ws}_def")
        if r and "f1" in r:
            out.append(f"{c} {ws}: F1 {r['f1']:.4f} P {r['precision']:.4f} R {r['recall']:.4f} "
                       f"trap {r['trap']} cross {r['cross']} perfect {r.get('perfect')} pairs {r['pairs']} hash {r['pairs_hash']}")
print("\n".join(out))
