#!/usr/bin/env python3
"""memrun.py OUTPREFIX BINARY ARGS... : run, sample PSS every 0.1 s, map to -v stages."""
import os, sys, time, subprocess, threading, re, json
pre, cmd = sys.argv[1], sys.argv[2:]
def pss(pid):
    p = sw = 0
    try:
        with open(f"/proc/{pid}/smaps_rollup") as fh:
            for l in fh:
                if l.startswith("Pss:"): p = int(l.split()[1])//1024
                if l.startswith("Swap:"): sw = int(l.split()[1])//1024
    except OSError: return 0
    return p + sw * 1000000
def memavail():
    for l in open("/proc/meminfo"):
        if l.startswith("MemAvailable"): return int(l.split()[1])//1024
ma = memavail()
t0 = time.time()
p = subprocess.Popen(cmd + ["-v"], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
samples = []; swaps = []
def samp():
    while p.poll() is None:
        v = pss(p.pid); samples.append((time.time()-t0, v % 1000000)); swaps.append(v // 1000000); time.sleep(0.1)
th = threading.Thread(target=samp, daemon=True); th.start()
stages = []
for line in p.stderr:
    m = re.search(r"\[\s*([\d.]+)s\] (.*)", line)
    if m: stages.append((float(m.group(1)), m.group(2).strip()))
    elif "MEMDBG" in line or "TMPDBG" in line: print(line.strip(), flush=True)
p.wait(); th.join()
wall = time.time()-t0
ru = os.wait4 if False else None
peak = max(samples, key=lambda s: s[1])
print(f"peak swap {max(swaps) if swaps else 0} MB; peak pss+swap {max(a[1]+b for a,b in zip(samples,swaps))} MB"); print(f"exit {p.returncode} wall {wall:.1f}s memavail_start {ma} MB peak PSS {peak[1]} MB at {peak[0]:.1f}s")
# peak between consecutive stage lines (stage time measured from run start ~ process start)
bounds = [(0.0, "start")] + stages + [(wall, "end")]
for (ta, na), (tb, nb) in zip(bounds, bounds[1:]):
    seg = [s[1] for s in samples if ta <= s[0] < tb]
    if seg and tb - ta > 0.3:
        print(f"  {ta:7.1f}-{tb:7.1f}s  max {max(seg):5d} MB  end {seg[-1]:5d}  | -> {nb[:70]}")
json.dump({"samples": samples, "stages": stages, "wall": wall, "rc": p.returncode, "memavail": ma}, open(pre + ".json", "w"))
