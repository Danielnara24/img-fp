#!/usr/bin/env python3
"""The sweep's table: one row per run of results.jsonl, IMGS-ALL at 512."""
import json
rows = [json.loads(l) for l in open("results.jsonl")]
order = ["def_all"] + [f"{p}{v}_all" for p, vs in [("thumb", [64, 96, 192, 256]), ("grid", [32, 40, 56, 64, 80]),
                                                     ("block", [4, 6, 12, 16]), ("flat", [0, 1, 2, 3, 6, 8, 12])] for v in vs]
by = {r["key"]: r for r in rows}
print("| run | F1 | precision | recall | perfect | trap | cross-family | halves alt / hash | largest seed pairs | peak MB |")
print("|---|---|---|---|---|---|---|---|---|---|")
for k in order:
    r = by.get(k)
    if not r:
        continue
    sp = sorted(r.get("seedpairs", {}).items(), key=lambda kv: -kv[1])[:2]
    sp = ", ".join(f"{a.split('|')[0].split('/')[-1]}/{a.split('|')[1].split('/')[-1]} {n}" for a, n in sp if n >= 50)
    print(f"| {k[:-4]} | {r['f1']:.4f} | {100*r['precision']:.2f}% | {100*r['recall']:.2f}% | {r['perfect']} | {r['trap']:,} | "
          f"{r['cross']:,}{' †' if any(n >= 300 for n in r['seedpairs'].values()) else ''} | {r['halves']['alt']:.4f} / {r['halves']['hash']:.4f} | {sp} | {r['peak_pss_swap']} |")
