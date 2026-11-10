#!/usr/bin/env python3
"""Score tool output against the generated corpus. One F1, no labelling.

    python3 score.py out/v4/*.json
    python3 score.py out/v4/czkawka.json --per-transform

## Why there is nothing to hand-label

Every file in this corpus was generated from a known seed, so the ground truth
is exact in both directions:

  * **Positives** are pairs inside one seed family whose surviving regions say
    they are the same image (SAME) or that one contains the other (CROP).
  * **Negatives** are every pair whose two files descend from *different*
    seeds. There are millions of them and not one needs a human, because two
    photographs that were never the same photograph cannot become duplicates.

That second half is what an earlier version of this benchmark was missing. It
had a found corpus of 9,285 butterfly photographs bolted on to supply hard
negatives, which meant precision could only be estimated by hand-labelling
what the tools proposed — and on a single-species corpus a person cannot
honestly tell a duplicate from another photograph of the same insect. Judging
those pairs was measuring the labeller, not the tools.

Generated negatives have none of that problem. They are also harder than they
sound: the seeds deliberately include near-miss pairs — two Excel screenshots
sharing all their UI chrome, two black-and-white photographs, three panoramas
— so a tool that matches on layout or palette rather than content is caught by
exactly the pairs it should be.

## What is deliberately excluded

  * **PARTIAL** pairs, where two crops overlap but neither contains the other.
    They genuinely share content and genuinely are not duplicates; a wrong
    label there is worse than no label. Counted in neither numerator nor
    denominator.
  * **exif_rot90** by default, which has two defensible right answers: its
    pixels match the original, its EXIF says to display it rotated, so a tool
    that honours orientation matches it to rot90 instead and scores as having
    missed it. That is not a tool error.

A skipped pair is excluded from BOTH ends. It is not a hit, and it must not
sit in the precision denominator either — charging a tool for finding
something we declined to judge once understated czkawka's precision as 95.9%
when it was 99.99%.
"""
import argparse
import csv
import glob
import json
import os
import sys
from collections import defaultdict

DEFAULT_CORPUS = "/home/daniel/Documents/IMGS-ALL"


def key(a, b):
    return (a, b) if a <= b else (b, a)


def load_truth(corpus, manifest_dir=None):
    base = manifest_dir or os.path.join(corpus, "derived")
    with open(os.path.join(base, "manifest.csv")) as fh:
        manifest = list(csv.DictReader(fh))
    with open(os.path.join(base, "pairs.csv")) as fh:
        pairs = list(csv.DictReader(fh))
    return manifest, pairs


def load_tool(path):
    """What a tool claims, as pairs.

    `pairs` wins over `groups` wherever both exist: groups are the union-find
    closure of the pairs, and expanding a closure back into pairs credits the
    tool with every match its chains imply rather than the ones it made. That
    inflated SSCD from 9,172 claims to 238,771 once already.
    """
    with open(path) as fh:
        data = json.load(fh)
    name = data.get("tool", os.path.basename(path)) if isinstance(data, dict) else None
    runtime, config = None, {}
    if isinstance(data, dict):
        runtime, config = data.get("runtime_seconds"), data.get("config", {})
    if isinstance(data, dict) and data.get("pairs"):
        found = {key(p["a"], p["b"]) for p in data["pairs"]}
    elif isinstance(data, dict) and data.get("groups"):
        found = set()
        for g in data["groups"]:
            files = g["files"] if isinstance(g, dict) else g
            # img-fp writes each member as a row object with its evidence
            # beside it; every other tool, and img-fp's runs before it did,
            # a bare path.
            files = [f["path"] if isinstance(f, dict) else f for f in files]
            for i in range(len(files)):
                for j in range(i + 1, len(files)):
                    found.add(key(files[i], files[j]))
    elif isinstance(data, dict) and ("groups" in data or "pairs" in data):
        found = set()
    else:
        # The output directory also holds metrics.json and run_meta.json, which
        # are not tool results. Skipping them beats making the caller spell out
        # a glob that excludes them.
        return None, None, None, None
    return name, found, runtime, config


def build(manifest, pairs, exclude_geometry=(), exclude_variants=()):
    known = {r["path"] for r in manifest}
    positives, skipped = set(), set()
    prim = []                       # (catalogue, variant, pair-key, relation)
    excl_g, excl_v = set(exclude_geometry), set(exclude_variants)
    for p in pairs:
        k = key(p["a"], p["b"])
        geoms = {g for g in p["geometry"].split(",") if g}
        variants = {p["variant_a"], p["variant_b"]}
        if p["relation"] == "PARTIAL" or (geoms & excl_g) or (variants & excl_v):
            skipped.add(k)
            continue
        if p["relation"] in ("SAME", "CROP"):
            positives.add(k)
            if "ORIGINAL" in variants:
                other = (p["variant_b"] if p["variant_a"] == "ORIGINAL"
                         else p["variant_a"])
                prim.append((p.get("catalogue", "A"), other, k, p["relation"]))
        # DIFFERENT in-family pairs (disjoint crops, shuffled tilings) need no
        # entry: anything not a positive and not skipped is already a negative.
    return known, positives, skipped, prim


