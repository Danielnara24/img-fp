#!/usr/bin/env python3
"""Generate the transformed half of the benchmark corpus, with its ground truth.

Every seed image is put through the catalogue in `transforms.py` — re-encodes,
format changes, resizes, crops, colour shifts, rotations, degradations and the
composite chains real software actually applies — and each output lands under a
plausible-looking folder with a plausible-looking filename.

The filenames are deliberately uninformative. Nothing in a derived file's name
says which seed it came from or what was done to it, so a tool cannot do well
here by accident. `manifest.csv` is the only link back.

Because the corpus is *made* rather than found, the ground truth is exact and
free in BOTH directions. Within a seed family we know which file came from
which photograph and which region survived, so the positives are computable.
Across families we know the two files descend from different photographs, so
every cross-family pair is a negative — and there are millions of them. That is
what removes the need to hand-label anything and what lets one F1 stand for the
whole corpus.

One catalogue runs over every seed, wherever it sits. It used to be two —
one for the corpus root and one for `archive/` — which meant a tool's score
depended on which folder a photograph happened to be filed in.

Every transformation with an amount draws it per seed, from the seed's own
pixels, so the corpus is still byte-for-byte reproducible while no
transformation is a single fixed point. `seed_key` in the manifest is that
draw; `region`, `coverage`, `width` and `height` record what it did.

    python3 make_variants.py --corpus /home/daniel/Documents/IMGS

Re-running is idempotent: same inputs, same bytes, same names.
"""
import argparse
import csv
import gc
import hashlib
import inspect
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
from transforms import (ALPHA_CATALOGUE, CATALOGUE, DYNAMIC_REGIONS, FULL,  # noqa: E402
                        alpha_background, flatten_onto, seed_key_of)

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
    """Every image under the corpus except the derived tree, in sorted order.

    One catalogue now applies to every seed, so there is no longer anything to
    gain from keeping some of them in a separate folder — where a photograph
    sits no longer decides which transformations it faces.
    """
    seeds = []
    for dirpath, dirnames, filenames in os.walk(corpus):
        dirnames[:] = sorted(d for d in dirnames
                             if os.path.join(dirpath, d) not in exclude)
        for name in sorted(filenames):
            path = os.path.join(dirpath, name)
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
    return sorted(seeds)


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


def resolve_region(variant, size, key):
    """The rectangle of the original that survives this variant.

    Callables taking a third argument get the seed's key, because catalogue C
    varies a transform's strength per seed and the region has to vary with it.
    The region callable and the transform itself are always produced by the
    same factory from the same formula, so they cannot drift apart.
    """
    fn = DYNAMIC_REGIONS.get(variant.name)
    if fn is None:
        return variant.region
    if len(inspect.signature(fn).parameters) >= 3:
        return fn(size[0], size[1], key)
    return fn(size[0], size[1])


def resolve(value, key):
    """A catalogue field that may be a constant or a function of the seed key."""
    return value(key) if callable(value) else value


def save(image, fmt, out_path, kwargs, exif_orientation=None, writer=None, key=0):
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    if writer is not None:
        # An external encoder owns the output path; save_kwargs do not apply.
        writer(image, out_path, key)
        return
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


def release(path):
    """Drop a freshly written file from the page cache.

    The corpus is 2.3 GB and every byte of it was just written, so without this
    the run ends with most of that sitting in cache as clean pages. They are
    reclaimable, but `free` collapses long before the kernel bothers, and on a
    machine already deep into swap that is the difference between finishing and
    being killed. `bench.py` uses the same call to evict the corpus before a
    timed run; here it is used to avoid filling the cache in the first place.
    """
    try:
        fd = os.open(path, os.O_RDONLY)
    except OSError:
        return
    try:
        os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
    except (AttributeError, OSError):
        pass
    finally:
        os.close(fd)


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


