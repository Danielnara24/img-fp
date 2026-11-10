#!/usr/bin/env python3
"""Containment: every true pair the 512 run states, the 1024 run states too.
contain.py CORPUS R512.json R1024.json"""
import sys
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score
corpus, a, b = sys.argv[1:4]
manifest, pairs = score.load_truth(corpus)
known, pos, skipped, prim = score.build(manifest, pairs)
fa, fb = score.load_tool(a)[1], score.load_tool(b)[1]
ta, tb = fa & pos, fb & pos
print(f"true at 512 {len(ta):,}, at 1024 {len(tb):,}, lost {len(ta - tb)}, gained {len(tb - ta):,}; all pairs lost {len(fa - fb)}")
