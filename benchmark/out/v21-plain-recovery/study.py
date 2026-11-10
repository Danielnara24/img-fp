#!/usr/bin/env python3
"""Are the pairs the plain x86-64 build loses to v3 (JPEG XL rounding, see
CLAUDE.md) lost for good, or can the main settings bring them back?

Phase 1: both builds at the defaults on each corpus (own caches, --dump).
Phase 2: why plain missed each pair v3 found (from plain's dump).
Phase 3: plain swept over the four main settings; how many lost pairs return.
"""
import sys, os, json, csv, subprocess, re
from collections import Counter, defaultdict
S = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v19-aspect-texture")
import sweep, analyse
csv.field_size_limit(1 << 30)
D = "/home/daniel/Documents/"
CORP = {"imgs": D + "IMGS", "imgs2": D + "IMGS2", "imgs3": D + "IMGS3", "imgs4": D + "IMGS4"}
FLAG = dict(ap="--min-aligned-points", ov="--min-frame-overlap", corr="--min-pixel-correlation", k="--candidates")

def run(build, corpus, tag, params=(), dump=False):
    out = f"{S}/logs/{build}_{corpus}_{tag}.json"
    if os.path.exists(out):
        return out
    cmd = [f"{S}/img-fp-{build}", CORP[corpus], "-r", "-x", "*", "--cache", f"{S}/caches/{build}_{corpus}.cache", "-o", out + ".tmp", "--format", "json", *params]
    if dump:
        cmd += ["--dump", f"{S}/logs/{build}_{corpus}_{tag}.csv"]
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    os.rename(out + ".tmp", out)
    return out

def pairs(path):
    return {analyse.key(p["a"], p["b"]): p for p in json.load(open(path))["pairs"]}

def positives(corpus):
    return sweep.truth(corpus)[3]

summary = {}
for corpus in CORP:
    v3 = pairs(run("v3", corpus, "default", dump=True))
    pl = pairs(run("plain", corpus, "default", dump=True))
    pos = positives(corpus)
    lost = {k for k in set(v3) - set(pl) if k in pos}
    gained = {k for k in set(pl) - set(v3) if k in pos}
    lost_fp = len((set(v3) - set(pl)) - pos); gained_fp = len((set(pl) - set(v3)) - pos)
    m3, mp = sweep.score(f"{S}/logs/v3_{corpus}_default.json", corpus), sweep.score(f"{S}/logs/plain_{corpus}_default.json", corpus)
    print(f"\n== {corpus}: v3 F1 {m3['f1']:.5f} perfect {m3['perfect']} | plain F1 {mp['f1']:.5f} perfect {mp['perfect']} | "
          f"true pairs lost {len(lost)}, gained {len(gained)}; false pairs lost {lost_fp}, gained {gained_fp}", flush=True)
    summary[corpus] = dict(lost=len(lost), gained=len(gained))
    if not lost:
        continue
    # Phase 2: what plain's dump says about each lost true pair.
    rows = defaultdict(list)
    with open(f"{S}/logs/plain_{corpus}_default.csv", newline="") as fh:
        r = csv.reader(fh); next(r)
        for row in r:
            k = analyse.key(row[0], row[1])
            if k in lost:
                rows[k].append(row)
    why = Counter()
    for k in lost:
        v = v3[k]
        rs = rows.get(k)
        if not rs:
            why["no verdict with 3+ points (not a candidate, or retrieval/geometry found nothing)"] += 1
            continue
        best = max(rs, key=lambda x: (int(x[4]), float(x[9])))
        kind, n_in, ova, ovb, blk = best[2], int(best[4]), float(best[5]), float(best[6]), float(best[9])
        fails = []
        if kind != "propagated" and n_in < 10: fails.append("points")
        if max(ova, ovb) < 0.85: fails.append("overlap")
        if blk < 0.6: fails.append("correlation")
        why[f"{kind} verdict failing: {'+'.join(fails) or 'none of the three bars (cluster rules)'}"] += 1
    print("   why plain missed them:")
    for w, n in why.most_common():
        print(f"     {n:5}  {w}")
    how = Counter('identical' if v3[k].get('identical') else 'propagated' if v3[k].get('propagated') else
                  'corroborated' if v3[k].get('corroborated') else 'direct' for k in lost)
    print(f"   how v3 found them: {dict(how)}")
    # Phase 3: loosen the settings on plain.
    grid = [("ap", 9), ("ap", 8), ("ap", 7), ("ap", 6), ("ap", 5), ("ap", 3),
            ("ov", 0.8), ("ov", 0.7), ("ov", 0.6), ("ov", 0.5),
            ("corr", 0.55), ("corr", 0.5), ("corr", 0.4), ("corr", 0.3),
            ("k", 300), ("k", 600)]
    combos = [dict(ap=7, ov=0.7, corr=0.5), dict(ap=5, ov=0.6, corr=0.4, k=300), dict(ap=3, ov=0.5, corr=0.3, k=600)]
    for params in [dict([g]) for g in grid] + combos:
        tag = "_".join(f"{p}{v}" for p, v in params.items())
        argv = [x for p, v in params.items() for x in (FLAG[p], str(v))]
        out = run("plain", corpus, tag, argv)
        found = set(pairs(out))
        m = sweep.score(out, corpus)
        back = len(lost & found)
        print(f"   plain {tag:<22} recovers {back:4}/{len(lost)} ({100*back/len(lost):5.1f}%) | F1 {m['f1']:.5f} "
              f"P {100*m['precision']:.2f} R {100*m['recall']:.2f} cross {m['cross']} merges {m['merges']} perfect {m['perfect']}", flush=True)
        os.remove(out); open(out, "w").write(json.dumps({"pairs": [{"a": a, "b": b} for a, b in found & lost]}))
json.dump(summary, open(f"{S}/summary.json", "w"))
