#!/usr/bin/env python3
"""Prototype of img-fp's decision logic on OpenCV SIFT, to fix thresholds
before the Rust implementation. Evaluates verification on selected families
(all in-family pairs) and on the hard cross-family pairs.

Outputs one CSV row per evaluated pair with every statistic the decision
could use, plus the ground-truth relation. Analysis is a separate step.
"""
import csv, json, os, sys, time, itertools, math
import numpy as np
import cv2
from concurrent.futures import ProcessPoolExecutor

CORPUS = "/home/daniel/Documents/IMGS"
WORK = 640          # working long side for features
THUMB = 160         # long side of gray kept for pixel verification
NFEAT = 800

def load_gray(path):
    data = np.fromfile(path, dtype=np.uint8)
    im = cv2.imdecode(data, cv2.IMREAD_COLOR)
    if im is None:
        # PIL fallback for avif/heic/jxl
        from PIL import Image
        import pillow_heif; pillow_heif.register_heif_opener()
        try:
            import pillow_jxl
        except Exception:
            pass
        pil = Image.open(path).convert("RGB")
        im = cv2.cvtColor(np.asarray(pil), cv2.COLOR_RGB2BGR)
    g = im.astype(np.float32).mean(axis=2)          # channel-permutation invariant
    return g

def prep(path):
    g = load_gray(path)
    h, w = g.shape
    s = min(1.0, WORK / max(h, w))
    if s < 1.0:
        gw = cv2.resize(g, (max(1, round(w * s)), max(1, round(h * s))), interpolation=cv2.INTER_AREA)
    else:
        gw = g
    t = THUMB / max(h, w)
    th = cv2.resize(g, (max(1, round(w * t)), max(1, round(h * t))), interpolation=cv2.INTER_AREA)
    return gw, th

_sift = None
def sift():
    global _sift
    if _sift is None:
        _sift = cv2.SIFT_create(nfeatures=NFEAT)
    return _sift

def extract(path):
    gw, th = prep(path)
    g8 = np.clip(gw, 0, 255).astype(np.uint8)
    kps, des = sift().detectAndCompute(g8, None)
    if des is None:
        des = np.zeros((0, 128), np.float32); kps = []
    pts = np.array([[k.pt[0], k.pt[1], k.size / 2.0, math.radians(k.angle)] for k in kps], np.float32).reshape(-1, 4)
    # mirrored version, for the mirror hypothesis
    g8m = g8[:, ::-1].copy()
    kpm, desm = sift().detectAndCompute(g8m, None)
    if desm is None:
        desm = np.zeros((0, 128), np.float32); kpm = []
    ptsm = np.array([[k.pt[0], k.pt[1], k.size / 2.0, math.radians(k.angle)] for k in kpm], np.float32).reshape(-1, 4)
    return dict(path=path, w=g8.shape[1], h=g8.shape[0], pts=pts, des=des, ptsm=ptsm, desm=desm, thumb=th.astype(np.float32))

def match_desc(da, db, ratio=0.8):
    if len(da) < 2 or len(db) < 2:
        return np.zeros((0, 2), np.int32)
    bf = cv2.BFMatcher(cv2.NORM_L2)
    m = bf.knnMatch(da, db, k=2)
    out = [(x[0].queryIdx, x[0].trainIdx) for x in m if len(x) == 2 and x[0].distance < ratio * x[1].distance]
    return np.array(out, np.int32).reshape(-1, 2)

def similarity_from(pa, pb):
    """single-correspondence similarity: b = s R a + t"""
    s = pb[2] / max(pa[2], 1e-6)
    th = pb[3] - pa[3]
    c, sn = math.cos(th) * s, math.sin(th) * s
    tx = pb[0] - (c * pa[0] - sn * pa[1])
    ty = pb[1] - (sn * pa[0] + c * pa[1])
    return np.array([[c, -sn, tx], [sn, c, ty]], np.float64)

def apply(M, P):
    return P @ M[:, :2].T + M[:, 2]

def fit_affine(A, B):
    n = len(A)
    X = np.hstack([A, np.ones((n, 1))])
    M, *_ = np.linalg.lstsq(X, B, rcond=None)
    return M.T  # 2x3

