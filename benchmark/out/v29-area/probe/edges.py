"""edges.py DUMP: rows between the stray files and the sibling family."""
import csv, sys
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import analyse
analyse.CORPORA = ["/home/daniel/Documents/IMGS-ALL"]
m, p = analyse.load_truth(); t = analyse.build(m, p)
seed_of, variant_of = t[1], t[2]
want = {("IMGS3/algiu2.jpg", v) for v in ("transpose_diag", "mirror_crop", "flip_h")} | {("IMGS3/Segovia1.jpg", "crop_micro")}
other = {"IMGS3/algiu2.jpg": "IMGS3/algiu1.jpg", "IMGS3/Segovia1.jpg": "IMGS3/Segovia2.jpg"}
stray = {f for f in seed_of if (seed_of[f], variant_of.get(f)) in want}
for f in sorted(stray): print("stray", f)
rows = []
for r in csv.DictReader(open(sys.argv[1])):
    a, b = r["a"], r["b"]
    for s, o in ((a, b), (b, a)):
        if s in stray and seed_of.get(o) == other[seed_of[s]]:
            rows.append((s.rsplit("/", 1)[1], variant_of.get(o), r["kind"], int(r["n_in"]), float(r["blk"]), float(r["blk_min"]), max(float(r["ov_a"]), float(r["ov_b"])), r["centred"], r["inverted"]))
rows.sort(key=lambda x: (x[0], -x[3]))
print(len(rows), "rows")
for x in rows:
    if x[3] >= 10 or x[2] != "direct": print(x)
