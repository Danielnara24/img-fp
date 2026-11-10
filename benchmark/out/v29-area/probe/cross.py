"""cross.py REPORT CORPUS: the files behind each cross-family seed pair, with
how many cross pairs each carries, its variant, size and aspect, and how it
was matched."""
import json, os, sys
from collections import Counter, defaultdict
sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import analyse
from PIL import Image
D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3", "imgs4": f"{D}/IMGS4", "all": f"{D}/IMGS-ALL"}
analyse.CORPORA = [CORP[sys.argv[2]]]
m, p = analyse.load_truth(); t = analyse.build(m, p)
known, seed_of, variant_of = t[0], t[1], t[2]
d = json.load(open(sys.argv[1]))
pr = {}
for x in d["pairs"]:
    k = analyse.key(x["a"], x["b"])
    if k[0] in known and k[1] in known and seed_of[k[0]] != seed_of[k[1]]:
        pr[k] = x
by = defaultdict(list)
for (a, b), x in pr.items():
    by[analyse.key(seed_of[a], seed_of[b])].append((a, b, x))
for sp, L in sorted(by.items(), key=lambda kv: -len(kv[1])):
    print(f"== {sp[0]} | {sp[1]}  {len(L)}")
    c = Counter()
    for a, b, x in L:
        c[a] += 1; c[b] += 1
    for f, n in c.most_common(6):
        try: w, h = Image.open(f).size
        except Exception: w = h = 0
        rel = Counter(x.get("relation") for a, b, x in L if f in (a, b))
        print(f"   {n:4d} {seed_of[f]:<28} {variant_of.get(f, '?'):<22} {w}x{h} ({max(w,h)/max(1,min(w,h)):.2f}) {dict(rel)}")
