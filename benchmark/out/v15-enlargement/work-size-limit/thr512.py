#!/usr/bin/env python3
"""Every threshold at the new default, --work-size 512."""
import sys, os, json
L = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(L))
import sweep
sweep.BIN = f"{L}/img-fp-lim"; sweep.RES = f"{L}/results.jsonl"; sweep.S = L
sweep.done = {json.loads(l)["key"] for l in open(sweep.RES)} if os.path.exists(sweep.RES) else set()
cache = f"{L}/cache/c512.bin"
for c in ("imgs", "imgs2", "imgs3"):
    sweep.run(f"lim_{c}_512_cold", c, 512, cache, {})
sweep.run("lim_all_512", "all", 512, cache, {})
SW = dict(sweep.SWEEP)
SW["corr"] = SW["corr"] + [0.56, 0.57, 0.58, 0.59, 0.61, 0.62, 0.63, 0.64, 0.66, 0.67, 0.68, 0.69, 0.75]
for p, vals in SW.items():
    for v in vals:
        for c in ("imgs", "imgs2", "imgs3", "all"):
            if p == "corr" and v in (0.56, 0.57, 0.58, 0.59, 0.61, 0.62, 0.63, 0.64, 0.66, 0.67, 0.68, 0.69, 0.75) and c in ("imgs", "all"):
                continue
            sweep.run(f"thr_{c}_512_{p}{v}", c, 512, cache, {p: v})
print("THR512 DONE", flush=True)