def verify(fa, fb, mirror=False):
    pa_all = fa["pts"]; da = fa["des"]
    if mirror:
        pb_all = fb["ptsm"]; db = fb["desm"]; wb = fb["w"]
    else:
        pb_all = fb["pts"]; db = fb["des"]; wb = fb["w"]
    m = match_desc(da, db)
    res = dict(n_match=len(m), n_in=0, ratio=0.0, ovA=0.0, ovB=0.0, scale=0.0, rot=0.0, ncc=0.0, blk=0.0, blk_n=0)
    if len(m) < 4:
        return res
    A = pa_all[m[:, 0]]; B = pb_all[m[:, 1]]
    diag = math.hypot(fa["w"], fa["h"])
    tol = max(3.0, 0.03 * math.hypot(fb["w"], fb["h"]))
    best, best_in = None, None
    PA = A[:, :2].astype(np.float64); PB = B[:, :2].astype(np.float64)
    # hypotheses from every correspondence (cap 400)
    idx = np.arange(len(m))
    if len(idx) > 400:
        idx = idx[:400]
    bestcount = 0
    for i in idx:
        M = similarity_from(A[i], B[i])
        proj = apply(M, PA)
        d = np.linalg.norm(proj - PB, axis=1)
        inl = d < tol
        c = int(inl.sum())
        if c > bestcount:
            bestcount, best, best_in = c, M, inl
    if bestcount < 3:
        return res
    # refine: LS affine on inliers, recount, twice
    for _ in range(3):
        if best_in.sum() < 3:
            break
        M = fit_affine(PA[best_in], PB[best_in])
        proj = apply(M, PA)
        d = np.linalg.norm(proj - PB, axis=1)
        new_in = d < tol
        if new_in.sum() < best_in.sum():
            break
        best, best_in = M, new_in
    n_in = int(best_in.sum())
    # dedupe inliers by unique positions (SIFT emits multiple orientations per location)
    uniq = len(set(map(tuple, np.round(PA[best_in]).astype(int))))
    res.update(n_in=uniq, ratio=uniq / max(1, len(m)))
    M = best
    sc = math.sqrt(abs(M[0, 0] * M[1, 1] - M[0, 1] * M[1, 0]))
    res["scale"] = sc
    res["rot"] = math.degrees(math.atan2(M[1, 0], M[0, 0]))
    # frame overlap: project A's frame into B, intersect with B's frame
    wa, ha = fa["w"], fa["h"]; hb = fb["h"]
    ovA, ovB = frame_overlap(M, wa, ha, wb, hb)
    res.update(ovA=ovA, ovB=ovB)
    # pixel verification on thumbs
    ncc, blk, blk_n = pixel_check(fa["thumb"], fb["thumb"] if not mirror else fb["thumb"][:, ::-1], M, wa, ha, wb, hb)
    res.update(ncc=ncc, blk=blk, blk_n=blk_n)
    return res

def frame_overlap(M, wa, ha, wb, hb):
    """fraction of A's frame that lands inside B, and fraction of B's frame covered by A (sampled)."""
    n = 40
    xs, ys = np.meshgrid((np.arange(n) + 0.5) / n * wa, (np.arange(n) + 0.5) / n * ha)
    P = np.stack([xs.ravel(), ys.ravel()], 1)
    Q = apply(M, P)
    inside = (Q[:, 0] >= 0) & (Q[:, 0] < wb) & (Q[:, 1] >= 0) & (Q[:, 1] < hb)
    ovA = inside.mean()
    # inverse
    Mi = cv2.invertAffineTransform(M.astype(np.float64))
    xs, ys = np.meshgrid((np.arange(n) + 0.5) / n * wb, (np.arange(n) + 0.5) / n * hb)
    P = np.stack([xs.ravel(), ys.ravel()], 1)
    Q = apply(Mi, P)
    inside = (Q[:, 0] >= 0) & (Q[:, 0] < wa) & (Q[:, 1] >= 0) & (Q[:, 1] < ha)
    ovB = inside.mean()
    return float(ovA), float(ovB)

