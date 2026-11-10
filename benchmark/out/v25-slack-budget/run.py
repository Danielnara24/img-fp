#!/usr/bin/env python3
"""CLUSTER_SLACK_POINTS and CLUSTER_SLACK_CORRELATION swept, one at a time, on
IMGS-ALL at 512 from v24's cached analysis (accuracy only, not cooled). One
binary (img-fp-exp, plain x86-64, --features prof) reading IMGFP_SLACK_POINTS
/ IMGFP_SLACK_CORR / IMGFP_BUDGET_DIV, scaffolding since removed from the
source. Scoring is ../v23-plain/score1.py, as in v24.

usage: run.py KEY...   keys: corrX (X in hundredths), ptsN, or 'slack'
"""
import json, os, re, resource, subprocess, sys, threading, time

S = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(S, "..", "v23-plain", "score1.py")
D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3",
        "imgs4": f"{D}/IMGS4", "all": f"{D}/IMGS-ALL"}
CACHE = {512: f"{S}/../v24-constants/cache/all_t128.bin"}
RES = f"{S}/results.jsonl"
for d in ("out", "logs"):
    os.makedirs(f"{S}/{d}", exist_ok=True)
done = set()
if os.path.exists(RES):
    for line in open(RES):
        done.add(json.loads(line)["key"])


def footprint(pid):
    p = sw = 0
    try:
        for l in open(f"/proc/{pid}/smaps_rollup"):
            if l.startswith("Pss:"):
                p = int(l.split()[1]) // 1024
            elif l.startswith("Swap:"):
                sw = int(l.split()[1]) // 1024
    except OSError:
        pass
    return p, sw


def run(key, corpus, env, cache, ws=512, extra=()):
    if key in done:
        return
    out = f"{S}/out/{key}.json"
    log = f"{S}/logs/{key}.log"
    cmd = [f"{S}/img-fp-exp", CORP[corpus], "-r", "-x", "*", "-v", "-o", out,
           "--format", "json", "--cache", cache]
    if ws != 512:
        cmd += ["--work-size", str(ws)]
    cmd += list(extra)
    row = dict(key=key, corpus=corpus, env=env, ws=ws)
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time()
    with open(log, "w") as fh:
        p = subprocess.Popen(cmd, stdout=fh, stderr=subprocess.STDOUT, env={**os.environ, **env})
        peak = [0]

        def samp():
            while p.poll() is None:
                ps, sw = footprint(p.pid)
                peak[0] = max(peak[0], ps + sw)
                time.sleep(0.2)
        th = threading.Thread(target=samp, daemon=True)
        th.start()
        rc = p.wait()
        th.join()
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    row.update(rc=rc, wall=round(time.time() - t0, 1),
               cpu=round((r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime), 1),
               peak_pss_swap=peak[0])
    txt = open(log, errors="replace").read()
    for name, pat in [("weak", r"weak anchors: (\d+) admitted, (\d+) refused"),
                      ("same_pixels", r"same pixels: (\d+) files"),
                      ("damaged", r"(\d+)\s+image\(s\) are cut off")]:
        m = re.search(pat, txt)
        if m:
            row[name] = [int(g) for g in m.groups()] if len(m.groups()) > 1 else int(m[1])
    if rc in (0, 2) and os.path.exists(out):
        r = subprocess.run([sys.executable, SCORE, out, corpus], capture_output=True, text=True)
        if r.returncode == 0:
            row.update(json.loads(r.stdout))
        else:
            row["score_error"] = r.stderr[-2000:]
        os.remove(out)
    else:
        row["error"] = txt[-2000:]
    with open(RES, "a") as fh:
        fh.write(json.dumps(row) + "\n")
    done.add(key)
    s = row
    print(f"{time.strftime('%H:%M:%S')} {key:<22} rc={rc} F1 {s.get('f1', 0):.4f} "
          f"P {100*s.get('precision', 0):.2f} R {100*s.get('recall', 0):.2f} "
          f"trap {s.get('trap', 0):,} cross {s.get('cross', 0):,} merges {s.get('merges', 0)} "
          f"perfect {s.get('perfect', 0)} | {s['wall']:.0f}s {s['cpu']:.0f}cpu {s['peak_pss_swap']}MB", flush=True)



CORR = [0, 5, 10, 15, 20, 30, 60]
PTS = [0, 1, 2, 3, 4, 6]

def one(k, ws=512):
    m = re.fullmatch(r"(corr|pts)(\d+)", k)
    env = {"IMGFP_SLACK_CORR": f"{int(m[2]) / 100}"} if m[1] == "corr" else {"IMGFP_SLACK_POINTS": m[2]}
    run(f"{k}_all{'' if ws == 512 else ws}", "all", env, CACHE[ws], ws=ws)

if __name__ == "__main__":
    for k in sys.argv[1:]:
        if k == "slack":
            for v in CORR:
                one(f"corr{v}")
            for v in PTS:
                one(f"pts{v}")
        else:
            one(k)
