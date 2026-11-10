"""to_triples.py REPORT.json|DUMP.csv OUT.csv [--score correlation|points|blk]:
query-reference pairs as `Qxxxxx,Rxxxxxx,score` for score_disc.py.

From a report, the pairs img-fp states (JSON `pairs`), scored by pixel
correlation (default) or aligned points. From a dump, every verdict the
verifier wrote (gate loosened to 3 points / 0.2), scored by `blk` or `n_in`.
A pair of two queries or two references is left out.
"""
import csv, json, os, sys

src, out = sys.argv[1], sys.argv[2]
score = sys.argv[4] if len(sys.argv) > 4 else "correlation"
ident = lambda p: os.path.splitext(os.path.basename(p))[0]

def qr(a, b):
    a, b = ident(a), ident(b)
    if a[0] == "R" and b[0] == "Q":
        a, b = b, a
    return (a, b) if a[0] == "Q" and b[0] == "R" else None

n = 0
with open(out, "w") as f:
    if src.endswith(".json"):
        for p in json.load(open(src))["pairs"]:
            k = qr(p["a"], p["b"])
            if k:
                s = p["pixel_correlation"] if score == "correlation" else p["aligned_points"]
                f.write(f"{k[0]},{k[1]},{s}\n"); n += 1
    else:
        for r in csv.DictReader(open(src)):
            k = qr(r["a"], r["b"])
            if k:
                s = r["blk"] if score in ("blk", "correlation") else r["n_in"]
                f.write(f"{k[0]},{k[1]},{s}\n"); n += 1
print(f"{n} query-reference rows")
