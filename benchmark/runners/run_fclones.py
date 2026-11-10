#!/usr/bin/env python3
"""Run fclones and normalise its output to the benchmark's shape.

fclones is not an image tool. It finds byte-identical files by size then hash,
and it is in the pool as the floor: whatever a perceptual matcher costs, it
should buy something over this. It is also the only entrant that cannot be
fooled by a container change and cannot possibly find a re-encode.

Its own JSON nests the groups under a header and reports files as objects, so
this flattens them and adds the runtime and the explicit pair list every other
runner emits.

    python3 run_fclones.py CORPUS -o out/fclones.json
"""
import argparse
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_BIN = os.path.join(HERE, "..", "..", "vendor", "bin", "fclones")


def files_of(group):
    """fclones reports either bare paths or {path: ...} objects."""
    out = []
    for f in group.get("files", []):
        if isinstance(f, str):
            out.append(f)
        elif isinstance(f, dict):
            for k in ("path", "Path", "file"):
                if k in f:
                    out.append(f[k])
                    break
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--bin", default=DEFAULT_BIN)
    ap.add_argument("--min-size", default="1",
                    help="fclones skips small files by default; this does not")
    args = ap.parse_args()

    cmd = [os.path.abspath(args.bin), "group", "--format", "json",
           "-s", args.min_size] + [os.path.abspath(r) for r in args.roots]

    started = time.time()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.time() - started
    if proc.returncode != 0:
        sys.exit(f"fclones exited {proc.returncode}\n{proc.stderr[-2000:]}")

    data = json.loads(proc.stdout)
    raw = data.get("groups", data) if isinstance(data, dict) else data

    groups, pairs = [], []
    for g in raw:
        fs = sorted(files_of(g))
        if len(fs) < 2:
            continue
        groups.append(fs)
        for i in range(len(fs)):
            for j in range(i + 1, len(fs)):
                pairs.append({"a": fs[i], "b": fs[j]})

    out = {"tool": "fclones",
           "config": {"min_size": args.min_size},
           "runtime_seconds": round(elapsed, 3),
           "groups": groups, "pairs": pairs}
    with open(args.output, "w") as fh:
        json.dump(out, fh)
    print(f"{len(groups)} groups, {len(pairs)} pairs -> {args.output}",
          file=sys.stderr)


if __name__ == "__main__":
    main()
