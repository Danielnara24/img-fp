#!/usr/bin/env python3
"""Run difPy and emit groups in the benchmark's format.

difPy is the odd one out in the pool: no perceptual hash, just mean squared
error between images downscaled to 50x50, which fails and succeeds in
different places from everything else here. That is why it is worth its
runtime, which is considerable — it is O(n^2) in image comparisons and took
19 minutes on 9,285 images where czkawka took 13 seconds.

Two defaults worth knowing:

  * `-s/--similarity` defaults to `0` (MSE 0, i.e. "duplicates"). That is much
    stricter than it sounds like: it is not byte equality, but it is near-pixel
    equality after the downscale. Raise it for "similar".
  * `-dim/--same_dim` defaults to True, so images of different dimensions are
    never compared at all. On a corpus of resized copies that alone decides its
    recall. `--different-dimensions` turns it off.

    python3 run_difpy.py CORPUS -o out/difpy.json -s 0
"""
import argparse
import glob
import json
import os
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_DIF = os.path.join(HERE, "..", "..", "vendor", "venv", "lib",
                           "python3.12", "site-packages", "difPy", "dif.py")
DEFAULT_PY = os.path.join(HERE, "..", "..", "vendor", "venv", "bin", "python")


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
    for i, key in enumerate(keys):
        groups.setdefault(find(i), []).append(key)
    return [g for g in groups.values() if len(g) > 1]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--python", default=DEFAULT_PY)
    ap.add_argument("--dif", default=DEFAULT_DIF)
    ap.add_argument("-s", "--similarity", default="0",
                    help="MSE threshold; difPy's default 0 means near-pixel equality")
    ap.add_argument("-px", "--px-size", type=int, default=50)
    ap.add_argument("--rotate", default="True", choices=["True", "False"])
    ap.add_argument("--different-dimensions", action="store_true",
                    help="compare images of differing dimensions (difPy default: no)")
    ap.add_argument("--not-recursive", action="store_true")
    ap.add_argument("-proc", "--processes", type=int, default=None)
    args = ap.parse_args()

    workdir = tempfile.mkdtemp(prefix="difpy-out-")
    cmd = [os.path.abspath(args.python), os.path.abspath(args.dif),
           "-D"] + [os.path.abspath(r) for r in args.roots] + [
        "-Z", workdir,
        "-r", "False" if args.not_recursive else "True",
        "-s", str(args.similarity),
        "-px", str(args.px_size),
        "-ro", args.rotate,
        "-dim", "False" if args.different_dimensions else "True",
        "-p", "False",
    ]
    if args.processes:
        cmd += ["-proc", str(args.processes)]

    started = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.time() - started
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout[-4000:] + proc.stderr[-4000:])
        sys.exit(f"difPy exited {proc.returncode}")

    results_file = glob.glob(os.path.join(workdir, "*_results.json"))
    stats_file = glob.glob(os.path.join(workdir, "*_stats.json"))
    if not results_file:
        sys.exit(f"difPy wrote no results file into {workdir}")
    with open(results_file[0]) as fh:
        results = json.load(fh)
    stats = {}
    if stats_file:
        with open(stats_file[0]) as fh:
            stats = json.load(fh)

    seen, pairs, pair_rows = set(), [], []
    for path, matches in results.items():
        for other, mse in matches:
            key = tuple(sorted((path, other)))
            if key not in seen:
                seen.add(key)
                pairs.append(key)
                pair_rows.append({"a": key[0], "b": key[1], "mse": mse})

    keys = sorted({p for pair in pairs for p in pair})
    groups = union_find(keys, pairs)

    result = {
        "tool": "difpy",
        "config": {"similarity": args.similarity, "px_size": args.px_size,
                   "rotate": args.rotate == "True",
                   "same_dim": not args.different_dimensions},
        "command": " ".join(cmd),
        "files_enumerated": stats.get("total_files"),
        "failures": [{"path": p, "error": e} for p, e in
                     stats.get("invalid_files", {}).get("logs", {}).items()],
        "runtime_seconds": round(elapsed, 2),
        "groups": [sorted(g) for g in sorted(groups, key=min)],
        "pairs": sorted(pair_rows, key=lambda r: (r["a"], r["b"])),
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    print(f"{len(groups)} groups, {len(pair_rows)} pairs in {elapsed:.1f}s "
          f"-> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
