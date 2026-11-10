"""score_norm.py: µAP of the merged shards at the 0.5 bar, ranked by aligned
points less a per-query bias read off its verdicts with other dev queries
(the query set as a stand-in for SSCD's background set)."""
import csv, json, os, collections
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
gt = {(r[0], r[1]) for r in csv.reader(open("gt/dev_ground_truth.csv")) if len(r) > 1 and r[1].startswith("R")}
best = {}
for n in range(20):
    for p in json.load(open(f"shard{n}/report.json"))["pairs"]:
        a, b = ident(p["a"]), ident(p["b"])
        if {a[0], b[0]} != {"Q", "R"}: continue
        k = (a, b) if a[0] == "Q" else (b, a)
        v = (p["aligned_points"], p["pixel_correlation"])
        if k not in best or v > best[k]: best[k] = v
nb = collections.defaultdict(list)  # query -> [(points, corr)] with other queries
seen = set()
for a, b, pts, c in csv.reader(open("norm/qq.csv")):
    k = tuple(sorted((a, b)))
    if k in seen: continue  # both orders met: keep the first (best of its order)
    seen.add(k)
    nb[a].append((int(pts), float(c))); nb[b].append((int(pts), float(c)))
def uap(score, lo=0.5):
    sel = sorted(((score(k, v), k in gt) for k, v in best.items() if v[1] >= lo), reverse=True)
    tp = ap = r90 = 0
    for i, (_, t) in enumerate(sel, 1):
        if t: tp += 1; ap += tp / i
        if tp / i >= 0.9: r90 = max(r90, tp / len(gt))
    return ap / len(gt), r90
def bias(q, first, last, cbar):
    s = sorted((p for p, c in nb.get(q, ()) if c >= cbar), reverse=True)
    s = s[first:last + 1] + [0] * (last + 1 - first - len(s[first:last + 1]))
    return sum(s) / len(s)
print(f"{len(gt)} true; queries with any query neighbour {len(nb)}")
base = lambda k, v: v[0] + v[1] / 2
u, r = uap(base); print(f"plain                      uAP {u*100:.1f}  R@P90 {r*100:.1f}")
for cbar in (0.2, 0.5, 0.7):
    for first, last in ((0, 0), (0, 2), (1, 1)):
        B = {q: bias(q, first, last, cbar) for q in nb}
        for beta in (0.5, 1.0):
            u, r = uap(lambda k, v: v[0] + v[1] / 2 - beta * B.get(k[0], 0))
            print(f"bg corr>={cbar} n[{first},{last}] beta {beta}  uAP {u*100:.1f}  R@P90 {r*100:.1f}")
