#!/usr/bin/env python3
"""Tables from results.jsonl. usage: tables.py [ws|ap|ov|corr|k] [384|640]"""
import json, re, sys, os
S = os.path.dirname(os.path.abspath(__file__))
rows = [json.loads(l) for l in open(f"{S}/results.jsonl")]
DEF = dict(ap=10, ov=0.85, corr=0.6, k=150)


def stem(seed):
    s = seed.split("/")[-1]
    s = re.sub(r"\.[a-z0-9]+$", "", s, flags=re.I)
    return re.sub(r"[\d_-]+$", "", s).lower()


def split_cross(r):
    """cross-family pairs between sibling seeds (one scene) and unrelated ones,
    and the seed pairs carrying 20 or more of them (a family merge rather than
    a stray file or three)."""
    sib = unrel = 0
    big = []
    for k, n in r.get("seedpairs", {}).items():
        a, b = k.split("|")
        if stem(a) == stem(b):
            sib += n
        else:
            unrel += n
        if n >= 300:
            big.append((k, n))
    return sib, unrel, big


def line(r, label):
    sib, unrel, big = split_cross(r)
    h = r.get("halves", {})
    extra = ""
    if "wall" in r:
        extra = f" {r['wall']:>6.0f}s {r['cpu']:>6.0f}c {r.get('maxrss_mb', 0):>5}MB"
    return (f"{label:>6} {r['f1']:.4f} {100*r['precision']:6.2f}% {100*r['recall']:6.2f}% "
            f"{r['fp']:>6,} {r['trap']:>6,} {r['cross']:>6,} {sib:>6,} {unrel:>6,} "
            f"{len(big):>3} {r['merges']:>3} {r['perfect']:>3} "
            f"{h.get('alt', 0):.4f} {h.get('hash', 0):.4f} "
            f"{r.get('weak_admitted', 0):>6} {r.get('weak_refused', 0):>5}{extra}")


HDR = (f"{'value':>6} {'F1':>6} {'prec':>7} {'recall':>7} {'FP':>6} {'trap':>6} {'cross':>6} "
       f"{'sibl':>6} {'unrel':>6} {'big':>3} {'mrg':>3} {'prf':>3} {'halfA':>6} {'halfB':>6} "
       f"{'wkadm':>6} {'wkref':>5}")


def table(param, ws):
    for c in ("imgs", "imgs2", "imgs3", "all"):
        pts = []
        for r in rows:
            if r["corpus"] != c or "f1" not in r:
                continue
            if param == "ws":
                if r["key"].startswith("ws_"):
                    pts.append((r["ws"], r))
            elif r["ws"] == ws and (r["key"].startswith("thr_") or r["key"].startswith("ws_")):
                if all(r[p] == DEF[p] for p in DEF if p != param):
                    pts.append((r[param], r))
        pts.sort(key=lambda x: (x[0] == 0, x[0]))
        print(f"\n## {c}  {param}" + (f" at ws {ws}" if param != "ws" else ""))
        print(HDR + ("   wall    cpu   rss" if param == "ws" else ""))
        for v, r in pts:
            mark = "*" if (param == "ws" and v == 384) or (param != "ws" and v == DEF[param]) else " "
            print(line(r, f"{v}{mark}"))


if __name__ == "__main__":
    p = sys.argv[1] if len(sys.argv) > 1 else "ws"
    ws = int(sys.argv[2]) if len(sys.argv) > 2 else 384
    table(p, ws)
