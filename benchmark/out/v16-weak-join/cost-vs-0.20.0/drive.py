import sys, os, json
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench
S = os.path.dirname(os.path.abspath(__file__))
BIN = {"v020": os.path.join(os.path.dirname(S), "img-fp-v020"), "new": os.path.join(os.path.dirname(S), "img-fp-hub")}
ROOT = {"imgs": "/home/daniel/Documents/IMGS", "found": "/home/daniel/Downloads/archive"}
bench.OK_EXIT["r"] = {0, 2}
base = bench.baseline_temp(); ceiling = base + 3.0
print(f"idle {base:.1f} C, ceiling {ceiling:.1f} C", flush=True)
plan = [("imgs", b, w) for b, w in (("v020", 512), ("new", 512), ("v020", 384),
                                    ("new", 512), ("v020", 384), ("v020", 512),
                                    ("v020", 384), ("v020", 512), ("new", 512))] + \
       [("found", b, w) for b, w in (("v020", 512), ("new", 512), ("v020", 384),
                                     ("new", 512), ("v020", 384), ("v020", 512))]
res = []
for k, (corp, b, ws) in enumerate(plan):
    d = f"{S}/{k:02d}_{corp}_{b}_{ws}"; os.makedirs(d, exist_ok=True)
    cmd = [BIN[b], ROOT[corp], "-r", "-x", "*", "--no-cache", "--work-size", str(ws), "-o", f"{d}/r.json", "--format", "json"]
    m = bench.run_one("r", cmd, d, ROOT[corp], 60.0, ceiling, [])
    m.pop("mem_samples", None); m.update(build=b, ws=ws, slot=k, corpus=corp)
    m["pairs"] = len(json.load(open(f"{d}/r.json"))["pairs"]) if m["ok"] else None
    res.append(m)
    print(f"RESULT {k:02d} {corp} {b} {ws}: wall {m['wall_seconds']} s, cpu {m['cpu_seconds']} s, PSS {m['peak_pss_mb']} MB, {m['mhz_mean']} MHz, pairs {m['pairs']}, ok {m['ok']}", flush=True)
    os.remove(f"{d}/r.json") if os.path.exists(f"{d}/r.json") else None
    json.dump(res, open(f"{S}/results.json", "w"), indent=1)
print("COST DONE", flush=True)
