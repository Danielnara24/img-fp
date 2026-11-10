#!/usr/bin/env python3
"""--skip-mirrored against the default, on the plain build of the same source.

usage: run.py [CORPUS...]   (default: imgs imgs2 imgs3 imgs4 all found)

Per corpus, in this order:
  cold    bench.py's protocol (cooled to idle + 3 C, corpus evicted, --no-cache),
          in the order full skip skip full;
  fill    one uncooled run that writes this corpus's own cache file;
  cached  the same protocol reading that cache, full skip skip full.
Accuracy is deterministic, so each report is scored (score1.py, in a
subprocess) and the two runs of a setting are checked to agree. Rows go to
results.jsonl keyed, so a restart skips what is done.
"""
import hashlib, json, os, resource, subprocess, sys, threading, time

S = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench

D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3",
        "imgs4": f"{D}/IMGS4", "all": f"{D}/IMGS-ALL", "found": "/home/daniel/Downloads/archive"}
BIN = f"{S}/bin/img-fp-plain"
RES = f"{S}/results.jsonl"
for d in ("out", "logs", "cache"):
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


def run(key, corpus, skip, mode, protocol=True):
    if key in done:
        return
    out = f"{S}/out/{key}.json"
    log = f"{S}/logs/{key}.log"
    cmd = [BIN, CORP[corpus], "-r", "-x", "*", "-v", "-o", out, "--format", "json"]
    cmd += ["--no-cache"] if mode == "cold" else ["--cache", f"{S}/cache/{corpus}.bin"]
    if skip:
        cmd.append("--skip-mirrored")
    row = dict(key=key, corpus=corpus, skip=skip, mode=mode, protocol=protocol)
    if protocol:
        waited, cooled = bench.wait_cool(60, ceiling())
        bench.evict(CORP[corpus])
        row.update(waited=round(waited), cooled=cooled, t_start=bench.cpu_temp())
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time()
    with open(log, "w") as fh:
        p = subprocess.Popen(cmd, stdout=fh, stderr=subprocess.STDOUT)
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
        if corpus != "found":
            r = subprocess.run([sys.executable, f"{S}/score1.py", out, corpus], capture_output=True, text=True)
            if r.returncode == 0:
                row.update(json.loads(r.stdout))
            else:
                row["score_error"] = r.stderr[-2000:]
    else:
        row["error"] = open(log, errors="replace").read()[-2000:]
    # keep one report per corpus and setting; the second run is checked against it
    first = f"{S}/out/{corpus}_{'skip' if skip else 'full'}.json"
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
          f"F1 {s.get('f1', 0):.4f} P {100*s.get('precision', 0):.2f} R {100*s.get('recall', 0):.2f} "
          f"| {s['wall']:.1f}s {s['cpu']:.1f}cpu {s['peak_pss_swap']}MB (swap {s['swap_at_peak']}) "
          f"{s['mhz']}MHz", flush=True)


ORDER = [False, True, True, False]
if __name__ == "__main__":
    for c in sys.argv[1:] or ["imgs", "imgs2", "imgs3", "imgs4", "all", "found"]:
        print(f"=== {c} {time.strftime('%H:%M:%S')}", flush=True)
        for i, skip in enumerate(ORDER):
            run(f"{c}_cold_{i}_{'skip' if skip else 'full'}", c, skip, "cold")
        run(f"{c}_fill", c, False, "fill", protocol=False)
        for i, skip in enumerate(ORDER):
            run(f"{c}_cached_{i}_{'skip' if skip else 'full'}", c, skip, "cached")
