#!/usr/bin/env python3
"""acc.py NAME CORPUS WS AREA DEMOTE CACHE: one accuracy run (not timed) of
img-fp-exp, scored, appended to results.jsonl. Reports kept as out/NAME.json."""
import hashlib, json, os, subprocess, sys, time
S = os.path.dirname(os.path.abspath(__file__))
D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3", "imgs4": f"{D}/IMGS4",
        "all": f"{D}/IMGS-ALL", "elong": f"{D}/IMGS-ELONG", "found": "/home/daniel/Downloads/archive"}
name, corpus, ws, area, demote, cache = sys.argv[1:7]
os.makedirs(f"{S}/out", exist_ok=True); os.makedirs(f"{S}/caches", exist_ok=True)
out = f"{S}/out/{name}.json"
cmd = [f"{S}/img-fp-exp", CORP[corpus], "-r", "-x", "*", "--no-last", "--work-size", ws,
       "--cache", f"{S}/caches/{cache}.bin", "--format", "json", "-o", out]
t0 = time.time()
rc = subprocess.run(cmd, env={**os.environ, "IMGFP_AREA": area, "IMGFP_DEMOTE": demote},
                    stdout=subprocess.DEVNULL, stderr=open(f"{S}/out/{name}.log", "w")).returncode
row = dict(name=name, corpus=corpus, ws=int(ws), area=area, demote=demote, rc=rc, secs=round(time.time() - t0))
d = json.load(open(out))
pairs = sorted((min(x["a"], x["b"]), max(x["a"], x["b"])) for x in d["pairs"])
row.update(pairs=len(pairs), groups=len(d["groups"]), hash=hashlib.sha1(json.dumps(pairs).encode()).hexdigest()[:12])
del d, pairs
if corpus == "elong":
    T = json.load(open(f"{CORP['elong']}/truth.json"))
    pos = {tuple(sorted(x)) for x in T["positives"]}; part = {tuple(sorted(x)) for x in T["partial"]}
    f = {tuple(sorted((os.path.basename(x["a"]), os.path.basename(x["b"])))) for x in json.load(open(out))["pairs"]} - part
    row.update(tp=len(f & pos), fp=len(f - pos))
elif corpus != "found":
    r = subprocess.run([sys.executable, f"{S}/../v29-area/score1.py", out, corpus], capture_output=True, text=True)
    row.update(json.loads(r.stdout))
with open(f"{S}/results.jsonl", "a") as fh:
    fh.write(json.dumps(row) + "\n")
print(f"{name:<24} {row['hash']} pairs {row['pairs']:,} groups {row['groups']} "
      + (f"tp {row['tp']} fp {row['fp']}" if corpus == 'elong' else
         f"F1 {row.get('f1', 0):.4f} P {100*row.get('precision', 0):.2f} R {100*row.get('recall', 0):.2f} perfect {row.get('perfect')} trap {row.get('trap')} cross {row.get('cross')} {list(row.get('seedpairs', {}).items())[:3]}"), flush=True)
