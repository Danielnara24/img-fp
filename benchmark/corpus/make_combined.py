#!/usr/bin/env python3
"""Build the combined benchmark corpus: IMGS, IMGS2, IMGS3 and IMGS4 under one root.

    python3 make_combined.py            # -> /home/daniel/Documents/IMGS-ALL

    # the second generation (sides as well as amounts drawn per seed), IMGS5 in:
    python3 make_combined.py --src /home/daniel/Documents/IMGS-v2 \
        --parts IMGS,IMGS2,IMGS3,IMGS4,IMGS5 --out /home/daniel/Documents/IMGS-ALL2

The four corpora are generated separately, each with its own `derived/`
manifest and ground truth. The baseline is all four together, and some tools
(imgdupes) take a single directory, so this makes one: a tree of **hard
links**, `IMGS-ALL/IMGS`, `IMGS-ALL/IMGS2`, ..., costing no disk and holding
the same inodes, so evicting the page cache through either path evicts both.

It then writes `IMGS-ALL/derived/manifest.csv` and `pairs.csv`, the four
ground truths concatenated with every path rewritten into the combined tree
and every seed prefixed with its corpus (`IMGS3/Segovia1.jpg`), since two
corpora may each have a seed of the same name. A pair across corpora is in no
`pairs.csv`, so it is a negative, which it is: no seed is in two corpora (this
checks, by sha256).

Hard links go stale when a corpus is regenerated (`make_variants.py --clean`
writes new inodes), so re-run this after any regeneration; `--clean` here
deletes the old tree first.
"""
import argparse
import csv
import os
import shutil
import subprocess
import sys

DOCS = "/home/daniel/Documents"
PARTS = ["IMGS", "IMGS2", "IMGS3", "IMGS4", "IMGS5"]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=os.path.join(DOCS, "IMGS-ALL"))
    ap.add_argument("--src", default=DOCS, help="the folder holding the parts")
    ap.add_argument("--parts", default=",".join(PARTS),
                    help="comma-separated corpora under --src")
    ap.add_argument("--clean", action="store_true")
    args = ap.parse_args()
    out = os.path.abspath(args.out)
    parts = [p for p in args.parts.split(",") if p]

    if os.path.exists(out):
        if not args.clean:
            sys.exit(f"{out} exists; pass --clean to rebuild it")
        shutil.rmtree(out)
    os.makedirs(os.path.join(out, "derived"))

    manifest, pairs, seen_sha = [], [], {}
    for part in parts:
        src = os.path.join(os.path.abspath(args.src), part)
        dst = os.path.join(out, part)
        subprocess.run(["cp", "-al", src, dst], check=True)

        def move(p):
            assert p.startswith(src + "/"), p
            return dst + p[len(src):]

        der = os.path.join(src, "derived")
        with open(os.path.join(der, "manifest.csv")) as fh:
            rd = csv.DictReader(fh)
            m_fields = rd.fieldnames
            for r in rd:
                if r["sha256"] in seen_sha and seen_sha[r["sha256"]] != part:
                    sys.exit(f"{r['path']} is byte-identical to a file in "
                             f"{seen_sha[r['sha256']]}: a cross-corpus pair "
                             "would be scored as a negative")
                seen_sha[r["sha256"]] = part
                r["path"] = move(r["path"])
                r["rel_path"] = f"{part}/{r['rel_path']}"
                r["seed"] = f"{part}/{r['seed']}"
                manifest.append(r)
        with open(os.path.join(der, "pairs.csv")) as fh:
            rd = csv.DictReader(fh)
            p_fields = rd.fieldnames
            for r in rd:
                r["a"], r["b"] = move(r["a"]), move(r["b"])
                r["seed"] = f"{part}/{r['seed']}"
                pairs.append(r)

    for name, fields, rows in (("manifest.csv", m_fields, manifest),
                               ("pairs.csv", p_fields, pairs)):
        with open(os.path.join(out, "derived", name), "w", newline="") as fh:
            w = csv.DictWriter(fh, fieldnames=fields)
            w.writeheader()
            w.writerows(rows)

    missing = [r["path"] for r in manifest if not os.path.exists(r["path"])]
    if missing:
        sys.exit(f"{len(missing)} manifest paths missing, e.g. {missing[0]}")
    seeds = sum(1 for r in manifest if r["variant"] == "ORIGINAL")
    print(f"{out}: {len(manifest):,} files from {seeds} seeds, "
          f"{len(pairs):,} ground-truth pairs")


if __name__ == "__main__":
    main()
