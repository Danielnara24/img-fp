import csv, sys
from collections import defaultdict
def key(a,b): return (a,b) if a<=b else (b,a)
corpus, dumps = sys.argv[1], sys.argv[2:]
seed, var = {}, {}
for r in csv.DictReader(open(f"{corpus}/derived/manifest.csv")): seed[r["path"]]=r["seed"]; var[r["path"]]=r["variant"]
rel = {}
for p in csv.DictReader(open(f"{corpus}/derived/pairs.csv")): rel[key(p["a"],p["b"])] = (p["relation"], p["variant_a"], p["variant_b"])
for d in dumps:
    rows = defaultdict(list)
    for r in csv.DictReader(open(d)):
        a, b = r["a"], r["b"]
        if a not in seed or b not in seed: continue
        n_in = int(r["n_in"]); blk = float(r["blk"]); bm = float(r["blk_min"]); ov = max(float(r["ov_a"]), float(r["ov_b"]))
        if not (n_in >= 10 and blk >= 0.7 and ov >= 0.85): continue
        if seed[a] != seed[b]: c = "sibling"
        else:
            rr = rel.get(key(a,b), ("?",))[0]
            c = "true" if rr in ("SAME","CROP") else ("partial" if rr=="PARTIAL" else "trap")
        rows[c].append((blk, bm, n_in, r["kind"], var[a], var[b], a, b))
    print("==", d)
    for c, v in rows.items():
        bms = sorted(x[1] for x in v); blks = sorted(x[0] for x in v); ns = sorted(x[2] for x in v)
        q = lambda s, f: s[min(len(s)-1, int(f*len(s)))]
        print(f"  {c:<8} n={len(v):>5}  blk p01/p10/p50 {q(blks,.01):.3f}/{q(blks,.1):.3f}/{q(blks,.5):.3f}  blk_min p01/p10/p50 {q(bms,.01):.3f}/{q(bms,.1):.3f}/{q(bms,.5):.3f}  n_in p10/p50 {q(ns,.1)}/{q(ns,.5)}")
        if c == "sibling":
            for x in sorted(v, key=lambda x: -x[1])[:12]:
                print(f"      blk {x[0]:.3f} blk_min {x[1]:.3f} n_in {x[2]:>3} {x[3]:<7} {seed[x[6]].split('/')[-1]}:{x[4]} ~ {seed[x[7]].split('/')[-1]}:{x[5]}")
    # true positives with blk_min below the best sibling's
    if rows["sibling"]:
        top = max(x[1] for x in rows["sibling"])
        t = rows["true"]; below = [x for x in t if x[1] <= top]
        from collections import Counter
        print(f"  true verdicts at or below the highest sibling blk_min ({top:.3f}): {len(below)} of {len(t)}; by variant:", Counter(x[5] if x[4]=='ORIGINAL' else x[4] for x in below).most_common(12))