def pair_relation(a, b, same_iou=0.95, contain=0.95):
    """SAME / CROP / PARTIAL / DIFFERENT for two manifest rows.

    Region alone is not enough once catalogue B is in play. Three things decide
    it, in order:

      * **Scramble.** A shuffled tiling holds every pixel of the original and
        is not the original. If the two sides were rearranged differently, the
        answer is DIFFERENT no matter what their regions say. This is the only
        rule that can override a perfect region match, and it exists to catch
        anything that reduces an image to a global colour summary.
      * **Region.** What rectangle of the original each side still carries.
      * **Coverage.** What fraction of each derived file is original content. A
        slide holding a photograph carries 100% of the region at 35% coverage;
        against the photograph itself that is a containment, not an equality.
        Without this, "the image" and "a poster with the image on it" would be
        called the same file.
    """
    if (a.get("scramble") or "") != (b.get("scramble") or ""):
        return "DIFFERENT", 0.0
    rel, iou = rect_relation(json.loads(a["region"]), json.loads(b["region"]))
    if rel == "SAME":
        ca, cb = float(a.get("coverage", 1.0)), float(b.get("coverage", 1.0))
        lo, hi = sorted((ca, cb))
        if hi > 0 and lo / hi < 0.9:
            return "CROP", iou
    return rel, iou


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
    ap.add_argument("--alpha", action="store_true",
                    help="a corpus of seeds with transparency (IMGS-ALPHA): the "
                         "standard catalogue runs on each seed flattened onto a "
                         "background drawn per seed, and ALPHA_CATALOGUE runs too")
    ap.add_argument("--clean", action="store_true",
                    help="delete the derived tree first")
    args = ap.parse_args()

    corpus = os.path.abspath(args.corpus)
    derived_root = os.path.join(corpus, args.out_subdir)
    manifest_dir = args.manifest_dir or derived_root

    if args.clean and os.path.isdir(derived_root):
        shutil.rmtree(derived_root)

    excl = [os.path.join(corpus, args.out_subdir)]
    seeds = find_seeds(corpus, excl)
    if not seeds:
        sys.exit(f"no seed images found in {corpus}")
    catalogue = CATALOGUE + ALPHA_CATALOGUE if args.alpha else CATALOGUE
    if args.only:
        wanted = set(args.only.split(","))
        catalogue = [v for v in catalogue if v.name in wanted]
    jobs = [("ONE", catalogue, seeds)]
    print(f"{len(seeds)} seeds x {len(catalogue)} transforms  "
          f"({', '.join(os.path.basename(x) for x in seeds[:4])}"
          f"{', ...' if len(seeds) > 4 else ''})")

    namer = Namer(derived_root)
    rows, failures = [], []
    resumed = 0
    started = time.time()

    for cat_key, catalogue, seed_paths in jobs:
      for seed_path in seed_paths:
        seed_id = os.path.basename(seed_path)
        with Image.open(seed_path) as im:
            im.load()
            source = im.convert("RGB")
            # A transparent seed's pixels under its transparency are not
            # what anyone sees: `convert("RGB")` shows them. Under --alpha
            # the standard variants are of the picture as it is shown on a
            # background of its own (`alpha_background`).
            background = None
            if args.alpha and im.mode in ("RGBA", "LA", "P"):
                background = alpha_background(im.convert("RGBA"))
                source = flatten_onto(im.convert("RGBA"), background)
            # Kept separately: flattening to RGB before the transform runs
            # would delete the alpha channel the alpha_* variants exist to
            # test. Only catalogue B asks for it.
            source_alpha = im.convert("RGBA") if im.mode in ("RGBA", "LA", "P") \
                else None
        size = source.size
        # Catalogue C jitters each transform's strength per seed, deterministically,
        # from this. Recorded below so any realised parameter can be recomputed.
        seed_key = seed_key_of(source)
        rows.append({
            "path": seed_path, "rel_path": os.path.relpath(seed_path, corpus),
            "seed": seed_id, "variant": "ORIGINAL", "format": "",
            "region": json.dumps(FULL), "geometry": "none",
            "coverage": 1.0, "scramble": "", "catalogue": cat_key,
            "seed_key": seed_key,
            "width": size[0], "height": size[1],
            "bytes": os.path.getsize(seed_path), "sha256": sha256(seed_path),
            "note": "seed image, unmodified" + (f"; standard variants on {background}" if background else ""),
        })
        print(f"  {seed_id} {size[0]}x{size[1]}")

        gc.collect()
        for variant in catalogue:
            ext = EXT_FOR_FORMAT.get(variant.fmt) or os.path.splitext(seed_path)[1] or ".jpg"
            rel = namer.next(ext)
            out_path = os.path.join(derived_root, os.path.relpath(rel, "derived"))
            # Resume: a run that was interrupted leaves good files behind, and
            # the Namer is a pure function of position, so this run assigns the
            # same path to the same (seed, variant). If that file is already
            # there and reads back, it is the file this run would have written.
            # Generating the corpus takes a quarter of an hour and something on
            # this machine keeps killing long jobs; making restarts cheap is
            # worth more than arguing with it.
            if os.path.exists(out_path):
                try:
                    out_size = verify(out_path, variant.fmt)
                except Exception:
                    discard(out_path)
                else:
                    rows.append({
                        "path": out_path,
                        "rel_path": os.path.relpath(out_path, corpus),
                        "seed": seed_id,
                        "variant": variant.name,
                        "format": variant.fmt,
                        "region": json.dumps(resolve_region(variant, size, seed_key)),
                        "geometry": resolve(variant.geometry, seed_key),
                        "coverage": round(resolve(variant.coverage, seed_key), 6),
                        "scramble": variant.scramble,
                        "catalogue": cat_key,
                        "seed_key": seed_key,
                        "width": out_size[0], "height": out_size[1],
                        "bytes": os.path.getsize(out_path),
                        "sha256": sha256(out_path),
                        "note": variant.note,
                    })
                    resumed += 1
                    release(out_path)
                    continue

            try:
                if variant.fmt == "RAW_COPY":
                    os.makedirs(os.path.dirname(out_path), exist_ok=True)
                    shutil.copy2(seed_path, out_path)
                elif variant.fmt == "EXIF_STRIP":
                    strip_metadata(seed_path, out_path)
                else:
                    src = (source_alpha or source) if variant.wants_alpha else source
                    save(variant.fn(src.copy()), variant.fmt, out_path,
                         resolve(variant.save_kwargs, seed_key),
                         variant.exif_orientation, variant.writer, seed_key)
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

            release(out_path)
            rows.append({
                "path": out_path,
                "rel_path": os.path.relpath(out_path, corpus),
                "seed": seed_id,
                "variant": variant.name,
                "format": variant.fmt,
                "region": json.dumps(resolve_region(variant, size, seed_key)),
                "geometry": resolve(variant.geometry, seed_key),
                "coverage": round(resolve(variant.coverage, seed_key), 6),
                "scramble": variant.scramble,
                "catalogue": cat_key,
                "seed_key": seed_key,
                "width": out_size[0], "height": out_size[1],
                "bytes": os.path.getsize(out_path),
                "sha256": sha256(out_path),
                "note": variant.note,
            })

    os.makedirs(manifest_dir, exist_ok=True)
    fields = ["path", "rel_path", "seed", "variant", "format", "region",
              "geometry", "coverage", "scramble", "catalogue", "seed_key",
              "width", "height", "bytes", "sha256", "note"]
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
                rel, iou = pair_relation(a, b)
                geoms = {a["geometry"], b["geometry"]} - {"none"}
                pairs.append({
                    "a": a["path"], "b": b["path"], "seed": seed_id,
                    "variant_a": a["variant"], "variant_b": b["variant"],
                    "catalogue": a["catalogue"],
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
        "catalogues": {k: {"seeds": len(sd), "transforms": len(c)}
                       for k, c, sd in jobs},
        "files_written": len(rows) - len(seeds),
        "files_reused": resumed,
        "files_total_in_families": len(rows),
        "failures": failures,
        "pair_relations": counts,
        "generated_seconds": round(time.time() - started, 1),
    }
    with open(os.path.join(manifest_dir, "summary.json"), "w") as fh:
        json.dump(summary, fh, indent=1)

    print(f"\nwrote {len(rows) - len(seeds) - resumed} derived files from "
          f"{len(seeds)} seeds in {summary['generated_seconds']}s"
          + (f" ({resumed} already present, reused)" if resumed else ""))
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
