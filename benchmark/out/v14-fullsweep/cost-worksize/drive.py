import sys, os, json
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench
S = os.path.dirname(os.path.abspath(__file__))
BIN = os.path.join(os.path.dirname(S), "img-fp-v20")
IMGS = "/home/daniel/Documents/IMGS"
bench.OK_EXIT["w"] = {0, 2}
base = bench.baseline_temp(); ceiling = base + 3.0
print(f"idle {base:.1f} C, ceiling {ceiling:.1f} C", flush=True)
res = []
for k, ws in enumerate([384, 256, 512, 128, 640, 320, 448, 192, 384, 256]):
    d = f"{S}/w{ws}_{k}"; os.makedirs(d, exist_ok=True)
    cmd = [BIN, IMGS, "-r", "-x", "*", "--no-cache", "--work-size", str(ws), "-o", f"{d}/o.json", "--format", "json"]
    m = bench.run_one("w", cmd, d, IMGS, 60.0, ceiling, [])
    m.pop("mem_samples", None); m.update(ws=ws, slot=k); res.append(m)
    print(f"RESULT ws {ws} slot {k}: wall {m['wall_seconds']} s, cpu {m['cpu_seconds']} s, PSS {m['peak_pss_mb']} MB, {m['mhz_mean']} MHz, ok {m['ok']}", flush=True)
    try: os.remove(f"{d}/o.json")
    except OSError: pass
    json.dump(res, open(f"{S}/results.json", "w"), indent=1)
print("COST DONE", flush=True)
