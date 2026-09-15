#!/usr/bin/env python3
"""Generate the transformed half of the benchmark corpus, with its ground truth.

Every seed image is put through the catalogue in `transforms.py` — re-encodes,
format changes, resizes, crops, colour shifts, rotations, degradations and the
composite chains real software actually applies — and each output lands under a
plausible-looking folder with a plausible-looking filename.

The filenames are deliberately uninformative. Nothing in a derived file's name
says which seed it came from or what was done to it, so a tool cannot do well
here by accident. `manifest.csv` is the only link back.

Because the corpus is *made* rather than found, the positive ground truth is
exact and free: we know which file came from which photograph and which region
of it survived. That is the one advantage this half has over the vid-fp
benchmark's, and it is why `pairs.csv` can be generated rather than adjudicated.
It does not remove the need to label what the tools propose *outside* these
families — a derived file matching a butterfly is a false positive nobody has
enumerated in advance.

    python3 make_variants.py --corpus /home/daniel/Documents/IMGS

Re-running is idempotent: same inputs, same bytes, same names.
"""
import argparse
import csv
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from transforms import CATALOGUE, DYNAMIC_REGIONS, FULL  # noqa: E402

try:
    import pillow_heif
    pillow_heif.register_heif_opener()
    HEIF_OK = True
except ImportError:
    HEIF_OK = False

Image.MAX_IMAGE_PIXELS = None

SEED_EXTS = {".jpg", ".jpeg", ".png", ".webp", ".avif", ".heic", ".heif",
             ".tif", ".tiff", ".bmp", ""}

EXT_FOR_FORMAT = {
    "JPEG": ".jpg", "PNG": ".png", "WEBP": ".webp", "AVIF": ".avif",
    "HEIF": ".heic", "TIFF": ".tif", "GIF": ".gif", "BMP": ".bmp",
    "JXL": ".jxl",
}

# Where derived files live. Real duplicates are scattered, not filed together.
FOLDERS = [
    "derived/Downloads",
    "derived/Pictures/2023",
    "derived/Pictures/2024",
    "derived/Pictures/Camera Roll",
    "derived/Pictures/edited",
    "derived/WhatsApp Images",
    "derived/Desktop",
    "derived/Backup/old-phone",
    "derived/Documents/scans",
]

# Deliberately ordinary names. None of them encodes the transform.
NAME_PATTERNS = [
    "IMG_{n:04d}",
    "DSC{n:05d}",
    "PXL_2024{mm:02d}{dd:02d}_{n:06d}",
    "image ({n})",
    "photo-{n}",
    "Screenshot from 2024-{mm:02d}-{dd:02d} {hh:02d}-{mi:02d}-{ss:02d}",
    "{n}",
    "received_{n}",
    "Foto {n}",
    "scan_{n:03d}",
]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def find_seeds(corpus, exclude):
    """Images sitting directly in the corpus root, not in the bulk archive."""
    seeds = []
    for name in sorted(os.listdir(corpus)):
        path = os.path.join(corpus, name)
        if not os.path.isfile(path):
            continue
        if any(name.endswith(e) for e in (".csv", ".json", ".md", ".txt")):
            continue
        if os.path.splitext(name)[1].lower() not in SEED_EXTS:
            continue
        if any(x in path for x in exclude):
            continue
        try:
            with Image.open(path) as im:
                im.verify()
        except Exception as exc:
            print(f"  skipping unreadable seed {name}: {exc}", file=sys.stderr)
            continue
        seeds.append(path)
    return seeds


class Namer:
    """Deterministic, collision-free, realistic-looking output paths."""

    def __init__(self, root):
        self.root = root
        self.used = set()
        self.n = 0

    def next(self, ext):
        while True:
            self.n += 1
            folder = FOLDERS[self.n % len(FOLDERS)]
            pattern = NAME_PATTERNS[(self.n * 7) % len(NAME_PATTERNS)]
            stem = pattern.format(
                n=1000 + self.n * 13,
                mm=1 + (self.n * 5) % 12, dd=1 + (self.n * 11) % 28,
                hh=(self.n * 3) % 24, mi=(self.n * 17) % 60, ss=(self.n * 29) % 60)
            rel = os.path.join(folder, stem + ext)
            if rel not in self.used:
                self.used.add(rel)
                return rel


def resolve_region(variant, size):
    fn = DYNAMIC_REGIONS.get(variant.name)
    if fn is None:
        return variant.region
    return fn(size[0], size[1])


