import sys, rp
name, corp = sys.argv[1], sys.argv[2].split(",")
d = rp.load(name, [rp.CORP[c] for c in corp])
grid = [dict(bar=b) for b in (3, 5, 6, 7, 8, 9, 10, 12)] + \
       [dict(corr=c) for c in (0.2, 0.4, 0.5, 0.55, 0.58, 0.6, 0.65, 0.69, 0.7, 0.75, 0.8)] + \
       [dict(ov=o) for o in (0.3, 0.4, 0.5, 0.6, 0.7)]
for kw in grid:
    out = []
    for nm, rule in (("shipped", rp.shipped), ("acyclic", rp.acyclic_side), ("hub", rp.hub_side)):
        r = rp.run(d, rule=rule, **kw)
        out.append(f"{nm}: m={r['merges']} s={r['strays']:>2} x={r['xpairs']:>5} reach={r['reach']:.5f} {[w[:2] for w in r['worst']] if r['worst'] else ''}")
    print(name, kw, " | ".join(out), flush=True)
