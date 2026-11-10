#!/usr/bin/env python3
"""Score an img-fp run the way CLAUDE.md's tuning rule requires.

score.py gives one F1. This adds the three things the rule asks for and
score.py does not carry: false pairs split into deliberate rearrangement traps
and real cross-family errors, the number of family merges those errors imply,
and the same run re-scored on two disjoint halves of the seeds.
"""
import csv, json, os, sys, hashlib
from collections import defaultdict

# `--corpus=DIR`, repeatable, scores a run against several corpora at once: a
# second set of seeds alone, or merged with this one to test a rule at twice
# the size. Seeds are named per corpus then, since two corpora can each have a
# `sunset1.jpeg` and those are two photographs.
CORPORA = [a.split("=", 1)[1] for a in sys.argv[1:] if a.startswith("--corpus=")] or ["/home/daniel/Documents/IMGS-ALL"]


def key(a, b):
    return (a, b) if a <= b else (b, a)


def load_truth():
    manifest, pairs = [], []
    for corpus in CORPORA:
        tag = os.path.basename(os.path.normpath(corpus)) + "/" if len(CORPORA) > 1 else ""
        der = os.path.join(corpus, "derived")
        with open(os.path.join(der, "manifest.csv")) as fh:
            for r in csv.DictReader(fh):
                r["seed"] = tag + r["seed"]
                manifest.append(r)
        with open(os.path.join(der, "pairs.csv")) as fh:
            for r in csv.DictReader(fh):
                r["seed"] = tag + r["seed"]
                pairs.append(r)
    return manifest, pairs


def build(manifest, pairs):
    known = {r["path"] for r in manifest}
    seed_of = {r["path"]: r["seed"] for r in manifest}
    variant_of = {r["path"]: r["variant"] for r in manifest}
    positives, skipped, prim = set(), set(), []
    for p in pairs:
        k = key(p["a"], p["b"])
        if p["relation"] == "PARTIAL":
            skipped.add(k)
            continue
        if p["relation"] in ("SAME", "CROP"):
            positives.add(k)
            vs = {p["variant_a"], p["variant_b"]}
            if "ORIGINAL" in vs:
                other = p["variant_b"] if p["variant_a"] == "ORIGINAL" else p["variant_a"]
                prim.append((other, k))
    return known, seed_of, variant_of, positives, skipped, prim


def load_pairs(path):
    with open(path) as fh:
        data = json.load(fh)
    found = {key(p["a"], p["b"]) for p in data.get("pairs", [])}
    groups = data.get("groups", [])
    return found, len(groups), data.get("runtime_seconds"), data.get("config", {})


def metrics(found, positives):
    hit = found & positives
    prop = len(found)
    pr = len(hit) / prop if prop else 0.0
    rc = len(hit) / len(positives) if positives else 0.0
    f1 = 2 * pr * rc / (pr + rc) if pr + rc else 0.0
    return dict(proposed=prop, tp=len(hit), fp=prop - len(hit),
                fn=len(positives) - len(hit), precision=pr, recall=rc, f1=f1)


def halves(seeds):
    """Two disjoint halves of the seeds, under two different splits."""
    ordered = sorted(seeds)
    a = {s for i, s in enumerate(ordered) if i % 2 == 0}
    h = {s for s in ordered
         if int(hashlib.sha256(s.encode()).hexdigest(), 16) % 2 == 0}
    return [("alt", a), ("hash", h)]


def merge_count(cross):
    """Family merges implied by the cross-family edges: union-find over seeds."""
    parent = {}

    def find(x):
        parent.setdefault(x, x)
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    merges = 0
    for sa, sb in cross:
        ra, rb = find(sa), find(sb)
        if ra != rb:
            parent[ra] = rb
            merges += 1
    return merges


def report(path, manifest, truth, per_transform=False):
    known, seed_of, variant_of, positives, skipped, prim = truth
    found, n_groups, runtime, config = load_pairs(path)
    unknown = {k for k in found if k[0] not in known or k[1] not in known}
    found = found - unknown - skipped

    m = metrics(found, positives)
    fp = found - positives
    trap = {k for k in fp if seed_of[k[0]] == seed_of[k[1]]}
    cross = {k for k in fp if seed_of[k[0]] != seed_of[k[1]]}
    seed_pairs = {key(seed_of[a], seed_of[b]) for a, b in cross}

    print(f"== {os.path.basename(path)} ==")
    if config:
        print("   config:", json.dumps({k: v for k, v in sorted(config.items())
                                        if k not in ("roots",)}))
    print(f"   F1 {m['f1']:.4f}  precision {100*m['precision']:.2f}%  "
          f"recall {100*m['recall']:.2f}%")
    print(f"   proposed {m['proposed']:,}  TP {m['tp']:,}  FP {m['fp']:,}  "
          f"FN {m['fn']:,}  groups {n_groups}")
    print(f"   FP: trap {len(trap):,}  cross-family {len(cross):,}  "
          f"seed-pairs {len(seed_pairs)}  merges {merge_count(seed_pairs)}")
    if unknown:
        print(f"   {len(unknown):,} pairs touch unknown files")
    if cross:
        by = defaultdict(int)
        for a, b in cross:
            by[key(seed_of[a], seed_of[b])] += 1
        for (sa, sb), n in sorted(by.items(), key=lambda kv: -kv[1])[:10]:
            print(f"     cross {sa} <-> {sb}: {n:,} pairs")

    for name, half in halves(set(seed_of.values())):
        pos_h = {k for k in positives if seed_of[k[0]] in half and seed_of[k[1]] in half}
        found_h = {k for k in found if seed_of[k[0]] in half and seed_of[k[1]] in half}
        mh = metrics(found_h, pos_h)
        print(f"   half[{name}] F1 {mh['f1']:.4f}  P {100*mh['precision']:.2f}%  "
              f"R {100*mh['recall']:.2f}%  ({len(half)} seeds)")

    per = defaultdict(lambda: [0, 0])
    for variant, k in prim:
        per[variant][1] += 1
        if k in found:
            per[variant][0] += 1
    perfect = [v for v, (h, t) in per.items() if h == t]
    print(f"   perfect transforms: {len(perfect)} of {len(per)}")
    if per_transform:
        for v, (h, t) in sorted(per.items(), key=lambda kv: (kv[1][0] / kv[1][1], kv[0])):
            print(f"     {v:<24} {h:>3}/{t}")
    print()
    return dict(file=os.path.basename(path), trap=len(trap), cross=len(cross),
                merges=merge_count(seed_pairs), groups=n_groups,
                per_transform={v: tuple(x) for v, x in per.items()}, **m)


if __name__ == "__main__":
    pt = "--per-transform" in sys.argv
    files = [a for a in sys.argv[1:] if not a.startswith("--")]
    manifest, pairs = load_truth()
    truth = build(manifest, pairs)
    rows = [report(f, manifest, truth, pt) for f in files]
    if len(rows) > 1:
        print(f"{'run':<22} {'F1':>7} {'prec':>8} {'recall':>8} {'FP':>7} "
              f"{'trap':>7} {'cross':>7} {'merge':>5} {'groups':>7}")
        for r in rows:
            print(f"{r['file'][:22]:<22} {r['f1']:>7.4f} "
                  f"{100*r['precision']:>7.2f}% {100*r['recall']:>7.2f}% "
                  f"{r['fp']:>7,} {r['trap']:>7,} {r['cross']:>7,} "
                  f"{r['merges']:>5} {r['groups']:>7}")
