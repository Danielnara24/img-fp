#!/usr/bin/env python3
"""Run czkawka_cli's image mode and emit groups in the benchmark's format.

Czkawka already writes JSON, so this is a thin shim — but two of its defaults
would quietly distort a benchmark and are overridden here:

  * `-m/--minimal-file-size` defaults to 16384 bytes. On a corpus of small or
    downscaled images that silently drops files: 315 of the 9285 in the first
    corpus here fall under it. This passes `-m 1` unless told otherwise.
  * Czkawka caches hashes between runs keyed on the hash settings. A timed run
    must disable that cache (`-H`) or it measures a database read, not a scan.

It also exits 11 rather than 0 whenever it finds anything, so `-W` is passed to
keep a successful scan from looking like a failure.

    python3 run_czkawka.py CORPUS -o out/czkawka.json --bin vendor/bin/czkawka_cli
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--bin", default=os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "..", "vendor", "bin", "czkawka_cli"))
    ap.add_argument("-s", "--max-difference", type=int, default=5,
                    help="0-40, czkawka's default is 5")
    ap.add_argument("-g", "--hash-alg", default="Gradient",
                    choices=["Mean", "Gradient", "Blockhash", "VertGradient",
                             "DoubleGradient", "Median"])
    ap.add_argument("-c", "--hash-size", type=int, default=16,
                    choices=[8, 16, 32, 64])
    ap.add_argument("-z", "--image-filter", default="Nearest")
    ap.add_argument("--geometric-invariance", default="off",
                    choices=["off", "mirror-flip", "mirror-flip-rotate90"])
    ap.add_argument("-m", "--minimal-file-size", type=int, default=1,
                    help="bytes; czkawka's own default of 16384 skips small images")
    ap.add_argument("-R", "--not-recursive", action="store_true")
    ap.add_argument("-T", "--thread-number", type=int, default=0)
    ap.add_argument("--keep-cache", action="store_true",
                    help="allow czkawka's hash cache (faster, but not a cold run)")
    args = ap.parse_args()

    raw = tempfile.NamedTemporaryFile(suffix=".json", delete=False)
    raw.close()
    cmd = [os.path.abspath(args.bin), "image"]
    for root in args.roots:
        cmd += ["-d", os.path.abspath(root)]
    cmd += ["-s", str(args.max_difference),
            "-g", args.hash_alg,
            "-c", str(args.hash_size),
            "-z", args.image_filter,
            "--geometric-invariance", args.geometric_invariance,
            "-m", str(args.minimal_file_size),
            "-T", str(args.thread_number),
            "-C", raw.name,
            "-W"]                       # exit 0 even when groups are found
    if not args.keep_cache:
        cmd.append("-H")                # no cache read or write: a real cold scan
    if args.not_recursive:
        cmd.append("-R")

    started = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.time() - started
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout[-4000:] + proc.stderr[-4000:])
        sys.exit(f"czkawka_cli exited {proc.returncode}")

    with open(raw.name) as fh:
        groups = json.load(fh)
    os.unlink(raw.name)

    result = {
        "tool": "czkawka",
        "config": {"max_difference": args.max_difference,
                   "hash_alg": args.hash_alg,
                   "hash_size": args.hash_size,
                   "image_filter": args.image_filter,
                   "geometric_invariance": args.geometric_invariance,
                   "minimal_file_size": args.minimal_file_size,
                   "cache": bool(args.keep_cache)},
        "command": " ".join(cmd),
        "runtime_seconds": round(elapsed, 2),
        "groups": [sorted(f["path"] for f in g) for g in groups],
        "pairs": [],
    }
    # czkawka reports a group with each member's distance from the first file,
    # not pairwise distances, so pairs are the group's closure.
    for g in result["groups"]:
        for i in range(len(g)):
            for j in range(i + 1, len(g)):
                result["pairs"].append({"a": g[i], "b": g[j]})

    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    print(f"{len(result['groups'])} groups, {len(result['pairs'])} pairs "
          f"in {elapsed:.1f}s -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
