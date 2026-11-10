#!/usr/bin/env python3
"""Split IMGS-ALL into a library and a scan, for `img-fp SCAN --against LIBRARY`.

    python3 make_against.py            # -> /home/daniel/Documents/IMGS-AGAINST

The question `--against` answers is per scanned file: does the library hold
it? The generated ground truth answers that as exactly as it answers pairs, so
the corpus is IMGS-ALL split in two, deterministically:

  * a quarter of the seeds are **absent**: every file of the family goes in
    the scan, and the right answer for each is that the library does not have
    it (the scan may still hold it twice, which is a repeat within the scan);
  * the rest are **split**: each file goes to the library with probability
    3/4 and to the scan otherwise, by a hash of its path.

Both sides are trees of **symlinks** into IMGS-ALL, not hard links: a hard
link changes the inode's ctime, which is part of img-fp's cache key, so a
hard-link tree would cost IMGS-ALL's cache every record. Run img-fp with
`--follow-symlinks -r -x '*'`, and the cache, keyed on canonical paths, is
IMGS-ALL's own. `split.csv` maps each link to its IMGS-ALL path and side;
`against_score.py` reads it.
"""
import argparse
import csv
import hashlib
import os
import shutil
import sys

DOCS = "/home/daniel/Documents"


def unit(s):
    return int.from_bytes(hashlib.sha1(s.encode()).digest()[:8], "big") / 2**64


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--corpus", default=os.path.join(DOCS, "IMGS-ALL"))
    ap.add_argument("--out", default=os.path.join(DOCS, "IMGS-AGAINST"))
    ap.add_argument("--absent", type=float, default=0.25, help="share of seeds wholly in the scan")
    ap.add_argument("--library", type=float, default=0.75, help="share of a split family's files in the library")
    ap.add_argument("--clean", action="store_true")
    args = ap.parse_args()
    out = os.path.abspath(args.out)
    if os.path.exists(out):
        if not args.clean:
            sys.exit(f"{out} exists; pass --clean to rebuild it")
        shutil.rmtree(out)
    with open(os.path.join(args.corpus, "derived", "manifest.csv")) as fh:
        manifest = list(csv.DictReader(fh))
    rows = []
    for r in manifest:
        absent = unit("seed:" + r["seed"]) < args.absent
        side = "scan" if absent or unit("file:" + r["rel_path"]) >= args.library else "library"
        link = os.path.join(out, side, r["rel_path"])
        os.makedirs(os.path.dirname(link), exist_ok=True)
        os.symlink(r["path"], link)
        rows.append({"link": link, "path": r["path"], "side": side, "seed": r["seed"], "absent": int(absent)})
    with open(os.path.join(out, "split.csv"), "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=["link", "path", "side", "seed", "absent"])
        w.writeheader()
        w.writerows(rows)
    n_scan = sum(r["side"] == "scan" for r in rows)
    seeds = {r["seed"] for r in rows}
    absent = {r["seed"] for r in rows if r["absent"]}
    print(f"{out}: {n_scan:,} scanned, {len(rows) - n_scan:,} in the library; "
          f"{len(absent)} of {len(seeds)} seeds absent from the library")


if __name__ == "__main__":
    main()
