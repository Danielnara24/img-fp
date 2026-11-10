"""score_shards.py N...: merge the query-reference pairs of shards N... and
score them against the ground truth limited to those shards' references, at
bars 0.5-0.8, ranked by aligned points."""
import csv, json, os, sys
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
R = os.environ.get("REPORTS", "shard{}/report.json"); ns = sys.argv[1:]
refs = set()
for n in ns:
    refs |= {l.strip() for l in open(f"refs{n}.ids")}
gt = {(r[0], r[1]) for r in csv.reader(open("gt/dev_ground_truth.csv")) if len(r) > 1 and r[1] in refs}
best = {}
for n in ns:
    for p in json.load(open(R.format(n)))["pairs"]:
        a, b = ident(p["a"]), ident(p["b"])
        if {a[0], b[0]} != {"Q", "R"}: continue
        k = (a, b) if a[0] == "Q" else (b, a)
        v = (p["aligned_points"], p["pixel_correlation"])
        if k not in best or v > best[k]: best[k] = v
print(f"shards {' '.join(ns)}: {len(refs)} references, {len(gt)} true copies in reach")
for lo in (0.5, 0.6, 0.7, 0.8):
    sel = sorted(((v[0], k) for k, v in best.items() if v[1] >= lo), reverse=True)
    tp = ap = r90 = 0
    for i, (_, k) in enumerate(sel, 1):
        if k in gt: tp += 1; ap += tp / i
        if tp / i >= 0.9: r90 = max(r90, tp / len(gt))
    print(f"  bar {lo}: preds {len(sel):6d} true {tp:5d} false {len(sel)-tp:6d}  uAP {ap/len(gt):.4f}  recall {tp/len(gt):.4f}  R@P90 {r90:.4f}")
