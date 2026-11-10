#!/usr/bin/env python3
"""The decode budget's divisor (MemAvailable / N) swept on IMGS, cold: corpus
evicted from the page cache, --no-cache, cooled to idle + 3 C before each run,
-t 8, img-fp-exp (--features prof) so decode:permit says how long workers
waited on the budget. Peak is PSS + swap over the process, sampled.

usage: budget.py DIV...   appends to budget.jsonl
"""
import json, os, re, resource, subprocess, sys, threading, time
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench

S = os.path.dirname(os.path.abspath(__file__))
CORPUS = "/home/daniel/Documents/IMGS"
IDLE = float(os.environ.get("IDLE", "0")) or bench.baseline_temp()

def mem_available():
    for l in open("/proc/meminfo"):
        if l.startswith("MemAvailable:"):
            return int(l.split()[1]) // 1024

def one(div):
    waited, ok = bench.wait_cool(30, IDLE + 3)
    bench.evict(CORPUS)
    avail = mem_available()
    log = f"{S}/logs/budget{div}.log"
    cmd = [f"{S}/img-fp-exp", CORPUS, "-r", "-x", "*", "--no-cache", "-t", "8", "-v", "-o", "/dev/null", "--format", "json"]
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time(); peak = [0]; temps = []
    with open(log, "w") as fh:
        p = subprocess.Popen(cmd, stdout=fh, stderr=subprocess.STDOUT, env={**os.environ, "IMGFP_BUDGET_DIV": str(div)})
        def samp():
            while p.poll() is None:
                peak[0] = max(peak[0], sum(bench_rollup(p.pid))); t = bench.cpu_temp(); temps.append(t or 0); time.sleep(0.2)
        th = threading.Thread(target=samp, daemon=True); th.start(); rc = p.wait(); th.join()
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    txt = open(log, errors="replace").read()
    m = re.search(r"decode:permit\s+([\d.]+)", txt)
    row = dict(div=div, rc=rc, wall=round(time.time() - t0, 1),
               cpu=round(r1.ru_utime - r0.ru_utime + r1.ru_stime - r0.ru_stime, 1),
               peak_pss_swap=peak[0], mem_available_mb=avail, budget_mb=max(avail // div, 64),
               permit_s=float(m[1]) if m else None, cooled_s=round(waited), start_temp=temps[0] if temps else None,
               mean_temp=round(sum(temps) / max(len(temps), 1), 1), pairs=re.search(r"(\d[\d,]*) pairs over", txt)[1])
    open(f"{S}/budget.jsonl", "a").write(json.dumps(row) + "\n")
    print(json.dumps(row), flush=True)

def bench_rollup(pid):
    p = sw = 0
    try:
        for l in open(f"/proc/{pid}/smaps_rollup"):
            if l.startswith("Pss:"): p = int(l.split()[1]) // 1024
            elif l.startswith("Swap:"): sw = int(l.split()[1]) // 1024
    except OSError:
        pass
    return p, sw

if __name__ == "__main__":
    print(f"idle {IDLE:.1f} C", flush=True)
    for d in sys.argv[1:]:
        one(int(d))
