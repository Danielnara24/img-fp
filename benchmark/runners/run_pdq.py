#!/usr/bin/env python3
"""Run Meta's PDQ perceptual hash over a corpus and emit groups.

PDQ ships a hasher and a faiss matcher, not a dedup UX, so this supplies the
grouping: hash every image to 256 bits, take every pair within --distance, and
union-find those pairs into groups.

Meta's guidance is that a distance of 30 or less (of 256) works in production;
this defaults to 31, the threshold PDQ's own documentation calls a match.

    python3 run_pdq.py CORPUS -o out/pdq.json -d 31
"""
import argparse
import json
import os
import sys
import time
from concurrent.futures import ProcessPoolExecutor

import numpy as np
import pdqhash
from PIL import Image

EXTS = {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tif", ".tiff", ".webp",
        ".avif", ".heic", ".heif", ".jxl", ".ppm", ".pgm"}

# popcount for a byte, so hamming distance is a table lookup plus a sum
POPCOUNT = np.array([bin(i).count("1") for i in range(256)], dtype=np.uint8)


def hash_one(path):
    """-> (path, 32 packed bytes, quality) or (path, None, reason)."""
    try:
        with Image.open(path) as im:
            arr = np.array(im.convert("RGB"))
        vec, quality = pdqhash.compute(arr)
        return path, np.packbits(np.asarray(vec, dtype=np.uint8)), quality
    except Exception as exc:  # unreadable, truncated, unsupported codec
        return path, None, f"{type(exc).__name__}: {exc}"


def walk(roots, recursive, exts):
    for root in roots:
        if os.path.isfile(root):
            yield root
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            for name in sorted(filenames):
                if os.path.splitext(name)[1].lower() in exts:
                    yield os.path.join(dirpath, name)
            if not recursive:
                dirnames[:] = []
                break


def pairs_within(packed, threshold, chunk=256):
    """Every (i, j, distance) with i < j and distance <= threshold."""
    n = len(packed)
    for start in range(0, n, chunk):
        stop = min(start + chunk, n)
        # XOR this block against every later row, then popcount the bytes
        xor = packed[start:stop, None, :] ^ packed[None, start:, :]
        dist = POPCOUNT[xor].sum(axis=2, dtype=np.int16)
        # mask out the lower triangle of the block's own square
        rows, cols = np.nonzero(dist <= threshold)
        for r, c in zip(rows, cols):
            i, j = start + int(r), start + int(c)
            if i < j:
                yield i, j, int(dist[r, c])


def union_find(n, pairs):
    parent = list(range(n))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for i, j, _ in pairs:
        ri, rj = find(i), find(j)
        if ri != rj:
            parent[ri] = rj

    groups = {}
    for i in range(n):
        groups.setdefault(find(i), []).append(i)
    return [g for g in groups.values() if len(g) > 1]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("roots", nargs="+", help="folders or files to scan")
    ap.add_argument("-o", "--output", required=True, help="JSON output path")
    ap.add_argument("-d", "--distance", type=int, default=31,
                    help="max hamming distance of 256 bits (default 31)")
    ap.add_argument("-r", "--recursive", action="store_true", default=True)
    ap.add_argument("--not-recursive", dest="recursive", action="store_false")
    ap.add_argument("-j", "--jobs", type=int, default=os.cpu_count())
    ap.add_argument("-x", "--extensions", default=None,
                    help="comma-separated extension list (default: common image types)")
    args = ap.parse_args()

    exts = ({"." + e.lower().lstrip(".") for e in args.extensions.split(",")}
            if args.extensions else EXTS)

    started = time.time()
    files = sorted(walk(args.roots, args.recursive, exts))
    print(f"hashing {len(files)} files with {args.jobs} workers", file=sys.stderr)

    paths, packed, failures, qualities = [], [], [], []
    with ProcessPoolExecutor(max_workers=args.jobs) as pool:
        for path, bits, extra in pool.map(hash_one, files, chunksize=32):
            if bits is None:
                failures.append({"path": path, "error": extra})
            else:
                paths.append(path)
                packed.append(bits)
                qualities.append(int(extra))
    hashed_at = time.time()
    print(f"hashed {len(paths)} in {hashed_at - started:.1f}s "
          f"({len(failures)} failed); matching", file=sys.stderr)

    matrix = np.array(packed, dtype=np.uint8) if packed else np.zeros((0, 32), np.uint8)
    pairs = list(pairs_within(matrix, args.distance))
    groups = union_find(len(paths), pairs)

    result = {
        "tool": "pdq",
        "config": {"distance": args.distance, "bits": 256},
        "files_enumerated": len(files),
        "files_hashed": len(paths),
        "failures": failures,
        "runtime_seconds": round(time.time() - started, 2),
        "groups": [sorted(paths[i] for i in g) for g in
                   sorted(groups, key=lambda g: paths[g[0]])],
        "pairs": [{"a": paths[i], "b": paths[j], "distance": d} for i, j, d in pairs],
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    print(f"{len(groups)} groups, {len(pairs)} pairs -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
