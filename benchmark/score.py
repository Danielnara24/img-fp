#!/usr/bin/env python3
"""Score tool output against the generated corpus ground truth.

The primary number is **per-transform recall on original <-> variant pairs**:
one trial per seed per transformation, so the result is a table saying which
transformations a tool actually survives rather than a single figure. Aggregate
pair recall is reported too, and deliberately not led with — the families are
near-cliques, so one easy family moves the headline. That is the trap the
vid-fp benchmark documents, where a single 19-file cluster was 37% of the
positives.

Precision is reported as a **range**, because only in-family pairs are labelled:

  floor    every proposal outside the known positives is counted wrong
  ceiling  every such proposal is counted right

The truth is in between and needs the pool labelled by hand. A proposal that
lands on one of the known DIFFERENT pairs — the disjoint quadrants — is counted
wrong at both ends, because that one is not in doubt.

    python3 score.py out/v3/*.json
    python3 score.py out/v3/czkawka.json --per-transform
"""
import argparse
import csv
import glob
import json
import os
import sys
from collections import defaultdict

DEFAULT_CORPUS = "/home/daniel/Documents/IMGS"


def load_truth(corpus):
    base = os.path.join(corpus, "derived")
    with open(os.path.join(base, "manifest.csv")) as fh:
        manifest = list(csv.DictReader(fh))
    with open(os.path.join(base, "pairs.csv")) as fh:
        pairs = list(csv.DictReader(fh))
    return manifest, pairs


def key(a, b):
    return (a, b) if a <= b else (b, a)


def load_tool(path):
    with open(path) as fh:
        data = json.load(fh)
    if isinstance(data, dict) and "pairs" in data:
        found = {key(p["a"], p["b"]) for p in data["pairs"]}
        name = data.get("tool", os.path.basename(path))
        runtime = data.get("runtime_seconds")
        config = data.get("config", {})
    elif isinstance(data, dict) and "groups" in data and data["groups"] \
            and isinstance(data["groups"][0], dict):
        # fclones shape: {"groups":[{"files":[...]}, ...]}
        found = set()
        for g in data["groups"]:
            files = g["files"]
            for i in range(len(files)):
                for j in range(i + 1, len(files)):
                    found.add(key(files[i], files[j]))
        name, runtime, config = os.path.basename(path).replace(".json", ""), None, {}
    else:
        sys.exit(f"{path}: unrecognised shape")
    return name, found, runtime, config


def score(found, pairs, exclude_geometry=(), exclude_variants=()):
    positives, negatives, skipped = set(), set(), set()
    prim = []                       # (variant, pair-key) for ORIGINAL <-> variant
    for p in pairs:
        k = key(p["a"], p["b"])
        rel = p["relation"]
        geoms = {g for g in p["geometry"].split(",") if g}
        variants = {p["variant_a"], p["variant_b"]}
        if rel == "PARTIAL" or (geoms & set(exclude_geometry)) \
                or (variants & set(exclude_variants)):
            skipped.add(k)
            continue
        if rel in ("SAME", "CROP"):
            positives.add(k)
            if "ORIGINAL" in variants:
                other = (p["variant_b"] if p["variant_a"] == "ORIGINAL"
                         else p["variant_a"])
                prim.append((other, k, rel))
        elif rel == "DIFFERENT":
            negatives.add(k)

    # A skipped pair is excluded from BOTH ends: it is not a hit, and it must
    # not sit in the precision denominator either. Leaving it there charges a
    # tool for finding something we declined to judge — which understated
    # czkawka's precision as 95.9% when it is 99.99%.
    found = found - skipped
    hit = found & positives
    known_fp = found & negatives
    outside = found - positives - negatives

    per = defaultdict(lambda: [0, 0])
    per_rel = {}
    for variant, k, rel in prim:
        per[variant][1] += 1
        per_rel[variant] = rel
        if k in found:
            per[variant][0] += 1

    proposed = len(found)
    prec_floor = len(hit) / proposed if proposed else 0.0
    prec_ceil = (len(hit) + len(outside)) / proposed if proposed else 0.0
    recall = len(hit) / len(positives) if positives else 0.0
    return {
        "proposed": proposed,
        "positives": len(positives),
        "hit": len(hit),
        "recall": recall,
        "known_fp": len(known_fp),
        "outside": len(outside),
        "precision_floor": prec_floor,
        "precision_ceiling": prec_ceil,
        "per_transform": {k: tuple(v) for k, v in per.items()},
        "per_relation": per_rel,
    }


