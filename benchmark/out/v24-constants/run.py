#!/usr/bin/env python3
"""The pixel check's four underived constants, swept, on the build with the
0.30.0 audit fixes (A-F). One binary per value (build.sh); accuracy runs only,
not cooled, each from a cache at its own thumbnail size.

usage: run.py PHASE...   phases: def (the fixed build on every corpus), and
                          thumb / grid / block / flat (IMGS-ALL at 512)

Every run appends one row to results.jsonl, keyed, so a restart skips what is
done. Scoring is ../v23-plain/score1.py in a subprocess, as there.
"""
import json, os, re, resource, subprocess, sys, threading, time

S = os.path.dirname(os.path.abspath(__file__))
SCORE = os.path.join(S, "..", "v23-plain", "score1.py")
D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3",
        "imgs4": f"{D}/IMGS4", "all": f"{D}/IMGS-ALL"}
RES = f"{S}/results.jsonl"
for d in ("out", "logs", "cache"):
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


def run(key, corpus, binary, cache, ws=512, extra=()):
    if key in done:
        return
    out = f"{S}/out/{key}.json"
    log = f"{S}/logs/{key}.log"
    cmd = [f"{S}/bin/img-fp-{binary}", CORP[corpus], "-r", "-x", "*", "-v", "-o", out,
           "--format", "json", "--cache", f"{S}/cache/{cache}"]
    if ws != 512:
        cmd += ["--work-size", str(ws)]
    cmd += list(extra)
    row = dict(key=key, corpus=corpus, bin=binary, ws=ws)
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time()
    with open(log, "w") as fh:
        p = subprocess.Popen(cmd, stdout=fh, stderr=subprocess.STDOUT)
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


SWEEP = dict(thumb=[64, 96, 192, 256], grid=[32, 40, 56, 64, 80], block=[4, 6, 12, 16],
             flat=[0, 1, 2, 3, 6, 8, 12])

if __name__ == "__main__":
    for phase in sys.argv[1:]:
        print(f"=== PHASE {phase} {time.strftime('%H:%M:%S')}", flush=True)
        if phase == "def":
            for c in ["imgs", "imgs2", "imgs3", "imgs4", "all"]:
                run(f"def_{c}", c, "def", f"{c}_t128.bin")
        elif phase == "def640":
            for c in ["imgs", "imgs2", "imgs3", "imgs4", "all"]:
                run(f"def640_{c}", c, "def", f"{c}_t128_640.bin", ws=640)
        elif phase in SWEEP:
            for v in SWEEP[phase]:
                cache = f"all_t{v}.bin" if phase == "thumb" else "all_t128.bin"
                run(f"{phase}{v}_all", "all", f"{phase}{v}", cache)
        else:
            # a single key: PHASE_VALUE, e.g. grid32
            m = re.fullmatch(r"([a-z]+)(\d+)", phase)
            if m and m[1] in SWEEP:
                v = int(m[2])
                cache = f"all_t{v}.bin" if m[1] == "thumb" else "all_t128.bin"
                run(f"{phase}_all", "all", phase, cache)