def pixel_check(ta, tb, M, wa, ha, wb, hb, grid=48, blk=8):
    """Sample the overlap on a grid in A's frame, look up B through M. Both thumbs blurred."""
    sa = ta.shape[1] / wa; sb = tb.shape[1] / wb    # thumb px per working px
    ta_b = cv2.GaussianBlur(ta, (0, 0), 1.0); tb_b = cv2.GaussianBlur(tb, (0, 0), 1.0)
    # region of A's frame that maps inside B (bounding box in A coords)
    n = grid
    xs, ys = np.meshgrid((np.arange(n) + 0.5) / n * wa, (np.arange(n) + 0.5) / n * ha)
    P = np.stack([xs.ravel(), ys.ravel()], 1)
    Q = apply(M, P)
    inside = (Q[:, 0] >= 0) & (Q[:, 0] < wb) & (Q[:, 1] >= 0) & (Q[:, 1] < hb)
    if inside.sum() < 16:
        return 0.0, 0.0, 0
    # restrict to bbox of the inside region, resample a full grid there
    xi = P[inside, 0]; yi = P[inside, 1]
    x0, x1, y0, y1 = xi.min(), xi.max(), yi.min(), yi.max()
    xs, ys = np.meshgrid(np.linspace(x0, x1, n), np.linspace(y0, y1, n))
    P = np.stack([xs.ravel(), ys.ravel()], 1)
    Q = apply(M, P)
    va = cv2.remap(ta_b, (P[:, 0] * sa).astype(np.float32).reshape(n, n), (P[:, 1] * sa).astype(np.float32).reshape(n, n), cv2.INTER_LINEAR)
    vb = cv2.remap(tb_b, (Q[:, 0] * sb).astype(np.float32).reshape(n, n), (Q[:, 1] * sb).astype(np.float32).reshape(n, n), cv2.INTER_LINEAR, borderMode=cv2.BORDER_CONSTANT, borderValue=-1000)
    valid = vb > -500
    if valid.sum() < 16:
        return 0.0, 0.0, 0
    a = va[valid]; b = vb[valid]
    ncc = float(np.corrcoef(a, b)[0, 1]) if a.std() > 1e-3 and b.std() > 1e-3 else 0.0
    # blocks
    agree = 0; total = 0
    for by in range(0, n, blk):
        for bx in range(0, n, blk):
            m = valid[by:by+blk, bx:bx+blk]
            if m.mean() < 0.8:
                continue
            a = va[by:by+blk, bx:bx+blk][m]; b = vb[by:by+blk, bx:bx+blk][m]
            total += 1
            if a.std() < 4 and b.std() < 4:
                agree += 1          # both flat: no evidence against
            elif a.std() < 1e-3 or b.std() < 1e-3:
                pass
            else:
                c = np.corrcoef(a, b)[0, 1]
                if abs(c) > 0.5:
                    agree += 1
    return ncc, (agree / total if total else 0.0), total

def main():
    out = sys.argv[1]
    man = list(csv.DictReader(open(os.path.join(CORPUS, "derived", "manifest.csv"))))
    pairs = list(csv.DictReader(open(os.path.join(CORPUS, "derived", "pairs.csv"))))
    rel = {}
    for p in pairs:
        rel[(p["a"], p["b"])] = p["relation"]; rel[(p["b"], p["a"])] = p["relation"]
    by_seed = {}
    for r in man:
        by_seed.setdefault(r["seed"], []).append(r)
    fam_in = sys.argv[2].split(",")                      # families for in-family pairs
    fam_x = sys.argv[3].split(";") if len(sys.argv) > 3 else []  # groups for cross pairs, ';'-separated groups
    need = set()
    for s in fam_in:
        need.update(r["path"] for r in by_seed[s])
    xgroups = [g.split("+") for g in fam_x] if fam_x else []
    for g in xgroups:
        for s in g:
            need.update(r["path"] for r in by_seed[s])
    need = sorted(need)
    print(f"extracting {len(need)} images", file=sys.stderr)
    t0 = time.time()
    with ProcessPoolExecutor(8) as ex:
        feats = {f["path"]: f for f in ex.map(extract, need, chunksize=8)}
    print(f"extracted in {time.time()-t0:.1f}s", file=sys.stderr)
    jobs = []
    for s in fam_in:
        ps = [r["path"] for r in by_seed[s]]
        for a, b in itertools.combinations(ps, 2):
            jobs.append((a, b, s + "|" + s))
    for g in xgroups:
        for s1, s2 in itertools.combinations(g, 2):
            for a in (r["path"] for r in by_seed[s1]):
                for b in (r["path"] for r in by_seed[s2]):
                    jobs.append((a, b, s1 + "|" + s2))
    print(f"{len(jobs)} pairs to verify", file=sys.stderr)
    var = {r["path"]: r["variant"] for r in man}
    t0 = time.time()
    with open(out, "w", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["a", "b", "fam", "va", "vb", "rel", "mirror", "n_match", "n_in", "ratio", "ovA", "ovB", "scale", "rot", "ncc", "blk", "blk_n"])
        for k, (a, b, fam) in enumerate(jobs):
            r0 = verify(feats[a], feats[b], False)
            r1 = verify(feats[a], feats[b], True)
            r, mir = (r1, 1) if r1["n_in"] > r0["n_in"] else (r0, 0)
            w.writerow([a, b, fam, var[a], var[b], rel.get((a, b), "X"), mir] + [r[c] if not isinstance(r[c], float) else round(r[c], 4) for c in ["n_match", "n_in", "ratio", "ovA", "ovB", "scale", "rot", "ncc", "blk", "blk_n"]])
            if k % 2000 == 0:
                print(f"  {k}/{len(jobs)} {time.time()-t0:.0f}s", file=sys.stderr)

if __name__ == "__main__":
    main()
