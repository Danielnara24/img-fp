#!/usr/bin/env python3
"""Enlargement limited to the working size: accuracy at every size it changes,
and the found corpus's pairs. usage: lim.py"""
import sys, os, re, json, subprocess, time
L = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(L))
import sweep
sweep.BIN = f"{L}/img-fp-lim"; sweep.RES = f"{L}/results.jsonl"; sweep.S = L
for d in ("out", "logs", "cache"):
    os.makedirs(f"{L}/{d}", exist_ok=True)
sweep.done = set()
if os.path.exists(sweep.RES):
    sweep.done = {json.loads(l)["key"] for l in open(sweep.RES)}

def found(ws):
    key = f"lim_found_{ws}"
    if key in sweep.done:
        return
    log = f"{L}/logs/{key}.log"
    cmd = ["/usr/bin/time", "-f", "TIME %e %U %S %M", sweep.BIN, "/home/daniel/Downloads/archive",
           "-r", "-x", "*", "--no-cache", "--work-size", str(ws), "-o", "/dev/null", "--format", "json"]
    with open(log, "w") as fh:
        rc = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT).returncode
    txt = open(log).read()
    m = re.search(r"(\d+) groups, (\d+) pairs over (\d+) images", txt)
    tm = re.search(r"TIME ([\d.]+) ([\d.]+) ([\d.]+) (\d+)", txt)
    row = dict(key=key, corpus="found", ws=ws, rc=rc, groups=int(m[1]), pairs=int(m[2]),
               wall=float(tm[1]), cpu=float(tm[2]) + float(tm[3]))
    open(sweep.RES, "a").write(json.dumps(row) + "\n"); sweep.done.add(key)
    print(f"{time.strftime('%H:%M:%S')} {key:<34} pairs {row['pairs']} | {row['wall']:.0f}s {row['cpu']:.0f}cpu", flush=True)

for ws in (384, 448, 320, 256, 192, 128, 96, 64):
    found(ws)
    cache = f"{L}/cache/c{ws}.bin"
    for c in ("imgs", "imgs2", "imgs3"):
        sweep.run(f"lim_{c}_{ws}_cold", c, ws, cache, {})
    sweep.run(f"lim_all_{ws}", "all", ws, cache, {})
    if os.path.exists(cache):
        os.remove(cache)
found(512)
print("LIM DONE", flush=True)
