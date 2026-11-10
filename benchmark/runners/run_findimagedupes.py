#!/usr/bin/env python3
"""Run findimagedupes and emit groups in the benchmark's format.

findimagedupes is the twenty-year-old Linux baseline: a 16x16 monochrome
fingerprint, one bit per cell, compared by bit overlap. It sets the "what did
people use before any of this" line.

    python3 run_findimagedupes.py CORPUS -o out/findimagedupes.json -t 90%

Note on parsing: findimagedupes prints one group per line, paths separated by
single spaces, and has no machine-readable output mode. A path containing a
space is therefore unparseable, and this refuses to guess — it fails loudly
instead of silently splitting one file into two.
"""
import argparse
import json
import os
import subprocess
import sys
import shutil
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_BIN = os.path.join(HERE, "..", "..", "vendor", "bin", "findimagedupes")

EXTS = {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tif", ".tiff", ".webp"}


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


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--bin", default=DEFAULT_BIN)
    ap.add_argument("-t", "--threshold", default="90%",
                    help="bit-overlap threshold, e.g. 90%% (findimagedupes' default)")
    ap.add_argument("--not-recursive", action="store_true")
    ap.add_argument("--farm-dir", default="~/.cache",
                    help="where the space-free hard-link farm goes; must be on "
                         "the same filesystem as the corpus")
    args = ap.parse_args()

    files = sorted(walk(args.roots, not args.not_recursive, EXTS))

    # findimagedupes separates the files of a group with a space and offers no
    # NUL mode, so a path containing a space makes its output ambiguous — and
    # this corpus has a "WhatsApp Images" folder and an "image (1234).jpg"
    # naming pattern. Refusing to run would score the tool zero for what is
    # really a wrapper problem, so it gets the same treatment imagededup
    # already gets: a farm of space-free names, mapped back afterwards.
    #
    # They have to be HARD links. Symlinks do not work: findimagedupes
    # resolves them and prints the target, so the spaces come straight back
    # and the output is unparseable again. Hard links have no target to
    # resolve, which is also why the farm lives under $HOME — it must be on
    # the same filesystem as the corpus.
    farm = None
    real = {}
    if any(" " in f for f in files):
        farm = tempfile.mkdtemp(prefix="fid-farm-",
                                dir=os.path.expanduser(args.farm_dir))
        linked = []
        for i, src in enumerate(files):
            ext = os.path.splitext(src)[1].lower() or ".img"
            dst = os.path.join(farm, f"{i:07d}{ext}")
            try:
                os.link(src, dst)
            except OSError:
                os.symlink(src, dst)
            real[dst] = src
            linked.append(dst)
        scan = linked
    else:
        scan = files

    # Feed the file list on stdin so this tool enumerates exactly what the
    # others do, rather than relying on its own directory walk and extensions.
    cmd = [os.path.abspath(args.bin), "-t", args.threshold, "--", "-"]
    started = time.time()
    try:
        proc = subprocess.run(cmd, input="\n".join(scan) + "\n",
                              capture_output=True, text=True)
    finally:
        elapsed = time.time() - started
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-4000:])
        sys.exit(f"findimagedupes exited {proc.returncode}")

    known = set(scan)
    groups, unknown = [], set()
    for line in proc.stdout.splitlines():
        parts = [p for p in line.strip().split(" ") if p]
        if len(parts) < 2:
            continue
        unknown.update(p for p in parts if p not in known)
        groups.append(sorted(real.get(p, p) for p in parts))
    if farm:
        shutil.rmtree(farm, ignore_errors=True)
    if unknown:
        sys.exit(f"unparseable output: {len(unknown)} token(s) are not files "
                 f"we passed in, e.g. {sorted(unknown)[0]!r}")

    pairs = [{"a": g[i], "b": g[j]}
             for g in groups for i in range(len(g)) for j in range(i + 1, len(g))]

    result = {
        "tool": "findimagedupes",
        "config": {"threshold": args.threshold},
        "command": " ".join(cmd),
        "files_enumerated": len(files),
        "symlink_farm": bool(farm),
        "stderr_tail": proc.stderr[-2000:],
        "runtime_seconds": round(elapsed, 2),
        "groups": sorted(groups, key=lambda g: g[0]),
        "pairs": pairs,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    print(f"{len(groups)} groups, {len(pairs)} pairs in {elapsed:.1f}s "
          f"-> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
