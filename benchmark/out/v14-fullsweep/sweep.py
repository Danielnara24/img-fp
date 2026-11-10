#!/usr/bin/env python3
"""Full parameter sweep of img-fp 0.20.0 over IMGS, IMGS2, IMGS3 and all three.

usage: sweep.py PHASE   (ws | thr | ws0)
Results append to results.jsonl, keyed so that a restart skips what is done.
"""
import sys, os, json, re, subprocess, time
from collections import defaultdict

S = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import analyse

A, B, C = ("/home/daniel/Documents/IMGS", "/home/daniel/Documents/IMGS2",
           "/home/daniel/Documents/IMGS3")
CORP = {"imgs": [A], "imgs2": [B], "imgs3": [C], "all": [A, B, C]}
BIN = f"{S}/img-fp-v20"
RES = f"{S}/results.jsonl"
DEF = dict(ap=10, ov=0.85, corr=0.6, k=150)
FLAG = dict(ap="--min-aligned-points", ov="--min-frame-overlap",
            corr="--min-pixel-correlation", k="--candidates")

done = set()
if os.path.exists(RES):
    for line in open(RES):
        done.add(json.loads(line)["key"])

_truth = {}


def truth(corpus):
    if corpus not in _truth:
        analyse.CORPORA = CORP[corpus]
        m, p = analyse.load_truth()
        _truth[corpus] = analyse.build(m, p)
    return _truth[corpus]


def score_set(found, t, keep=lambda s: True):
    known, seed_of, variant_of, positives, skipped, prim = t
    pos = {k for k in positives if keep(seed_of[k[0]]) and keep(seed_of[k[1]])}
    fnd = {k for k in found if keep(seed_of[k[0]]) and keep(seed_of[k[1]])}
    m = analyse.metrics(fnd, pos)
    fp = fnd - pos
    trap = sum(1 for k in fp if seed_of[k[0]] == seed_of[k[1]])
    by = defaultdict(int)
    for a, b in fp:
        if seed_of[a] != seed_of[b]:
            by[analyse.key(seed_of[a], seed_of[b])] += 1
    m.update(trap=trap, cross=sum(by.values()), merges=analyse.merge_count(set(by)),
             seedpairs={f"{a}|{b}": n for (a, b), n in sorted(by.items(), key=lambda kv: -kv[1])})
    hs = {}
    seeds = {s for s in set(seed_of.values()) if keep(s)}
    for name, half in analyse.halves(seeds):
        ph = {k for k in pos if seed_of[k[0]] in half and seed_of[k[1]] in half}
        fh = {k for k in fnd if seed_of[k[0]] in half and seed_of[k[1]] in half}
        hs[name] = round(analyse.metrics(fh, ph)["f1"], 5)
    m["halves"] = hs
    per = defaultdict(lambda: [0, 0])
    for variant, k in prim:
        if not keep(seed_of[k[0]]):
            continue
        per[variant][1] += 1
        if k in fnd:
            per[variant][0] += 1
    m["perfect"] = sum(1 for h, tt in per.values() if h == tt)
    m["per_transform"] = {v: x for v, x in per.items()}
    return m


def score(path, corpus):
    t = truth(corpus)
    known, seed_of, _, _, skipped, _ = t
    data = json.load(open(path))
    found = {analyse.key(p["a"], p["b"]) for p in data.get("pairs", [])}
    groups = len(data.get("groups", []))
    del data
    unknown = {k for k in found if k[0] not in known or k[1] not in known}
    found = found - unknown - skipped
    out = score_set(found, t)
    out["groups"], out["unknown"] = groups, len(unknown)
    if corpus == "all":
        sub = {}
        for tag in ("IMGS", "IMGS2", "IMGS3"):
            m = score_set(found, t, keep=lambda s, tag=tag: s.split("/", 1)[0] == tag)
            m.pop("per_transform")
            sub[tag] = m
        out["sub"] = sub
        out["cross_corpus"] = sum(1 for a, b in found
                                  if seed_of[a].split("/", 1)[0] != seed_of[b].split("/", 1)[0])
    return out