def score(found, known, positives, skipped, prim):
    # A pair touching a file the manifest does not know about cannot be judged.
    # With a fully generated corpus this should be empty; it is reported rather
    # than assumed so that a stray file in the tree shows up as a number.
    unknown = {k for k in found if k[0] not in known or k[1] not in known}
    found = found - unknown - skipped

    hit = found & positives
    fp = found - positives
    prop = len(found)
    precision = len(hit) / prop if prop else 0.0
    recall = len(hit) / len(positives) if positives else 0.0

    per = defaultdict(lambda: [0, 0])
    cat_of, rel_of = {}, {}
    for cat, variant, k, rel in prim:
        per[variant][1] += 1
        cat_of[variant], rel_of[variant] = cat, rel
        if k in found:
            per[variant][0] += 1

    return {
        "proposed": prop, "unknown": len(unknown),
        "positives": len(positives), "hit": len(hit), "fp": len(fp),
        "fn": len(positives) - len(hit),
        "precision": precision, "recall": recall,
        "f1": 2 * precision * recall / (precision + recall)
             if (precision + recall) else 0.0,
        "per_transform": {k: tuple(v) for k, v in per.items()},
        "catalogue_of": cat_of, "relation_of": rel_of,
    }


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("tools", nargs="+", help="tool output JSON files (globs ok)")
    ap.add_argument("--corpus", default=DEFAULT_CORPUS)
    ap.add_argument("--manifest-dir", default=None)
    ap.add_argument("--per-transform", action="store_true",
                    help="print the full per-transform recall table")
    ap.add_argument("--exclude-geometry", default="",
                    help="comma-separated geometry kinds to skip, e.g. mirror,rot90")
    ap.add_argument("--exclude-variants", default="",
                    help="comma-separated variants to skip. Empty by default: "
                         "the one variant that ever needed it, exif_rot90, had "
                         "two defensible right answers and is no longer in the "
                         "catalogue")
    ap.add_argument("--csv", default=None, help="write the per-transform matrix here")
    args = ap.parse_args()

    manifest, pairs = load_truth(args.corpus, args.manifest_dir)
    paths = [p for pattern in args.tools for p in sorted(glob.glob(pattern))]
    if not paths:
        sys.exit("no tool outputs matched")

    excl_g = [g for g in args.exclude_geometry.split(",") if g]
    excl_v = [v for v in args.exclude_variants.split(",") if v]
    known, positives, skipped, prim = build(manifest, pairs, excl_g, excl_v)

    results = []
    for path in paths:
        name, found, runtime, config = load_tool(path)
        if found is None:
            continue
        s = score(found, known, positives, skipped, prim)
        s.update({"name": name, "file": os.path.basename(path),
                  "runtime": runtime, "config": config})
        results.append(s)

    seeds = sum(1 for r in manifest if r["variant"] == "ORIGINAL")
    cats = defaultdict(set)
    for r in manifest:
        if r["variant"] != "ORIGINAL":
            cats[r.get("catalogue", "A")].add(r["variant"])
    total_pairs = len(known) * (len(known) - 1) // 2
    negatives = total_pairs - len(positives) - len(skipped)

    print(f"corpus: {len(known):,} files from {seeds} seeds, "
          + " + ".join(f"catalogue {k} {len(v)} transforms"
                       for k, v in sorted(cats.items())))
    print(f"truth:  {len(positives):,} positive pairs, {negatives:,} negative, "
          f"{len(skipped):,} excluded")
    if excl_v or excl_g:
        print(f"excluded variants={excl_v or '-'} geometry={excl_g or '-'}")
    print()

    head = (f"{'tool':<26} {'proposed':>9} {'TP':>7} {'FP':>7} {'FN':>7} "
            f"{'precision':>10} {'recall':>8} {'F1':>7} {'wall':>7}")
    print(head)
    print("-" * len(head))
    for r in sorted(results, key=lambda r: -r["f1"]):
        wall = f"{r['runtime']:.0f}s" if r["runtime"] else "-"
        print(f"{r['file'][:26]:<26} {r['proposed']:>9,} {r['hit']:>7,} "
              f"{r['fp']:>7,} {r['fn']:>7,} "
              f"{100*r['precision']:>9.1f}% {100*r['recall']:>7.1f}% "
              f"{r['f1']:>7.3f} {wall:>7}")
    stray = sum(r["unknown"] for r in results)
    if stray:
        print(f"\n{stray:,} proposed pairs touch files the manifest does not "
              f"know about and were dropped.")

    if args.per_transform or args.csv:
        transforms = sorted(
            {v for r in results for v in r["per_transform"]},
            key=lambda v: (results[0]["catalogue_of"].get(v, "A"), v))
        rows = []
        width = max((len(t) for t in transforms), default=10) + 1
        if args.per_transform:
            print()
            header = f"{'transform':<{width}} {'cat':<4} " + " ".join(
                f"{r['file'][:12]:>12}" for r in results)
            print(header)
            print("-" * len(header))
        last_cat = None
        for t in transforms:
            cat = results[0]["catalogue_of"].get(t, "A")
            row = {"transform": t, "catalogue": cat}
            cells = []
            for r in results:
                got, tot = r["per_transform"].get(t, (0, 0))
                row[r["file"]] = f"{got}/{tot}"
                cells.append(f"{got:>5}/{tot:<6}")
            rows.append(row)
            if args.per_transform:
                if cat != last_cat:
                    print(f"-- catalogue {cat} " + "-" * (len(header) - 16))
                    last_cat = cat
                print(f"{t:<{width}} {cat:<4} "
                      + " ".join(c[:12].rjust(12) for c in cells))
        if args.csv:
            with open(args.csv, "w", newline="") as fh:
                w = csv.DictWriter(fh, fieldnames=list(rows[0]))
                w.writeheader()
                w.writerows(rows)
            print(f"\nper-transform matrix -> {args.csv}")


if __name__ == "__main__":
    main()
