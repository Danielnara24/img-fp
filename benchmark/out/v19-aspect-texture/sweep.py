#!/usr/bin/env python3
"""Sweeps for audit findings A (elongated pictures: AREA=1) and B (the
repeated-texture cut in index::shared: TEXTURE=N) on img-fp 0.28.0 plus two
environment switches (img-fp-exp; with neither set it is pair-for-pair 0.28.0).

usage: sweep.py PHASE   (a | b)
Each corpus is run and scored on its own. Results append to results.jsonl,
keyed, so a restart skips what is done.
"""
import sys, os, json, re, subprocess, time
from collections import defaultdict

S = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import analyse

D = "/home/daniel/Documents/"
CORP = {"imgs": D + "IMGS", "imgs2": D + "IMGS2", "imgs3": D + "IMGS3", "imgs4": D + "IMGS4"}
BIN = f"{S}/img-fp-exp"
RES = f"{S}/results.jsonl"

done = set()
if os.path.exists(RES):
    for line in open(RES):
        done.add(json.loads(line)["key"])

_truth = {}


def truth(corpus):
    if corpus not in _truth:
        analyse.CORPORA = [CORP[corpus]]
        m, p = analyse.load_truth()
        _truth[corpus] = analyse.build(m, p)
    return _truth[corpus]


def score(path, corpus):
    known, seed_of, variant_of, positives, skipped, prim = truth(corpus)
    data = json.load(open(path))
    found = {analyse.key(p["a"], p["b"]) for p in data.get("pairs", [])}
    groups = len(data.get("groups", []))
    del data
    found = {k for k in found if k[0] in known and k[1] in known} - skipped
    m = analyse.metrics(found, positives)
    fp = found - positives
    by = defaultdict(int)
    for a, b in fp:
        if seed_of[a] != seed_of[b]:
            by[analyse.key(seed_of[a], seed_of[b])] += 1
    m.update(trap=sum(1 for k in fp if seed_of[k[0]] == seed_of[k[1]]), cross=sum(by.values()),
             merges=sum(1 for n in by.values() if n >= 300),
             seedpairs={f"{a}|{b}": n for (a, b), n in sorted(by.items(), key=lambda kv: -kv[1])[:8]})
    hs = {}
    for name, half in analyse.halves(set(seed_of.values())):
        ph = {k for k in positives if seed_of[k[0]] in half and seed_of[k[1]] in half}
        fh = {k for k in found if seed_of[k[0]] in half and seed_of[k[1]] in half}
        hs[name] = round(analyse.metrics(fh, ph)["f1"], 5)
    m["halves"] = hs
    per = defaultdict(lambda: [0, 0])
    for variant, k in prim:
        per[variant][1] += 1
        per[variant][0] += k in found
    m["perfect"] = sum(1 for h, t in per.values() if h == t)
    m["per_transform"] = dict(per)
    m["groups"] = groups
    return m


def run(key, corpus, ws, cache, env, cold):
    if key in done:
        return
    out, log = f"{S}/out/{key}.json", f"{S}/logs/{key}.log"
    cmd = ["/usr/bin/time", "-f", "TIME %e %U %S %M", BIN, CORP[corpus], "-r", "-x", "*",
           "--work-size", str(ws), "-v", "-o", out, "--cache", cache]
    t0 = time.time()
    with open(log, "w") as fh:
        rc = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT, env={**os.environ, **env}).returncode
    txt = open(log).read()
    row = dict(key=key, corpus=corpus, ws=ws, rc=rc, cold=cold, **env)
    tm = re.search(r"TIME ([\d.]+) ([\d.]+) ([\d.]+) (\d+)", txt)
    if tm:
        row.update(wall=float(tm[1]), cpu=float(tm[2]) + float(tm[3]), maxrss_mb=int(tm[4]) // 1024)
    inst = re.search(r"INST texture=(\d+) words_dropped=(\d+) pairs_dropped=(\d+) words_kept=(\d+) capped=(\d+)", txt)
    if inst:
        row.update(words_dropped=int(inst[2]), pairs_dropped=int(inst[3]), words_kept=int(inst[4]), capped=int(inst[5]))
    w = re.search(r"weak anchors: (\d+) admitted, (\d+) refused", txt)
    if w:
        row.update(weak_admitted=int(w[1]), weak_refused=int(w[2]))
    a = re.search(r"anchors: (\d+) of (\d+) verified", txt)
    if a:
        row.update(anchors=int(a[1]), verified=int(a[2]))
    if rc in (0, 2) and os.path.exists(out):
        row.update(score(out, corpus))
        os.remove(out)
    else:
        row["error"] = txt[-2000:]
    with open(RES, "a") as fh:
        fh.write(json.dumps(row) + "\n")
    done.add(key)
    s = row
    print(f"{time.strftime('%H:%M:%S')} {key:<28} rc={rc} F1 {s.get('f1', 0):.4f} P {100*s.get('precision', 0):.2f} "
          f"R {100*s.get('recall', 0):.2f} trap {s.get('trap', 0):,} cross {s.get('cross', 0):,} merges {s.get('merges', 0)} "
          f"perfect {s.get('perfect', 0)} | {s.get('wall', 0):.0f}s {s.get('cpu', 0):.0f}cpu {s.get('maxrss_mb', 0)}MB "
          f"({time.time()-t0:.0f}s)", flush=True)


SIZES = [512, 384, 640]
TEXTURES = [16, 32, 128, 256, 1024, 1000000000, 8]

if __name__ == "__main__":
    phase = sys.argv[1]
    for ws in SIZES:
        for corpus in CORP:
            if phase == "a":
                run(f"a_base_{corpus}_{ws}", corpus, ws, f"{S}/caches/base_{corpus}.cache", {}, True)
                run(f"a_area_{corpus}_{ws}", corpus, ws, f"{S}/caches/area_{corpus}.cache", {"AREA": "1"}, True)
            elif phase == "b":
                for t in TEXTURES:
                    run(f"b_t{t}_{corpus}_{ws}", corpus, ws, f"{S}/caches/base_{corpus}.cache", {"TEXTURE": str(t)}, False)
