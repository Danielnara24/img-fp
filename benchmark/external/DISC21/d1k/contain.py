"""contain.py A B N...: true query-reference pairs (bar 0.5) of report pattern A not in B, and B's not in A, over shards N."""
import csv, json, os, sys
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
A, B, ns = sys.argv[1], sys.argv[2], sys.argv[3:]
refs = set()
for n in ns: refs |= {l.strip() for l in open(f"refs{n}.ids")}
gt = {(r[0], r[1]) for r in csv.reader(open("gt/dev_ground_truth.csv")) if len(r) > 1 and r[1] in refs}
def true(P):
    s = set()
    for n in ns:
        for p in json.load(open(P.format(n)))["pairs"]:
            a, b = ident(p["a"]), ident(p["b"])
            k = (a, b) if a[0] == "Q" else (b, a)
            if k in gt and p["pixel_correlation"] >= 0.5: s.add(k)
    return s
a, b = true(A), true(B)
print(f"{A}: {len(a)}  {B}: {len(b)}  lost {len(a-b)}  added {len(b-a)}")
