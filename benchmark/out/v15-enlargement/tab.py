#!/usr/bin/env python3
import json, os
E = os.path.dirname(os.path.abspath(__file__))
rows = {}
for l in open(f"{E}/results.jsonl"):
    r = json.loads(l)
    rows[r["key"]] = r
RULES = ["t512", "none", "t256", "t384", "t768", "t1024", "t512c2", "t512c4", "t1024c2", "t1024c4", "tws", "dbl"]
SMALL = ["thumbnail", "scale_small", "scale_mid", "crop_micro", "embed_tiny", "contact_sheet"]


def merged(r):
    return any(n >= 300 for n in r.get("seedpairs", {}).values())


def c(r):
    if not r or "f1" not in r:
        return "–"
    return f"{r['f1']:.4f}" + ("†" if merged(r) else "")


def table(prefix, labels, ws_list):
    for ws in ws_list:
        print(f"\n### ws {ws}")
        print("| rule | IMGS | IMGS2 | IMGS3 | all 3 | IMGS recall | IMGS CPU | IMGS3 CPU | found pairs | found CPU | " + " | ".join(SMALL) + " |")
        for lab in labels:
            g = lambda cp: rows.get(f"{prefix(lab)}_{cp}_{ws}_cold") or rows.get(f"{prefix(lab)}_{cp}_{ws}")
            i, f = g("imgs"), rows.get(f"enl_{lab}_found_384")
            pt = i["per_transform"] if i and "per_transform" in i else {}
            print(f"| {lab} | {c(i)} | {c(g('imgs2'))} | {c(g('imgs3'))} | {c(g('all'))} | "
                  f"{100*i['recall']:.2f}% | {i['cpu']:.0f} | {g('imgs3')['cpu'] if g('imgs3') else 0:.0f} | " if i else f"| {lab} | – | – | – | – | – | – | – | ",
                  end="")
            print(f"{f['pairs'] if f else '–'} | {f['cpu'] if f else 0:.0f} | " + " | ".join(str(pt.get(t, ['–'])[0]) for t in SMALL) + " |")


if __name__ == "__main__":
    import sys
    if len(sys.argv) > 1 and sys.argv[1] == "small":
        for ws in (16, 32, 48, 64, 96, 128, 192, 224, 240, 256, 288, 320):
            g = lambda cp: rows.get(f"fix_{cp}_{ws}_cold") or rows.get(f"fix_{cp}_{ws}")
            i = g("imgs")
            if i:
                print(f"| {ws} | {c(i)} | {100*i['recall']:.2f}% | {c(g('imgs2'))} | {c(g('imgs3'))} | {c(g('all'))} | {i['cpu']:.0f} |")
    else:
        table(lambda lab: f"enl_{lab}", [r for r in RULES if f"enl_{r}_imgs_384_cold" in rows], (384, 640))
