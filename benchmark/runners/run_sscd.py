#!/usr/bin/env python3
"""Run SSCD (Meta, CVPR 2022) over a corpus and emit groups.

SSCD is the self-supervised descriptor trained for image *copy* detection
rather than semantic similarity, so it is the pool's independent proposer:
nothing it finds comes from a perceptual hash, which is what every other tool
here is built on.

The model emits L2-normalised 512-d descriptors. SSCD's own README puts cosine
0.75 at roughly 90% precision on DISC2021; the oracle wants recall, so this
defaults lower and leaves the rejecting to the human.

    python3 run_sscd.py CORPUS -o out/sscd.json --threshold 0.6
"""
import argparse
import json
import os
import sys
import time

import numpy as np
import torch
from PIL import Image
from torch.utils.data import DataLoader, Dataset
from torchvision import transforms

EXTS = {".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tif", ".tiff", ".webp",
        ".avif", ".heic", ".heif", ".jxl", ".ppm", ".pgm"}

NORMALIZE = transforms.Normalize(mean=[0.485, 0.456, 0.406],
                                 std=[0.229, 0.224, 0.225])


def build_transform(size, mode):
    if mode == "square":
        resize = [transforms.Resize((size, size))]
    else:  # small edge, preserving aspect
        resize = [transforms.Resize(size)]
    return transforms.Compose(resize + [transforms.ToTensor(), NORMALIZE])


class Images(Dataset):
    def __init__(self, paths, tf):
        self.paths, self.tf = paths, tf

    def __len__(self):
        return len(self.paths)

    def __getitem__(self, i):
        path = self.paths[i]
        try:
            with Image.open(path) as im:
                return self.tf(im.convert("RGB")), i, ""
        except Exception as exc:
            return torch.zeros(3, 1, 1), -(i + 1), f"{type(exc).__name__}: {exc}"


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


def pairs_above(emb, threshold, chunk=512):
    """Every (i, j, cosine) with i < j and cosine >= threshold."""
    n = len(emb)
    for start in range(0, n, chunk):
        stop = min(start + chunk, n)
        sim = emb[start:stop] @ emb[start:].T
        rows, cols = np.nonzero(sim >= threshold)
        for r, c in zip(rows, cols):
            i, j = start + int(r), start + int(c)
            if i < j:
                yield i, j, float(sim[r, c])


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
    ap.add_argument("roots", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("-m", "--model", default=os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..", "..", "vendor", "models", "sscd_disc_mixup.torchscript.pt"))
    ap.add_argument("-t", "--threshold", type=float, default=0.6,
                    help="min cosine similarity (default 0.6; 0.75 ~ 90%% precision)")
    ap.add_argument("--size", type=int, default=320)
    ap.add_argument("--resize", choices=["square", "small-edge"], default="square",
                    help="square needs a fixed size to batch; small-edge forces batch 1")
    ap.add_argument("-b", "--batch-size", type=int, default=16)
    ap.add_argument("-j", "--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    ap.add_argument("-r", "--recursive", action="store_true", default=True)
    ap.add_argument("--not-recursive", dest="recursive", action="store_false")
    ap.add_argument("--embeddings", default=None,
                    help="cache .npz of embeddings here; reused if it matches the file list")
    args = ap.parse_args()

    started = time.time()
    files = sorted(walk(args.roots, args.recursive, EXTS))
    print(f"{len(files)} files", file=sys.stderr)

    cache_hit = False
    if args.embeddings and os.path.exists(args.embeddings):
        cached = np.load(args.embeddings, allow_pickle=True)
        # "enumerated" is the file list the cache was built from; "paths" is the
        # subset that actually embedded. Comparing against the former is what
        # makes the cache safe when some files failed to read.
        if "enumerated" in cached.files and list(cached["enumerated"]) == files:
            paths = list(cached["paths"])
            emb = cached["emb"]
            failures = list(cached["failures"])
            cache_hit = True
            print(f"reused cached embeddings ({len(paths)} vectors)", file=sys.stderr)

    if not cache_hit:
        model = torch.jit.load(args.model)
        model.eval()
        torch.set_num_threads(os.cpu_count() or 1)

        batch_size = 1 if args.resize == "small-edge" else args.batch_size
        loader = DataLoader(Images(files, build_transform(args.size, args.resize)),
                            batch_size=batch_size, num_workers=args.jobs,
                            shuffle=False)

        vectors, keep, failures = [], [], []
        done = 0
        with torch.no_grad():
            for batch, idx, err in loader:
                good = idx >= 0
                for k, ok in enumerate(good.tolist()):
                    if not ok:
                        failures.append({"path": files[-int(idx[k]) - 1],
                                         "error": err[k]})
                if good.any():
                    out = model(batch[good])
                    vectors.append(out.numpy().astype(np.float32))
                    keep.extend(int(v) for v in idx[good])
                done += len(idx)
                if done % (batch_size * 20) < batch_size:
                    rate = done / (time.time() - started)
                    print(f"  {done}/{len(files)}  {rate:.1f} img/s", file=sys.stderr)

        paths = [files[i] for i in keep]
        emb = np.concatenate(vectors) if vectors else np.zeros((0, 512), np.float32)
        emb /= np.linalg.norm(emb, axis=1, keepdims=True).clip(min=1e-12)
        if args.embeddings:
            os.makedirs(os.path.dirname(os.path.abspath(args.embeddings)) or ".",
                        exist_ok=True)
            np.savez(args.embeddings,
                     enumerated=np.array(files),
                     paths=np.array(paths),
                     emb=emb,
                     failures=np.array(failures, dtype=object))

    print(f"embedded {len(paths)} in {time.time() - started:.1f}s; matching",
          file=sys.stderr)
    pairs = list(pairs_above(emb, args.threshold))
    groups = union_find(len(paths), pairs)

    result = {
        "tool": "sscd",
        "config": {"threshold": args.threshold, "model": os.path.basename(args.model),
                   "size": args.size, "resize": args.resize},
        "files_enumerated": len(files),
        "files_embedded": len(paths),
        "failures": failures,
        "runtime_seconds": round(time.time() - started, 2),
        "groups": [sorted(paths[i] for i in g) for g in
                   sorted(groups, key=lambda g: paths[g[0]])],
        "pairs": [{"a": paths[i], "b": paths[j], "cosine": round(s, 4)}
                  for i, j, s in pairs],
    }
    os.makedirs(os.path.dirname(os.path.abspath(args.output)) or ".", exist_ok=True)
    with open(args.output, "w") as fh:
        json.dump(result, fh, indent=1)
    print(f"{len(groups)} groups, {len(pairs)} pairs -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
