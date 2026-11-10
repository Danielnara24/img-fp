#!/usr/bin/env python3
"""One cold `--against` run, bench.py's protocol, appended to cold/runs.jsonl.

    cold.py idle                 # measure the idle baseline, save it
    cold.py cool                 # 60 s, then wait to idle + 3 C
    cold.py run CORPUS MODE      # evict, run, record (CORPUS: imgs | found; MODE: asym | union)
"""
import json, os, resource, subprocess, sys, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", ".."))
import bench  # noqa: E402

BIN = os.path.join(HERE, "..", "..", "..", "target", "release", "img-fp")
CORPORA = {
    "imgs": (["/home/daniel/Documents/IMGS-AGAINST/scan"], ["/home/daniel/Documents/IMGS-AGAINST/library"],
             ["--follow-symlinks", "-r", "-x", "*"], ["/home/daniel/Documents/IMGS-ALL"]),
    "found": (["/home/daniel/Documents/IMGS/derived/Desktop"], ["/home/daniel/Downloads/archive"],
              ["-r"], ["/home/daniel/Documents/IMGS/derived/Desktop", "/home/daniel/Downloads/archive"]),
}
IDLE = os.path.join(HERE, "cold", "idle.json")


def main():
    what = sys.argv[1]
    if what == "idle":
        base = bench.baseline_temp()
        json.dump({"idle_c": base}, open(IDLE, "w"))
        print(f"idle {base:.1f} C")
    elif what == "cool":
        ceiling = json.load(open(IDLE))["idle_c"] + 3
        waited, ok = bench.wait_cool(60, ceiling, cap=500)
        print(f"cooled {waited:.0f}s to {bench.cpu_temp():.1f} C (ceiling {ceiling:.1f}){'' if ok else ' NOT REACHED'}")
    elif what == "run":
        corpus, mode = sys.argv[2], sys.argv[3]
        scan, lib, flags, evict = CORPORA[corpus]
        n = sum(1 for l in open(os.path.join(HERE, "cold", "runs.jsonl"))) if os.path.exists(os.path.join(HERE, "cold", "runs.jsonl")) else 0
        out = os.path.join(HERE, "cold", f"{corpus}-{mode}-{n}.json")
        cmd = [BIN, *scan, *flags, "--no-cache", "-o", out]
        for l in lib:
            cmd += ["--against", l]
        if mode == "union":
            cmd.append("--against-union")
        files = sum(bench.evict(r)[0] for r in evict)
        free = bench.cached_bytes()
        t0, before = bench.cpu_temp(), resource.getrusage(resource.RUSAGE_CHILDREN)
        started = time.time()
        with open(out[:-5] + ".log", "wb") as log:
            proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
            s = bench.TreeSampler(proc.pid)
            s.start()
            rc = proc.wait()
        wall = time.time() - started
        s.stop_flag.set(); s.join(2)
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
        with open("/proc/meminfo") as fh:
            avail = next(int(l.split()[1]) // 1024 for l in fh if l.startswith("MemAvailable"))
        m = {"corpus": corpus, "mode": mode, "exit": rc, "wall": round(wall, 1),
             "cpu": round(after.ru_utime - before.ru_utime + after.ru_stime - before.ru_stime, 1),
             "peak_pss_swap_mb": round(s.peak_pss_swap / 2**20), "swap_at_peak_mb": round(s.peak_swap / 2**20),
             "mhz_mean": round(sum(s.mhz) / len(s.mhz)) if s.mhz else None,
             "temp_start": t0, "temp_end": bench.cpu_temp(), "evicted": files,
             "mem_available_after_mb": avail, "out": out, "time": time.strftime("%H:%M:%S")}
        with open(os.path.join(HERE, "cold", "runs.jsonl"), "a") as fh:
            fh.write(json.dumps(m) + "\n")
        print(json.dumps(m))


main()
