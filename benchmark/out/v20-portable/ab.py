#!/usr/bin/env python3
"""ab.py CORPUS OUTPREFIX ORDER... : cold A/B under bench.py's protocol.
ORDER entries are binary names in the scratchpad (e.g. img-fp-base img-fp-final ...)."""
import os, sys, time, json, subprocess, threading, resource
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench
S = os.path.dirname(os.path.abspath(__file__))
corpus, pre, order = sys.argv[1], sys.argv[2], sys.argv[3:]
base = bench.baseline_temp()
ceiling = base + 3.0
print(f"idle {base:.1f} C, ceiling {ceiling:.1f} C", flush=True)
def footprint(pid):
    p = sw = 0
    try:
        for l in open(f"/proc/{pid}/smaps_rollup"):
            if l.startswith("Pss:"): p = int(l.split()[1]) // 1024
            elif l.startswith("Swap:"): sw = int(l.split()[1]) // 1024
    except OSError: pass
    return p, sw
results = []
for k, b in enumerate(order):
    waited, cooled = bench.wait_cool(60, ceiling)
    bench.evict(corpus)
    t0c = bench.cpu_temp()
    out = f"{S}/{pre}-{k}-{b}.json"
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
    t0 = time.time()
    p = subprocess.Popen([f"{S}/{b}", corpus, "-r", "-x", "*", "--no-cache", "-o", out], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    peak = [0, 0, 0]; mhz = []
    def samp():
        while p.poll() is None:
            ps, sw = footprint(p.pid)
            if ps + sw > peak[0]: peak[:] = [ps + sw, ps, sw]
            f = bench.cpu_mhz()
            if f: mhz.append(f)
            time.sleep(0.1)
    th = threading.Thread(target=samp, daemon=True); th.start()
    p.wait(); th.join()
    wall = time.time() - t0
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = (r1.ru_utime - r0.ru_utime) + (r1.ru_stime - r0.ru_stime)
    rec = dict(k=k, bin=b, rc=p.returncode, wall=round(wall, 1), cpu=round(cpu, 1), peak_pss_swap=peak[0], peak_pss=peak[1], swap_at_peak=peak[2],
               mhz=round(sum(mhz) / max(len(mhz), 1)), t_start=t0c, waited=round(waited), cooled=cooled)
    results.append(rec)
    print(json.dumps(rec), flush=True)
    json.dump(results, open(f"{S}/{pre}-results.json", "w"), indent=1)
