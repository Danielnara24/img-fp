#!/usr/bin/env python3
"""Run imgdupes and emit groups in the benchmark's format.

imgdupes prints fdupes-style output — one group per blank-line-separated block
of paths — and has no machine-readable mode, so this parses stdout. It is run
with --dry-run and --no-cache so a timed run is a real scan and nothing is
deleted.

    python3 run_imgdupes.py CORPUS -o out/imgdupes.json --method phash -d 4
"""
import argparse
import json
import os
import subprocess
import sys
import time

METHODS = ["ahash", "phash", "dhash", "whash", "phash_org"]


def parse_groups(stdout, corpus_roots):
    """Blocks of paths separated by blank lines. Ignore anything not a real path."""
    groups, current = [], []
    roots = tuple(os.path.abspath(r) for r in corpus_roots)
    for line in stdout.splitlines():
        line = line.rstrip()
        if not line.strip():
            if len(current) > 1:
                groups.append(sorted(current))
            current = []
            continue
        if line.startswith(roots) and os.path.exists(line):
            current.append(line)
    if len(current) > 1:
        groups.append(sorted(current))
    return groups


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--bin", default=os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "..", "vendor", "venv-imgdupes", "bin", "imgdupes"))
    ap.add_argument("--method", default="phash", choices=METHODS)
    ap.add_argument("-d", "--distance", type=int, default=4,
                    help="hamming distance threshold")
    ap.add_argument("--hash-bits", type=int, default=64,
                    help="must be a perfect square: 64, 144, 256 ...")
    ap.add_argument("--not-recursive", action="store_true")
    ap.add_argument("--num-proc", type=int, default=None)
    ap.add_argument("--keep-cache", action="store_true")
    args = ap.parse_args()

    if len(args.roots) != 1:
        sys.exit("imgdupes takes a single target directory")

    cmd = [os.path.abspath(args.bin)]
    if not args.not_recursive:
        cmd.append("--recursive")
    if not args.keep_cache:
        cmd.append("--no-cache")
    cmd += ["--dry-run", "--no-subdir-warning", "--hash-bits", str(args.hash_bits)]
    if args.num_proc:
        cmd += ["--num-proc", str(args.num_proc)]
    cmd += [os.path.abspath(args.roots[0]), args.method, str(args.distance)]

    started = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.time() - started
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-4000:])
        sys.exit(f"imgdupes exited {proc.returncode}")

    groups = parse_groups(proc.stdout, args.roots)
    pairs = [{"a": g[i], "b": g[j]}
             for g in groups for i in range(len(g)) for j in range(i + 1, len(g))]

    result = {
        "tool": f"imgdupes-{args.method}",
        "config": {"method": args.method, "distance": args.distance,
                   "hash_bits": args.hash_bits},
        "command": " ".join(cmd),
        "runtime_seconds": round(elapsed, 2),
        "groups": groups,
        "pairs": pairs,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    print(f"{len(groups)} groups, {len(pairs)} pairs in {elapsed:.1f}s "
          f"-> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
