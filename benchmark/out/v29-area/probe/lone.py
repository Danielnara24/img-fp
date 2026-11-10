"""lone.py DUMP SEED VARIANT: the anchors of one file, by family, best first."""
import sys
from clus import load, seed_of, variant_of
best = load(sys.argv[1])
s = [f for f in seed_of if seed_of[f] == sys.argv[2] and variant_of.get(f) == sys.argv[3]][0]
L = sorted(((v, seed_of[k[1] if k[0] == s else k[0]], variant_of.get(k[1] if k[0] == s else k[0])) for k, v in best.items() if s in k), key=lambda x: -x[0][1])
for v, sd, vr in L[:12]: print("  ", "OWN" if sd == sys.argv[2] else sd, vr, v)
