#!/usr/bin/env python3
"""Cliffs for 0.51.0 (img-fp-ship = the plain build at 9b243c1), at 512 and 640,
on the four corpora and IMGS-ALL. Accuracy only: each corpus and working size
has its own cache, made by its default run; the sweeps reuse it (matching
options are not analysis settings). Same values as out/v31-cost-cliff.

usage: run.py PHASE...   cliff512 cliff640
"""
import hashlib, json, os, subprocess, sys, time
S = os.path.dirname(os.path.abspath(__file__))
D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3", "imgs4": f"{D}/IMGS4",
        "all": f"{D}/IMGS-ALL"}
DEF = dict(ap=10, ov=0.85, corr=0.6, k=150)
FLAG = dict(ap="--min-aligned-points", ov="--min-frame-overlap", corr="--min-pixel-correlation", k="--candidates")
SWEEP = dict(ap=[3, 5, 6, 7, 8, 9, 12], ov=[0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
             corr=[0.2, 0.4, 0.5, 0.55, 0.65, 0.7, 0.8], k=[10, 25, 50, 100, 300])
SINGLES = ["imgs", "imgs2", "imgs3", "imgs4"]
RES = f"{S}/results.jsonl"
for d in ("out", "logs", "caches"):
    os.makedirs(f"{S}/{d}", exist_ok=True)
done = {json.loads(l)["key"] for l in open(RES)} if os.path.exists(RES) else set()


def run(key, corpus, ws, params):
    if key in done:
        return
    out = f"{S}/out/{key}.json"
    log = f"{S}/logs/{key}.log"
    cmd = [f"{S}/img-fp-ship", CORP[corpus], "-r", "-x", "*", "-v", "-o", out, "--format", "json",
           "--no-last", "--work-size", str(ws), "--cache", f"{S}/caches/{corpus}_{ws}.bin"]
    for p, v in params.items():
        cmd += [FLAG[p], str(v)]
    row = dict(key=key, corpus=corpus, ws=ws, **{p: params.get(p, DEF[p]) for p in DEF})
    t0 = time.time()
    with open(log, "w") as fh:
        rc = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT).returncode
    row.update(rc=rc, wall=round(time.time() - t0, 1))
    if rc in (0, 2) and os.path.exists(out):
        d = json.load(open(out))
        pairs = sorted((min(x["a"], x["b"]), max(x["a"], x["b"])) for x in d["pairs"])
        row.update(pairs=len(pairs), groups=len(d["groups"]),
                   pairs_hash=hashlib.sha1(json.dumps(pairs).encode()).hexdigest()[:12])
        del d, pairs
        r = subprocess.run([sys.executable, f"{S}/../v29-area/score1.py", out, corpus], capture_output=True, text=True)
        if r.returncode == 0:
            row.update(json.loads(r.stdout))
        else:
            row["score_error"] = r.stderr[-2000:]
        os.remove(out)
    else:
        row["error"] = open(log, errors="replace").read()[-2000:]
    with open(RES, "a") as fh:
        fh.write(json.dumps(row) + "\n")
    done.add(key)
    sp = list(row.get("seedpairs", {}).items())[:2]
    print(f"{time.strftime('%H:%M:%S')} {key:<24} rc={rc} pairs {row.get('pairs', 0):,} F1 {row.get('f1', 0):.4f} "
          f"cross {row.get('cross', '-')} {sp} | {row['wall']}s", flush=True)


def cliffs(ws):
    for c in SINGLES + ["all"]:
        run(f"cl_{c}_{ws}_def", c, ws, {})
    for p, vals in SWEEP.items():
        for v in vals:
            for c in SINGLES + ["all"]:
                run(f"cl_{c}_{ws}_{p}{v}", c, ws, {p: v})


if __name__ == "__main__":
    for ph in sys.argv[1:]:
        print(f"=== {ph} {time.strftime('%H:%M:%S')}", flush=True)
        cliffs(int(ph.removeprefix("cliff")))
        print(f"=== {ph} done {time.strftime('%H:%M:%S')}", flush=True)
