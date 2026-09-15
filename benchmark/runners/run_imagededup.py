#!/usr/bin/env python3
"""Run idealo/imagededup over a corpus and emit groups.

imagededup ships no CLI, so this is the wrapper. It covers both halves of the
package: the four perceptual hashes (phash, dhash, whash, ahash) and the CNN
encoder (MobileNet embeddings + cosine), which is the only learned matcher in
a mainstream packaged dedup tool.

Two things about the library shape this wrapper:

  * `encode_images(image_dir=...)` is not recursive and keys its result by
    *basename*, so on a corpus with `train/Image_1.jpg` and `test/Image_1.jpg`
    it silently collapses the two. This builds a flat farm of symlinks with
    unique names instead, so the tool sees every file exactly once and the
    mapping back to real paths is unambiguous.
  * `find_duplicates` returns a dict of filename -> list of matches, i.e. a
    neighbour list, not groups. Groups here are the union-find closure of
    those pairs, matching how every other tool in the benchmark reports.

    python3 run_imagededup.py CORPUS -o out/idd_phash.json --method phash
"""
import argparse
import json
import os
import shutil
import sys
import tempfile
import time

EXTS = {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tif", ".tiff", ".webp",
        ".avif", ".heic", ".heif", ".jxl", ".ppm", ".pgm"}

HASH_METHODS = {"phash": "PHash", "dhash": "DHash",
                "whash": "WHash", "ahash": "AHash"}


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


def union_find(keys, pairs):
    index = {k: i for i, k in enumerate(keys)}
    parent = list(range(len(keys)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for a, b in pairs:
        ra, rb = find(index[a]), find(index[b])
        if ra != rb:
            parent[ra] = rb

    groups = {}
    for i in range(len(keys)):
        groups.setdefault(find(i), []).append(keys[i])
    return [g for g in groups.values() if len(g) > 1]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--method", default="phash",
                    choices=list(HASH_METHODS) + ["cnn"])
    ap.add_argument("-d", "--max-distance-threshold", type=int, default=10,
                    help="hash methods: max hamming distance (library default 10)")
    ap.add_argument("-s", "--min-similarity-threshold", type=float, default=0.9,
                    help="cnn: min cosine similarity (library default 0.9)")
    ap.add_argument("-r", "--recursive", action="store_true", default=True)
    ap.add_argument("--not-recursive", dest="recursive", action="store_false")
    ap.add_argument("--keep-farm", action="store_true",
                    help="leave the symlink farm in place for inspection")
    args = ap.parse_args()

    started = time.time()
    files = sorted(walk(args.roots, args.recursive, EXTS))
    print(f"{len(files)} files", file=sys.stderr)

    # flat farm: <index>__<basename>, unique by construction
    farm = tempfile.mkdtemp(prefix="imagededup-farm-")
    alias = {}
    for i, path in enumerate(files):
        name = f"{i:07d}__{os.path.basename(path)}"
        os.symlink(os.path.abspath(path), os.path.join(farm, name))
        alias[name] = path

    try:
        if args.method == "cnn":
            from imagededup.methods import CNN
            method = CNN()
            encodings = method.encode_images(image_dir=farm)
            duplicates = method.find_duplicates(
                encoding_map=encodings,
                min_similarity_threshold=args.min_similarity_threshold)
            config = {"method": "cnn",
                      "min_similarity_threshold": args.min_similarity_threshold}
        else:
            import imagededup.methods as m
            method = getattr(m, HASH_METHODS[args.method])()
            encodings = method.encode_images(image_dir=farm)
            duplicates = method.find_duplicates(
                encoding_map=encodings,
                max_distance_threshold=args.max_distance_threshold)
            config = {"method": args.method,
                      "max_distance_threshold": args.max_distance_threshold}
    finally:
        if not args.keep_farm:
            shutil.rmtree(farm, ignore_errors=True)

    encoded = sorted(encodings)
    missing = [alias[n] for n in alias if n not in encodings]

    seen, pairs = set(), []
    for name, matches in duplicates.items():
        for other in matches:
            key = tuple(sorted((name, other)))
            if key not in seen:
                seen.add(key)
                pairs.append(key)

    groups = union_find(encoded, pairs)

    result = {
        "tool": f"imagededup-{args.method}",
        "config": config,
        "files_enumerated": len(files),
        "files_encoded": len(encoded),
        "failures": [{"path": p, "error": "not returned by encode_images"}
                     for p in sorted(missing)],
        "runtime_seconds": round(time.time() - started, 2),
        "groups": [sorted(alias[n] for n in g) for g in
                   sorted(groups, key=lambda g: min(g))],
        "pairs": [{"a": alias[a], "b": alias[b]} for a, b in sorted(pairs)],
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    print(f"{len(groups)} groups, {len(pairs)} pairs -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
