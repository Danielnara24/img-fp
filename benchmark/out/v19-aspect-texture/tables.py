import json, sys
from collections import defaultdict
R = {}
for line in open("results.jsonl"):
    r = json.loads(line); R[r["key"]] = r
C = ["imgs", "imgs2", "imgs3", "imgs4"]
def pooled(keys):
    rs = [R[k] for k in keys if k in R]
    tp = sum(r["tp"] for r in rs); fp = sum(r["fp"] for r in rs); fn = sum(r["fn"] for r in rs)
    p = tp / (tp + fp); rc = tp / (tp + fn)
    return dict(f1=2*p*rc/(p+rc), precision=p, recall=rc, trap=sum(r["trap"] for r in rs),
                cross=sum(r["cross"] for r in rs), merges=sum(r["merges"] for r in rs),
                perfect=sum(r["perfect"] for r in rs), cpu=sum(r.get("cpu", 0) for r in rs), tp=tp, fp=fp)
def fmt(r):
    m = "†" if r.get("merges") else " "
    return f"{r['f1']:.4f}{m} P{100*r['precision']:.2f} R{100*r['recall']:.2f} tr{r['trap']:>5} x{r['cross']:>4} pf{r['perfect']:>3}"
print("== A: AREA (elongated pictures get a 4:3 picture's pixels), cold runs; cpu is uncooled")
for ws in [384, 512, 640]:
    print(f"-- work {ws}")
    for v in ["base", "area"]:
        cells = [fmt(R[f"a_{v}_{c}_{ws}"]) for c in C]
        po = pooled([f"a_{v}_{c}_{ws}" for c in C])
        print(f"{v:5} " + " | ".join(cells) + f" || pooled {po['f1']:.4f} R{100*po['recall']:.2f} x{po['cross']} pf{po['perfect']} cpu{po['cpu']:.0f}")
print()
print("== B: TEXTURE cut (64 shipped = base), cached runs")
for ws in [384, 512, 640]:
    print(f"-- work {ws}")
    for t in [8, 16, 32, 64, 128, 256, 1024, 1000000000]:
        ks = [f"a_base_{c}_{ws}" if t == 64 else f"b_t{t}_{c}_{ws}" for c in C]
        cells = [fmt(R[k]) for k in ks]
        po = pooled(ks)
        lab = "inf" if t > 10**6 else str(t)
        print(f"{lab:>5} " + " | ".join(cells) + f" || pooled {po['f1']:.4f} R{100*po['recall']:.3f} tp{po['tp']} fp{po['fp']} x{po['cross']} pf{po['perfect']}")