def save(image, fmt, out_path, kwargs, exif_orientation=None):
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    if fmt == "JXL":
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
            tmp_png = tmp.name
        try:
            image.save(tmp_png, "PNG")
            subprocess.run(["cjxl", tmp_png, out_path,
                            "-q", str(kwargs.get("quality", 85))],
                           check=True, capture_output=True)
        finally:
            os.unlink(tmp_png)
        return
    if fmt == "HEIF" and not HEIF_OK:
        raise RuntimeError("pillow-heif not installed")

    save_kwargs = dict(kwargs)
    if exif_orientation is not None:
        exif = Image.Exif()
        exif[0x0112] = exif_orientation
        save_kwargs["exif"] = exif
    image.save(out_path, fmt, **save_kwargs)


def strip_metadata(src, dst):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy2(src, dst)
    proc = subprocess.run(["exiftool", "-all=", "-overwrite_original", dst],
                          capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr.strip()[:200])


def discard(path):
    """Remove a failed output, and any directory it left empty."""
    try:
        os.unlink(path)
    except FileNotFoundError:
        return
    try:
        os.removedirs(os.path.dirname(path))
    except OSError:
        pass


def verify(path, fmt):
    """Read the file back and return its dimensions.

    Pillow cannot read JPEG XL — which is itself worth knowing, since it is
    what most of the tools in this benchmark are built on — so those go back
    through djxl instead of being reported as a write failure.
    """
    if fmt == "JXL":
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as tmp:
            tmp_png = tmp.name
        try:
            subprocess.run(["djxl", path, tmp_png], check=True, capture_output=True)
            with Image.open(tmp_png) as check:
                return check.size
        finally:
            os.unlink(tmp_png)
    with Image.open(path) as check:
        return check.size