def run(key, corpus, ws, cache, params, threads=None, extra=()):
    if key in done:
        return
    out = f"{S}/out/{key}.json"
    log = f"{S}/logs/{key}.log"
    cmd = ["/usr/bin/time", "-f", "TIME %e %U %S %M", BIN, *CORP[corpus], "-r", "-x", "*",
           "--work-size", str(ws), "-v", "-o", out]
    cmd += ["--cache", cache] if cache else ["--no-cache"]
    if threads:
        cmd += ["-t", str(threads)]
    for p, v in params.items():
        if v != DEF[p]:
            cmd += [FLAG[p], str(v)]
    cmd += list(extra)
    t0 = time.time()
    with open(log, "w") as fh:
        rc = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT).returncode
    txt = open(log).read()
    tm = re.search(r"TIME ([\d.]+) ([\d.]+) ([\d.]+) (\d+)", txt)
    row = dict(key=key, corpus=corpus, ws=ws, rc=rc, **{p: params.get(p, DEF[p]) for p in DEF},
               cold=cache is None or "cold" in key)
    if tm:
        row.update(wall=float(tm[1]), cpu=float(tm[2]) + float(tm[3]), maxrss_mb=int(tm[4]) // 1024)
    w = re.search(r"weak anchors: (\d+) admitted, (\d+) refused", txt)
    if w:
        row.update(weak_admitted=int(w[1]), weak_refused=int(w[2]))
    v = re.search(r"vocabulary: (\d+) live words of (\d+) from (\d+) samples", txt)
    if v:
        row.update(live_words=int(v[1]), sample=int(v[3]))
    if rc in (0, 2) and os.path.exists(out):
        row.update(score(out, corpus))
    else:
        row["error"] = txt[-2000:]
    try:
        os.remove(out)
    except OSError:
        pass
    with open(RES, "a") as fh:
        fh.write(json.dumps(row) + "\n")
    done.add(key)
    s = row
    print(f"{time.strftime('%H:%M:%S')} {key:<34} rc={rc} F1 {s.get('f1', 0):.4f} "
          f"P {100*s.get('precision', 0):.2f} R {100*s.get('recall', 0):.2f} "
          f"FP {s.get('fp', 0):,} trap {s.get('trap', 0):,} cross {s.get('cross', 0):,} "
          f"merges {s.get('merges', 0)} perfect {s.get('perfect', 0)} "
          f"| {s.get('wall', 0):.0f}s {s.get('cpu', 0):.0f}cpu {s.get('maxrss_mb', 0)}MB "
          f"({time.time()-t0:.0f}s)", flush=True)


WS = [384, 640, 16, 32, 48, 64, 96, 128, 192, 256, 320, 448, 512, 768, 896, 1024, 1280]
KEEP_CACHE = {384, 640}
SWEEP = dict(
    ap=[3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 16, 18, 20],
    ov=[0.3, 0.4, 0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 0.95],
    corr=[0.2, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.65, 0.7, 0.8],
    k=[10, 25, 50, 100, 200, 300, 500],
)

if __name__ == "__main__":
    phase = sys.argv[1]
    if phase == "ws":
        for ws in WS:
            cache = f"{S}/cache/c{ws}.bin"
            # the three corpora cold, each writing into one cache for this work
            # size, then all three together read from it
            for c in ("imgs", "imgs2", "imgs3"):
                run(f"ws_{c}_{ws}_cold", c, ws, cache, {})
            run(f"ws_all_{ws}", "all", ws, cache, {})
            if ws not in KEEP_CACHE and os.path.exists(cache):
                os.remove(cache)
    elif phase == "thr":
        for ws in (384, 640):
            cache = f"{S}/cache/c{ws}.bin"
            for p, vals in SWEEP.items():
                for v in vals:
                    for c in ("imgs", "imgs2", "imgs3", "all"):
                        run(f"thr_{c}_{ws}_{p}{v}", c, ws, cache, {p: v})
    elif phase == "ws0":
        for c in ("imgs3", "imgs2", "imgs"):
            run(f"ws_{c}_0_cold", c, 0, None, {}, threads=int(sys.argv[2]) if len(sys.argv) > 2 else 2)
    elif phase == "fine":
        # the clean island around the shipped correlation at 640, on the corpus
        # where Segovia merges on both sides of it, and the same at 384
        for ws in (640, 384):
            cache = f"{S}/cache/c{ws}.bin"
            for v in (0.56, 0.57, 0.58, 0.59, 0.61, 0.62, 0.63, 0.64, 0.66, 0.67, 0.68, 0.69, 0.75):
                for c in ("imgs3", "imgs2"):
                    run(f"thr_{c}_{ws}_corr{v}", c, ws, cache, {"corr": v})
        # the enlargement band around 256, where IMGS3 merged Segovia
        for ws in (224, 240, 272, 288):
            run(f"ws_imgs3_{ws}_cold", "imgs3", ws, None, {})
    print(f"PHASE {phase} DONE", flush=True)