def f1(p, r):
    return 2 * p * r / (p + r) if (p + r) else 0.0


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("tools", nargs="+", help="tool output JSON files (globs ok)")
    ap.add_argument("--corpus", default=DEFAULT_CORPUS)
    ap.add_argument("--per-transform", action="store_true",
                    help="print the full per-transform table for each tool")
    ap.add_argument("--exclude-geometry", default="",
                    help="comma-separated geometry kinds to skip, e.g. mirror,rot90")
    ap.add_argument("--exclude-variants", default="exif_rot90",
                    help="comma-separated variants to skip; exif_rot90 has two "
                         "defensible right answers (see corpus README)")
    ap.add_argument("--csv", default=None, help="also write the per-transform matrix here")
    args = ap.parse_args()

    manifest, pairs = load_truth(args.corpus)
    paths = [p for pattern in args.tools for p in sorted(glob.glob(pattern))]
    if not paths:
        sys.exit("no tool outputs matched")

    excl_geom = [g for g in args.exclude_geometry.split(",") if g]
    excl_var = [v for v in args.exclude_variants.split(",") if v]

    results = []
    for path in paths:
        name, found, runtime, config = load_tool(path)
        s = score(found, pairs, excl_geom, excl_var)
        s["name"] = name
        s["file"] = os.path.basename(path)
        s["runtime"] = runtime
        s["config"] = config
        results.append(s)

    seeds = sum(1 for r in manifest if r["variant"] == "ORIGINAL")
    transforms = sorted({r["variant"] for r in manifest} - {"ORIGINAL"})
    print(f"corpus: {seeds} seeds x {len(transforms)} transforms, "
          f"{len(manifest)} files in families")
    if excl_var or excl_geom:
        print(f"excluded: variants={excl_var or '-'} geometry={excl_geom or '-'}")
    print()

    head = (f"{'tool':<26} {'proposed':>9} {'orig<->var':>12} {'all pairs':>11} "
            f"{'prec':>13} {'F1':>13} {'bad':>4}")
    print(head)
    print("-" * len(head))
    for r in results:
        pt = r["per_transform"]
        hit = sum(v[0] for v in pt.values())
        tot = sum(v[1] for v in pt.values())
        pf, pc = r["precision_floor"], r["precision_ceiling"]
        print(f"{r['file'][:26]:<26} {r['proposed']:>9} "
              f"{hit:>5}/{tot:<6} "
              f"{100*r['recall']:>10.1f}% "
              f"{100*pf:>5.1f}-{100*pc:<6.1f}% "
              f"{f1(pf, r['recall']):>5.3f}-{f1(pc, r['recall']):<6.3f} "
              f"{r['known_fp']:>4}")
    print()
    print("prec/F1 are ranges: the low end counts every proposal outside the")
    print("labelled in-family pairs as wrong, the high end as right. 'bad' is")
    print("proposals on the disjoint-quadrant pairs, which are wrong for certain.")

    if args.per_transform or args.csv:
        rows = []
        width = max(len(t) for t in transforms) + 1
        if args.per_transform:
            print()
            header = f"{'transform':<{width}} " + " ".join(
                f"{r['file'][:12]:>12}" for r in results)
            print(header)
            print("-" * len(header))
        for t in transforms:
            row = {"transform": t}
            cells = []
            for r in results:
                got, tot = r["per_transform"].get(t, (0, 0))
                row[r["file"]] = f"{got}/{tot}"
                cells.append(f"{got:>5}/{tot:<6}")
            rows.append(row)
            if args.per_transform:
                print(f"{t:<{width}} " + " ".join(c[:12].rjust(12) for c in cells))
        if args.csv:
            with open(args.csv, "w", newline="") as fh:
                w = csv.DictWriter(fh, fieldnames=list(rows[0]))
                w.writeheader()
                w.writerows(rows)
            print(f"\nper-transform matrix -> {args.csv}")


if __name__ == "__main__":
    main()
