#!/usr/bin/env python3
"""prof.py CORPUS PREFIX BIN... : cooled, cold, --no-cache runs; stderr kept (the prof table)."""
import os, sys, time, json, subprocess, resource
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import bench
S = os.path.dirname(os.path.abspath(__file__))
corpus, pre, order = sys.argv[1], sys.argv[2], sys.argv[3:]
ceiling = bench.baseline_temp() + 3.0
for k, b in enumerate(order):
    waited, cooled = bench.wait_cool(60, ceiling)
    bench.evict(corpus)
    r0 = resource.getrusage(resource.RUSAGE_CHILDREN); t0 = time.time()
    with open(f"{S}/{pre}-{k}-{b}.err", "w") as err:
        rc = subprocess.call([f"{S}/{b}", corpus, "-r", "-x", "*", "--no-cache", "-v", "-o", f"{S}/{pre}-{k}-{b}.json"], stdout=subprocess.DEVNULL, stderr=err)
    r1 = resource.getrusage(resource.RUSAGE_CHILDREN)
    print(json.dumps(dict(k=k, bin=b, rc=rc, wall=round(time.time()-t0,1), cpu=round(r1.ru_utime-r0.ru_utime+r1.ru_stime-r0.ru_stime,1), user=round(r1.ru_utime-r0.ru_utime,1))), flush=True)
