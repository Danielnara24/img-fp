#!/usr/bin/env python3
"""The area rule (IMGFP_AREA=1, img-fp-area) against 0.43.1 (img-fp-base), plain
builds, bench.py's protocol: cooled to idle + 3 C, corpus evicted, --no-cache.

usage: run.py [PHASE...]   (512 640 384 1024)
"""
import hashlib, json, os, resource, subprocess, sys, threading, time

S = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench

D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3",
        "imgs4": f"{D}/IMGS4", "all": f"{D}/IMGS-ALL", "found": "/home/daniel/Downloads/archive",
        "elong": f"{D}/IMGS-ELONG"}
BIN = {"base": f"{S}/img-fp-base", "area": f"{S}/img-fp-area"}
RES = f"{S}/results.jsonl"
for d in ("out", "logs"):
    os.makedirs(f"{S}/{d}", exist_ok=True)
done = {json.loads(l)["key"] for l in open(RES)} if os.path.exists(RES) else set()
_ceil = [None]


def ceiling():
    if _ceil[0] is None:
        b = bench.baseline_temp()
        _ceil[0] = b + 3.0
        print(f"idle {b:.1f} C, ceiling {_ceil[0]:.1f} C", flush=True)
    return _ceil[0]


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


def run(key, corpus, build, ws, protocol=True):
    if key in done:
        return
    out = f"{S}/out/{key}.json"
    log = f"{S}/logs/{key}.log"
    cmd = [BIN[build], CORP[corpus], "-r", "-x", "*", "-v", "-o", out, "--format", "json",
           "--work-size", str(ws), "--no-cache", "--no-last"] + (["-t", "2"] if ws == 0 else [])
    env = {**os.environ, "IMGFP_AREA": "1" if build == "area" else "0"}
    row = dict(key=key, corpus=corpus, build=build, ws=ws, protocol=protocol)
    if protocol:
        waited, cooled = bench.wait_cool(60, ceiling())
        bench.evict(CORP[corpus])
        row.update(waited=round(waited), cooled=cooled, t_start=bench.cpu_temp())
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time()
    with open(log, "w") as fh:
        p = subprocess.Popen(cmd, stdout=fh, stderr=subprocess.STDOUT, env=env)
        peak, mhz = [0, 0, 0], []

        def samp():
            while p.poll() is None:
                ps, sw = footprint(p.pid)
                if ps + sw > peak[0]:
                    peak[:] = [ps + sw, ps, sw]
                f = bench.cpu_mhz()
                if f:
                    mhz.append(f)
                time.sleep(0.1)
        th = threading.Thread(target=samp, daemon=True)
        th.start()
        rc = p.wait()
        th.join()
    wall = time.time() - t0
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    row.update(rc=rc, wall=round(wall, 1),
               cpu=round((r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime), 1),
               peak_pss_swap=peak[0], peak_pss=peak[1], swap_at_peak=peak[2],
               mhz=round(sum(mhz) / max(len(mhz), 1)), t_end=bench.cpu_temp())
    # the stage lines, for where the time went
    row["stages"] = [l.strip() for l in open(log, errors="replace") if l.startswith("[")]
    if rc in (0, 2) and os.path.exists(out):
        d = json.load(open(out))
        pairs = sorted((min(x["a"], x["b"]), max(x["a"], x["b"])) for x in d["pairs"])
        row.update(pairs=len(pairs), groups=len(d["groups"]),
                   pairs_hash=hashlib.sha1(json.dumps(pairs).encode()).hexdigest()[:12])
        acts = {}
        for g in d["groups"]:
            for f in g["files"]:
                acts[f["path"]] = f.get("action")
        for a in ("KEEP", "DELETE", "REVIEW"):
            row[a.lower()] = sum(1 for v in acts.values() if v == a)
        del d, pairs, acts
        if corpus == "elong":
            T = json.load(open(f"{CORP['elong']}/truth.json"))
            pos = {tuple(sorted(x)) for x in T["positives"]}
            part = {tuple(sorted(x)) for x in T["partial"]}
            d = json.load(open(out))
            f = {tuple(sorted((os.path.basename(x["a"]), os.path.basename(x["b"])))) for x in d["pairs"]} - part
            row.update(tp=len(f & pos), fp=len(f - pos), positives=len(pos))
        elif corpus != "found":
            r = subprocess.run([sys.executable, f"{S}/score1.py", out, corpus], capture_output=True, text=True)
            if r.returncode == 0:
                row.update(json.loads(r.stdout))
            else:
                row["score_error"] = r.stderr[-2000:]
    else:
        row["error"] = open(log, errors="replace").read()[-2000:]
    # keep one report per corpus and setting; the second run is checked against it
    first = f"{S}/out/{corpus}_{ws}_{build}.json"
    if os.path.exists(out):
        if os.path.exists(first):
            os.remove(out)
        else:
            os.rename(out, first)
    with open(RES, "a") as fh:
        fh.write(json.dumps(row) + "\n")
    done.add(key)
    s = row
    print(f"{time.strftime('%H:%M:%S')} {key:<26} rc={rc} pairs {s.get('pairs', 0):,} "
          f"tp {s.get('tp', '-')} fp {s.get('fp', '-')} F1 {s.get('f1', 0):.4f} P {100*s.get('precision', 0):.2f} R {100*s.get('recall', 0):.2f} "
          f"| {s['wall']:.1f}s {s['cpu']:.1f}cpu {s['peak_pss_swap']}MB (swap {s['swap_at_peak']}) "
          f"{s['mhz']}MHz", flush=True)


PHASES = {
    # the default: every corpus cold, A B B A
    "512": [(c, 512, ["base", "area", "area", "base"]) for c in ["elong", "imgs", "imgs2", "imgs3", "imgs4", "all", "found"]],
    # the other sizes: each corpus once a build, the order alternating corpus by corpus
    "640": [(c, 640, o) for c, o in zip(["elong", "imgs", "imgs2", "imgs3", "imgs4", "all"],
                                         [["base", "area"], ["area", "base"]] * 3)],
    "384": [(c, 384, o) for c, o in zip(["elong", "imgs", "imgs2", "imgs3", "imgs4", "all"],
                                         [["base", "area"], ["area", "base"]] * 3)],
    "1024": [("elong", 1024, ["base", "area"])],
    # full size, at -t 2 (a full-size scale space is not covered by the decode budget)
    "0": [("elong", 0, ["base", "area", "area", "base"])],
}
if __name__ == "__main__":
    for ph in sys.argv[1:] or ["512", "640", "384", "1024"]:
        for c, ws, order in PHASES[ph]:
            print(f"=== {c} {ws} {time.strftime('%H:%M:%S')}", flush=True)
            for i, b in enumerate(order):
                run(f"{c}_{ws}_{i}_{b}", c, b, ws)
