import sys, os, json
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench
S = os.path.dirname(os.path.abspath(__file__))
BIN = os.path.join(os.path.dirname(S), "img-fp-lim")
IMGS = "/home/daniel/Documents/IMGS"; FOUND = "/home/daniel/Downloads/archive"
bench.OK_EXIT["w"] = {0, 2}
base = bench.baseline_temp(); ceiling = base + 3.0
print(f"idle {base:.1f} C, ceiling {ceiling:.1f} C", flush=True)
res = []
for k, (corp, ws) in enumerate([("imgs", w) for w in (512, 128, 384, 640, 256, 448, 64, 320, 192, 512)] + [("found", w) for w in (448, 128, 384, 256, 512)]):
    root = IMGS if corp == "imgs" else FOUND
    d = f"{S}/{corp}_w{ws}_{k}"; os.makedirs(d, exist_ok=True)
    cmd = [BIN, root, "-r", "-x", "*", "--no-cache", "--work-size", str(ws), "-o", f"{d}/o.json", "--format", "json"]
    m = bench.run_one("w", cmd, d, root, 60.0, ceiling, [])
    m.pop("mem_samples", None); m.update(ws=ws, slot=k, corpus=corp); res.append(m)
    print(f"RESULT {corp} ws {ws} slot {k}: wall {m['wall_seconds']} s, cpu {m['cpu_seconds']} s, PSS {m['peak_pss_mb']} MB, {m['mhz_mean']} MHz, ok {m['ok']}", flush=True)
    try: os.remove(f"{d}/o.json")
    except OSError: pass
    json.dump(res, open(f"{S}/results.json", "w"), indent=1)
print("COST DONE", flush=True)