def rect_relation(a, b, same_iou=0.95, contain=0.95):
    """SAME / CROP / PARTIAL / DIFFERENT from two normalised rectangles."""
    ax0, ay0, ax1, ay1 = a
    bx0, by0, bx1, by1 = b
    ix0, iy0 = max(ax0, bx0), max(ay0, by0)
    ix1, iy1 = min(ax1, bx1), min(ay1, by1)
    if ix1 <= ix0 or iy1 <= iy0:
        return "DIFFERENT", 0.0
    inter = (ix1 - ix0) * (iy1 - iy0)
    area_a = (ax1 - ax0) * (ay1 - ay0)
    area_b = (bx1 - bx0) * (by1 - by0)
    union = area_a + area_b - inter
    iou = inter / union if union else 0.0
    if iou >= same_iou:
        return "SAME", iou
    if inter / min(area_a, area_b) >= contain:
        return "CROP", iou
    return "PARTIAL", iou


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--corpus", default="/home/daniel/Documents/IMGS")
    ap.add_argument("--out-subdir", default="derived")
    ap.add_argument("--manifest-dir", default=None,
                    help="where manifest.csv / pairs.csv go (default: <corpus>/derived)")
    ap.add_argument("--only", default=None,
                    help="comma-separated variant names, for a quick test run")
    ap.add_argument("--clean", action="store_true",
                    help="delete the derived tree first")
    args = ap.parse_args()

    corpus = os.path.abspath(args.corpus)
    derived_root = os.path.join(corpus, args.out_subdir)
    manifest_dir = args.manifest_dir or derived_root

    if args.clean and os.path.isdir(derived_root):
        shutil.rmtree(derived_root)

    seeds = find_seeds(corpus, exclude=[os.path.join(corpus, args.out_subdir)])
    if not seeds:
        sys.exit(f"no seed images found directly in {corpus}")
    print(f"{len(seeds)} seeds: {', '.join(os.path.basename(s) for s in seeds)}")

    catalogue = CATALOGUE
    if args.only:
        wanted = set(args.only.split(","))
        catalogue = [v for v in catalogue if v.name in wanted]

    namer = Namer(derived_root)
    rows, failures = [], []
    started = time.time()

    for seed_path in seeds:
        seed_id = os.path.basename(seed_path)
        with Image.open(seed_path) as im:
            source = im.convert("RGB")
            source.load()
        size = source.size
        rows.append({
            "path": seed_path, "rel_path": os.path.relpath(seed_path, corpus),
            "seed": seed_id, "variant": "ORIGINAL", "format": "",
            "region": json.dumps(FULL), "geometry": "none",
            "width": size[0], "height": size[1],
            "bytes": os.path.getsize(seed_path), "sha256": sha256(seed_path),
            "note": "seed image, unmodified",
        })
        print(f"  {seed_id} {size[0]}x{size[1]}")

        for variant in catalogue:
            ext = EXT_FOR_FORMAT.get(variant.fmt) or os.path.splitext(seed_path)[1] or ".jpg"
            rel = namer.next(ext)
            out_path = os.path.join(derived_root, os.path.relpath(rel, "derived"))
            try:
                if variant.fmt == "RAW_COPY":
                    os.makedirs(os.path.dirname(out_path), exist_ok=True)
                    shutil.copy2(seed_path, out_path)
                elif variant.fmt == "EXIF_STRIP":
                    strip_metadata(seed_path, out_path)
                else:
                    save(variant.fn(source.copy()), variant.fmt, out_path,
                         variant.save_kwargs, variant.exif_orientation)
            except Exception as exc:
                discard(out_path)
                failures.append({"seed": seed_id, "variant": variant.name,
                                 "error": f"{type(exc).__name__}: {exc}"})
                continue

            try:
                out_size = verify(out_path, variant.fmt)
            except Exception as exc:
                # A write that produced an unreadable file must not be left on
                # disk: it would sit in the corpus unmentioned by the manifest,
                # and every tool would report it as a failure nobody asked for.
                discard(out_path)
                failures.append({"seed": seed_id, "variant": variant.name,
                                 "error": f"unreadable after write: {exc}"})
                continue

            rows.append({
                "path": out_path,
                "rel_path": os.path.relpath(out_path, corpus),
                "seed": seed_id,
                "variant": variant.name,
                "format": variant.fmt,
                "region": json.dumps(resolve_region(variant, size)),
                "geometry": variant.geometry,
                "width": out_size[0], "height": out_size[1],
                "bytes": os.path.getsize(out_path),
                "sha256": sha256(out_path),
                "note": variant.note,
            })

    os.makedirs(manifest_dir, exist_ok=True)
    fields = ["path", "rel_path", "seed", "variant", "format", "region",
              "geometry", "width", "height", "bytes", "sha256", "note"]
    with open(os.path.join(manifest_dir, "manifest.csv"), "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=fields)
        w.writeheader()
        w.writerows(rows)

    # ---- generated pair ground truth, within each seed family -------------
    by_seed = {}
    for row in rows:
        by_seed.setdefault(row["seed"], []).append(row)

    pairs, counts = [], {}
    for seed_id, family in by_seed.items():
        for i in range(len(family)):
            for j in range(i + 1, len(family)):
                a, b = family[i], family[j]
                rel, iou = rect_relation(json.loads(a["region"]),
                                         json.loads(b["region"]))
                geoms = {a["geometry"], b["geometry"]} - {"none"}
                pairs.append({
                    "a": a["path"], "b": b["path"], "seed": seed_id,
                    "variant_a": a["variant"], "variant_b": b["variant"],
                    "relation": rel, "iou": round(iou, 4),
                    "geometry": ",".join(sorted(geoms)),
                    "identical_bytes": a["sha256"] == b["sha256"],
                })
                counts[rel] = counts.get(rel, 0) + 1

    with open(os.path.join(manifest_dir, "pairs.csv"), "w", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=list(pairs[0]) if pairs else ["a"])
        w.writeheader()
        w.writerows(pairs)

    summary = {
        "corpus": corpus,
        "seeds": len(seeds),
        "variants_per_seed": len(catalogue),
        "files_written": len(rows) - len(seeds),
        "files_total_in_families": len(rows),
        "failures": failures,
        "pair_relations": counts,
        "generated_seconds": round(time.time() - started, 1),
    }
    with open(os.path.join(manifest_dir, "summary.json"), "w") as fh:
        json.dump(summary, fh, indent=1)

    print(f"\nwrote {len(rows) - len(seeds)} derived files from {len(seeds)} seeds "
          f"in {summary['generated_seconds']}s")
    print(f"pair relations: {counts}")
    if failures:
        print(f"\n{len(failures)} failures:")
        seen = set()
        for f in failures:
            key = (f["variant"], f["error"][:60])
            if key not in seen:
                seen.add(key)
                print(f"  {f['variant']}: {f['error'][:140]}")
    print(f"\nmanifest: {manifest_dir}/manifest.csv, pairs.csv, summary.json")


if __name__ == "__main__":
    main()
