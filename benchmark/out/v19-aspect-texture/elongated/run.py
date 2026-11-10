import json, subprocess, os, sys, re
B = "../img-fp-exp"; D = "/home/daniel/Documents/IMGS-ELONG"
T = json.load(open(f"{D}/truth.json"))
pos = {tuple(sorted(p)) for p in T["positives"]}; part = {tuple(sorted(p)) for p in T["partial"]}
rows = []
for ws in [384, 512, 640, 1024]:
    for area in ["0", "1"]:
        out = f"/tmp/claude-1000/elong_{ws}_{area}.json"
        r = subprocess.run(["/usr/bin/time", "-f", "TIME %e %U %S", B, D, "--no-cache", "--work-size", str(ws), "-o", out],
                           env={**os.environ, "AREA": area}, capture_output=True, text=True)
        tm = re.search(r"TIME ([\d.]+) ([\d.]+) ([\d.]+)", r.stderr)
        found = {tuple(sorted((os.path.basename(p["a"]), os.path.basename(p["b"])))) for p in json.load(open(out))["pairs"]}
        found -= part
        tp = found & pos; fp = found - pos
        kinds = {"section": sum(1 for p in tp if "section" in p[0] + p[1] and "page" in p[0] + p[1]),
                 "viewport": sum(1 for p in tp if "viewport" in p[0] + p[1] and "page" in p[0] + p[1])}
        # recall by page aspect
        by = {}
        for a, b in pos:
            if "page" not in a + b: continue
            k = a.split("_")[1]
            h = by.setdefault(k[0] + ("2-4" if int(k[1:]) <= 4 else "5+"), [0, 0]); h[1] += 1; h[0] += (a, b) in found
        print(f"ws {ws:4} AREA={area}: found {len(tp)}/{len(pos)} (sections {kinds['section']}/33, viewports {kinds['viewport']}/33) false {len(fp)} "
              f"| " + " ".join(f"{k}:{v[0]}/{v[1]}" for k, v in sorted(by.items())) + f" | cpu {float(tm[2])+float(tm[3]):.0f}s", flush=True)
