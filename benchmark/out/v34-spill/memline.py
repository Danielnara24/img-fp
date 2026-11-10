#!/usr/bin/env python3
"""Run a command with stderr to a file; print anon/file RSS over time beside its -v stage lines."""
import subprocess, sys, time, re
err = open('memline.err', 'w')
p = subprocess.Popen(sys.argv[1:], stdout=subprocess.DEVNULL, stderr=err)
t0 = time.time(); samples = []
while p.poll() is None:
    try:
        d = {l.split(':')[0]: int(l.split()[1]) for l in open(f'/proc/{p.pid}/status') if l.startswith(('RssAnon', 'RssFile', 'VmSwap'))}
        samples.append((time.time() - t0, d.get('RssAnon', 0) // 1024, d.get('RssFile', 0) // 1024, d.get('VmSwap', 0) // 1024))
    except Exception: pass
    time.sleep(0.05)
err.close()
import json; json.dump(samples, open('memline.samples.json','w'))
stages = [(float(m.group(1)), m.group(2)) for m in (re.match(r'\[\s*([\d.]+)s\] (.*)', l) for l in open('memline.err')) if m]
peak = max(samples, key=lambda s: s[1] + s[3])
print(f"peak anon+swap {peak[1]+peak[3]} MB at {peak[0]:.1f}s (file {peak[2]})")
last = None
for t, msg in stages:
    w = [s for s in samples if s[0] <= t + 0.3]
    if w:
        s = w[-1]; print(f"{t:6.1f}s anon {s[1]:5d} file {s[2]:5d} swap {s[3]:4d}  {msg[:70]}")
