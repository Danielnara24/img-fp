#!/usr/bin/env python3
"""Cost of A (IMGFP_DEMOTE=1) and of the area rule plus A (IMGFP_AREA=1 too)
against 0.43.1, and the cliffs at 512 and 640 for 0.43.1 and A. One binary,
img-fp-exp (the plain build with both switches; both off is 0.43.1's code path).

usage: run.py PHASE...   costA cachedA costAreaA cliff512 cliff640
"""
import hashlib, json, os, resource, subprocess, sys, threading, time
S = os.path.dirname(os.path.abspath(__file__))
B = "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark"
sys.path.insert(0, B)
import bench
D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3", "imgs4": f"{D}/IMGS4",
        "all": f"{D}/IMGS-ALL", "found": "/home/daniel/Downloads/archive", "elong": f"{D}/IMGS-ELONG"}
ENV = {"base": ("0", "0"), "A": ("0", "1"), "areaA": ("1", "1")}
DEF = dict(ap=10, ov=0.85, corr=0.6, k=150)
FLAG = dict(ap="--min-aligned-points", ov="--min-frame-overlap", corr="--min-pixel-correlation", k="--candidates")
SWEEP = dict(ap=[3, 5, 6, 7, 8, 9, 12], ov=[0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
             corr=[0.2, 0.4, 0.5, 0.55, 0.65, 0.7, 0.8], k=[10, 25, 50, 100, 300])
SINGLES = ["imgs", "imgs2", "imgs3", "imgs4"]
RES = f"{S}/results.jsonl"
for d in ("out", "logs"):
    os.makedirs(f"{S}/{d}", exist_ok=True)
done = {json.loads(l)["key"] for l in open(RES)} if os.path.exists(RES) else set()
_ceil = [None]


def ceiling():
    if _ceil[0] is None:
        _ceil[0] = bench.baseline_temp() + 3.0
        print(f"idle ceiling {_ceil[0]:.1f} C", flush=True)
    return _ceil[0]


def footprint(pid):
    p = sw = 0
    try:
        for l in open(f"/proc/{pid}/smaps_rollup"):
            if l.startswith("Pss:"): p = int(l.split()[1]) // 1024
            elif l.startswith("Swap:"): sw = int(l.split()[1]) // 1024
    except OSError:
        pass
    return p, sw


def run(key, corpus, build, ws=512, cache=None, params=None, protocol=False):
    if key in done:
        return
    params = params or {}
    out = f"{S}/out/{key}.json"
    log = f"{S}/logs/{key}.log"
    # The area rule plus A's sweep moved to the 0.44.0 build part-way: img-fp-ship
    # is that code hard-wired (identical pairs to img-fp-exp's areaA on IMGS-ALL
    # at 512 and 640 and on IMGS-ELONG), and it caches wide pictures, which the
    # experiment build re-analysed on every run.
    exe = "img-fp-ship" if build == "areaA" and cache and cache.startswith("ship_") else "img-fp-exp"
    cmd = [f"{S}/{exe}", CORP[corpus], "-r", "-x", "*", "-v", "-o", out, "--format", "json",
           "--no-last", "--work-size", str(ws)]
    cmd += ["--cache", f"{S}/caches/{cache}"] if cache else ["--no-cache"]
    for p, v in params.items():
        cmd += [FLAG[p], str(v)]
    a, dm = ENV[build]
    env = {**os.environ, "IMGFP_AREA": a, "IMGFP_DEMOTE": dm}
    row = dict(key=key, corpus=corpus, build=build, ws=ws, cached=bool(cache), protocol=protocol,
               **{p: params.get(p, DEF[p]) for p in DEF})
    if protocol:
        waited, cooled = bench.wait_cool(60, ceiling())
        if not cache:
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
                if ps + sw > peak[0]: peak[:] = [ps + sw, ps, sw]
                f = bench.cpu_mhz()
                if f: mhz.append(f)
                time.sleep(0.1)
        th = threading.Thread(target=samp, daemon=True); th.start()
        rc = p.wait(); th.join()
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    row.update(rc=rc, wall=round(time.time() - t0, 1),
               cpu=round((r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime), 1),
               peak_pss_swap=peak[0], swap_at_peak=peak[2], mhz=round(sum(mhz) / max(len(mhz), 1)),
               t_end=bench.cpu_temp())
    if rc in (0, 2) and os.path.exists(out):
        d = json.load(open(out))
        pairs = sorted((min(x["a"], x["b"]), max(x["a"], x["b"])) for x in d["pairs"])
        row.update(pairs=len(pairs), groups=len(d["groups"]),
                   pairs_hash=hashlib.sha1(json.dumps(pairs).encode()).hexdigest()[:12])
        del d, pairs
        if corpus == "elong":
            T = json.load(open(f"{CORP['elong']}/truth.json"))
            pos = {tuple(sorted(x)) for x in T["positives"]}; part = {tuple(sorted(x)) for x in T["partial"]}
            f = {tuple(sorted((os.path.basename(x["a"]), os.path.basename(x["b"])))) for x in json.load(open(out))["pairs"]} - part
            row.update(tp=len(f & pos), fp=len(f - pos))
        elif corpus != "found":
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
    s = row
    sp = list(s.get("seedpairs", {}).items())[:2]
    print(f"{time.strftime('%H:%M:%S')} {key:<30} rc={rc} pairs {s.get('pairs', 0):,} F1 {s.get('f1', 0):.4f} "
          f"cross {s.get('cross', '-')} {sp} | {s['wall']}s {s['cpu']}cpu {s['peak_pss_swap']}MB {s['mhz']}MHz", flush=True)


def abba(tag, corpus, x, y, ws=512, cache=None):
    for i, b in enumerate((x, y, y, x)):
        run(f"{tag}_{corpus}_{i}_{b}", corpus, b, ws, cache=cache, protocol=True)


def cliffs(ws, builds=("base", "A")):
    cf = lambda c, b: f"{c}_{ws}.bin" if b != "areaA" else f"ship_areaA_{c}_{ws}.bin"
    for c in SINGLES + ["all"]:
        for b in builds:
            run(f"cl_{c}_{ws}_def_{b}", c, b, ws, cache=cf(c, b))
    for p, vals in SWEEP.items():
        for v in vals:
            for c in SINGLES + ["all"]:
                for b in builds:
                    run(f"cl_{c}_{ws}_{p}{v}_{b}", c, b, ws, cache=cf(c, b), params={p: v})


if __name__ == "__main__":
    for ph in sys.argv[1:]:
        print(f"=== {ph} {time.strftime('%H:%M:%S')}", flush=True)
        if ph == "costA":
            for c in ("all", "found"):
                abba("costA", c, "base", "A")
        elif ph == "cachedA":
            abba("cachedA", "all", "base", "A", cache="all_512.bin")
        elif ph == "costAreaA":
            for c in ("all", "found", "elong"):
                abba("costAreaA", c, "base", "areaA")
        elif ph == "cost640":
            for c in ("all", "found"):
                for i, ws in enumerate((512, 640, 640, 512)):
                    run(f"costWS_{c}_{i}_{ws}", c, "base", ws, protocol=True)
        elif ph == "cliffArea512":
            cliffs(512, ("areaA",))
        elif ph == "cliffArea640":
            cliffs(640, ("areaA",))
        elif ph == "cliff512":
            cliffs(512)
        elif ph == "cliff640":
            cliffs(640)
        print(f"=== {ph} done {time.strftime('%H:%M:%S')}", flush=True)
