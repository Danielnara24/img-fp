#!/usr/bin/env python3
"""Drive dupeGuru's picture engine headlessly and emit groups.

dupeGuru ships as a GUI with no CLI, but its matcher is a library: the 15x15
average-colour block comparison in `core.pe.matchblock`, backed by the `_block`
C extension. This imports that matcher directly, so dupeGuru can be scored on
the same corpus as every CLI tool instead of being a hand-run GUI row.

Two pieces the GUI normally supplies are supplied here:

  * a `Photo` subclass. Upstream has one per platform (Qt's uses QImage); this
    one uses PIL, so no Qt is needed. It feeds the same `getblocks2` C function
    the Qt one does, and applies the same EXIF orientation transforms.
  * the threshold. dupeGuru calls it "filter hardness" and defaults to 95.

`--match-scaled` is dupeGuru's "match pictures of different dimensions", off by
default in the GUI and here. Leaving it off means the tool never compares two
images of different sizes at all, which on a corpus of resized copies is the
single biggest thing separating its recall from everyone else's.

Requires the C extensions: (cd vendor/dupeguru && python build_pe_modules.py)

    python3 run_dupeguru.py CORPUS -o out/dupeguru.json --threshold 95
"""
import argparse
import json
import logging
import os
import sys
import tempfile
import time
from pathlib import Path

HERE = os.path.dirname(os.path.abspath(__file__))
DUPEGURU_SRC = os.path.join(HERE, "..", "..", "vendor", "dupeguru", "src")
sys.path.insert(0, os.path.abspath(DUPEGURU_SRC))

from PIL import Image  # noqa: E402

try:
    from core.pe import matchblock  # noqa: E402
    from core.pe.block import getblocks2  # noqa: E402
    from core.pe.photo import Photo as PhotoBase  # noqa: E402
except ImportError as exc:  # pragma: no cover
    sys.exit(f"dupeGuru not importable ({exc}).\n"
             f"Build its C extensions first:\n"
             f"  cd {os.path.abspath(os.path.join(DUPEGURU_SRC, '..'))} "
             f"&& python build_pe_modules.py")

EXTS = {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tif", ".tiff"}

# EXIF orientation -> PIL transpose ops, mirroring qt/pe/photo.py
ORIENTATION_OPS = {
    2: [Image.FLIP_LEFT_RIGHT],
    3: [Image.ROTATE_180],
    4: [Image.FLIP_TOP_BOTTOM],
    5: [Image.FLIP_LEFT_RIGHT, Image.ROTATE_270],
    6: [Image.ROTATE_270],
    7: [Image.FLIP_LEFT_RIGHT, Image.ROTATE_90],
    8: [Image.ROTATE_90],
}


class PilPhoto(PhotoBase):
    """The platform-specific half of dupeGuru's Photo, backed by PIL."""

    def _plat_get_dimensions(self):
        try:
            with Image.open(str(self.path)) as im:
                return im.size
        except Exception:
            logging.warning("could not read dimensions of %s", self.path)
            return (0, 0)

    def _plat_get_blocks(self, block_count_per_side, orientation):
        with Image.open(str(self.path)) as im:
            image = im.convert("RGB")
            try:
                orientation = int(orientation)
            except Exception:
                orientation = 1
            for op in ORIENTATION_OPS.get(orientation, []):
                image = image.transpose(op)
            return getblocks2(image, block_count_per_side)


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


def union_find(n, pairs):
    parent = list(range(n))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for i, j in pairs:
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
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("-t", "--threshold", type=int, default=95,
                    help="dupeGuru's filter hardness, 1-100 (GUI default 95)")
    ap.add_argument("--match-scaled", action="store_true",
                    help="compare images of different dimensions (GUI default: off)")
    ap.add_argument("-r", "--recursive", action="store_true", default=True)
    ap.add_argument("--not-recursive", dest="recursive", action="store_false")
    ap.add_argument("--cache", default=None,
                    help="block cache path; a fresh temp db each run by default")
    args = ap.parse_args()

    logging.basicConfig(level=logging.ERROR)

    started = time.time()
    files = sorted(walk(args.roots, args.recursive, EXTS))
    print(f"{len(files)} files", file=sys.stderr)

    pictures = []
    for path in files:
        photo = PilPhoto(Path(path))
        photo.is_ref = False
        pictures.append(photo)

    cache_dir = None
    if args.cache:
        cache_path = args.cache
    else:
        cache_dir = tempfile.mkdtemp(prefix="dupeguru-cache-")
        cache_path = os.path.join(cache_dir, "blocks.db")

    matches = matchblock.getmatches(pictures, cache_path, args.threshold,
                                    match_scaled=args.match_scaled)

    index = {id(p): i for i, p in enumerate(pictures)}
    pairs, pair_rows = [], []
    for match in matches:
        i, j = index[id(match.first)], index[id(match.second)]
        if i > j:
            i, j = j, i
        pairs.append((i, j))
        pair_rows.append({"a": files[i], "b": files[j],
                          "percentage": int(match.percentage)})
    groups = union_find(len(files), pairs)

    result = {
        "tool": "dupeguru",
        "config": {"threshold": args.threshold,
                   "match_scaled": args.match_scaled,
                   "block_count_per_side": matchblock.BLOCK_COUNT_PER_SIDE},
        "files_enumerated": len(files),
        "runtime_seconds": round(time.time() - started, 2),
        "groups": [sorted(files[i] for i in g) for g in
                   sorted(groups, key=lambda g: files[g[0]])],
        "pairs": pair_rows,
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    if cache_dir:
        import shutil
        shutil.rmtree(cache_dir, ignore_errors=True)
    print(f"{len(groups)} groups, {len(pair_rows)} pairs -> {args.output}",
          file=sys.stderr)


if __name__ == "__main__":
    main()
