"""own.py DUMP: for each stray file, the anchors (shipped bars) into its own
family and into the sibling's, clean (blk_min >= 0.85) and weak."""
import csv, sys
from collections import defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import analyse
analyse.CORPORA = ["/home/daniel/Documents/IMGS-ALL"]
m, p = analyse.load_truth(); t = analyse.build(m, p)
seed_of, variant_of = t[1], t[2]
want = {("IMGS3/algiu2.jpg", v) for v in ("transpose_diag", "mirror_crop", "flip_h")} | {("IMGS3/Segovia1.jpg", "crop_micro")}
other = {"IMGS3/algiu2.jpg": "IMGS3/algiu1.jpg", "IMGS3/Segovia1.jpg": "IMGS3/Segovia2.jpg"}
stray = {f for f in seed_of if (seed_of[f], variant_of.get(f)) in want}
c = defaultdict(lambda: defaultdict(list))
for r in csv.DictReader(open(sys.argv[1])):
    a, b = r["a"], r["b"]
    if a not in stray and b not in stray: continue
    if r["kind"] not in ("direct", "variant"): continue
    ok = int(r["n_in"]) >= 10 and max(float(r["ov_a"]), float(r["ov_b"])) >= 0.85 and float(r["blk"]) >= 0.6 and r["centred"] == "1"
    if not ok: continue
    for s, o in ((a, b), (b, a)):
        if s not in stray or o not in seed_of: continue
        fam = "own" if seed_of[o] == seed_of[s] else ("sib" if seed_of[o] == other[seed_of[s]] else "else")
        cl = "clean" if float(r["blk_min"]) >= 0.85 else "weak"
        c[s][(fam, cl, r["kind"])].append((variant_of.get(o), int(r["n_in"]), round(float(r["blk_min"]), 3)))
for s in sorted(stray):
    print(variant_of[s], s.rsplit("/", 1)[1])
    for k, L in sorted(c[s].items()):
        L.sort(key=lambda x: -x[2])
        print("   ", k, len(L), L[:4])
