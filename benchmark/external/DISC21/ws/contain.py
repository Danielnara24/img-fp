"""contain.py CORPUS SMALL.json LARGE.json: of SMALL's true pairs, how many
LARGE does not state (and the reverse), by transform of the non-original end."""
import sys, collections
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import score
corpus, small, large = sys.argv[1:4]
manifest, pairs = score.load_truth(corpus)
known, positives, skipped, prim = score.build(manifest, pairs)
var = {m["path"]: m["variant"] for m in manifest}
_, a, _, _ = score.load_tool(small)
_, b, _, _ = score.load_tool(large)
ta, tb = a & positives, b & positives
lost, new = ta - tb, tb - ta
print(f"true: small {len(ta):,} large {len(tb):,}; lost {len(lost):,} ({len(lost)/max(len(ta),1)*100:.2f}%), new {len(new):,}")
c = collections.Counter(min(var.get(x, "?"), var.get(y, "?")) for x, y in lost)
print("lost by transform (top 12):", ", ".join(f"{t} {n}" for t, n in c.most_common(12)))
