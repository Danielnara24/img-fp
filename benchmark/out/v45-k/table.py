#!/usr/bin/env python3
"""table.py CORPUS TAG... : one row a run in r/, scored on its own list."""
import json, subprocess, sys, os
PY = sys.executable
corpus = sys.argv[1]
print(f"{'run':<16} {'files':>6} {'F1':>7} {'P%':>6} {'R%':>6} {'trap':>5} {'cross':>6} {'seedp':>5} {'orig':>4} {'grp':>5} {'mixed':>5} {'other':>6}  top seed pairs")
for t in sys.argv[2:]:
    if not os.path.exists(f"r/{t}.json"): continue
    subprocess.run([PY, "kscore.py", f"--corpus={corpus}", f"--list=lists/{t.split('@')[0]}.txt", f"r/{t}.json", "--quiet", "--json=/tmp/claude-1000/ks.json"], check=True)
    r = json.load(open("/tmp/claude-1000/ks.json"))[0]
    top = ", ".join(f"{k.replace('.jpg','').replace('.jpeg','')}:{v}{'*' if k in r['orig'] else ''}" for k, v in list(r["by"].items())[:6])
    print(f"{t:<16} {r['n']:>6} {r['f1']:>7.4f} {100*r['p']:>6.2f} {100*r['r']:>6.2f} {r['trap']:>5} {r['cross']:>6} {r['seedpairs']:>5} {r['origpairs']:>4} {r['groups']:>5} {r['mixed']:>5} {r['unknown']:>6}  {top}")
