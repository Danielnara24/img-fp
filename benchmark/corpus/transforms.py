"""The transformation catalogue for the generated corpus.

One catalogue, applied to every seed. It used to be three — A for the 38 seeds
in the corpus root, B for the 8 in `archive/`, C for a held-out validation set
— which meant a tool's score depended on which bucket a photograph happened to
land in, and meant three files to keep in step.

Each entry is a `Variant`: a name, the function that produces the image, the
output format, and the *region* of the original that survives, as a normalised
(x0, y0, x1, y1) rectangle. The region is what makes the pair-level ground
truth computable rather than guessed: two derived files that both carry the
whole original are SAME, one whose region contains the other's is CROP, and a
pair whose regions do not overlap is DIFFERENT no matter that both descend from
the same photograph.

`geometry` records a flip or rotation separately, so scoring can answer "found
it, but only because the tool is mirror-invariant" rather than folding that
into one recall number.

## Every amount varies with the seed

The older catalogues applied one fixed parameter to every image: `scale_50` was
exactly 0.50 for all 38 of them, `jpeg_q30` exactly quality 30. That turned 38
measurements of a transform into 38 samples of a *single point* on its axis,
and left room for a threshold to settle just past a value it would never be
asked to straddle.

Here a transform with an amount draws it per seed, from a range that straddles
the value a fixed catalogue would have picked — `scale_small` runs from 0.09 to
0.27, `jpeg_low` from quality 7 to 26, `rotate_small` from 0.6 to 9 degrees.
One entry now covers what used to need four, which is why this catalogue is
shorter than the three it replaces while testing more of each axis.

Two properties keep that honest:

  * **So does the side.** Where a transform has a place or a direction, that
    is drawn per seed too (the second generation, IMGS-ALL2): crops anywhere
    in the frame, a strip off any side, a keystone pulling in any edge, a
    rotation, shear or swirl either way, a sticker, watermark or inset in any
    corner, a tile order of its own. Geometric ones go through `_oriented`,
    which runs the transform on a turned or flipped copy and maps the region
    back through the same turn. The first generation (IMGS-ALL) faced one way.
  * **It is still deterministic.** The amount comes from `seed_key_of`, a hash
    of the seed's own 16x16 grey thumbnail, so regenerating the corpus produces
    the same bytes. Nothing here is random at call time.
  * **The ground truth cannot drift from the pixels.** Where an amount changes
    what survives — a crop rectangle, a rotation angle, how much of a host
    canvas the photograph fills — the transform *and* its region or coverage
    come out of one factory over one key (`_crop_jit`, `_rotate_jit`,
    `_scales`). `make_variants.py` records the key, and `region`, `coverage`,
    `width` and `height` record the consequences, so recall can be regressed
    against the strength actually applied.
"""
import io
import math
import os
import subprocess
import tempfile
from dataclasses import dataclass, field
from typing import Callable, Optional, Tuple

import numpy as np
from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageOps

FULL = (0.0, 0.0, 1.0, 1.0)


@dataclass
class Variant:
    name: str
    fn: Callable[[Image.Image], Image.Image]
    fmt: str                          # PIL format name, or "JXL" / "RAW_COPY"
    save_kwargs: object = field(default_factory=dict)   # dict, or key -> dict
    region: Tuple[float, float, float, float] = FULL
    geometry: str = "none"            # none|mirror|rot90|rot180|rot270|rotate|perspective
    note: str = ""
    exif_orientation: Optional[int] = None

    # How much of the DERIVED image is original content. `region` says what
    # survives of the original; this says what the original is now surrounded
    # by. A slide holding a photograph keeps 100% of the photograph at 30%
    # coverage, and that pair is a containment, not an equality — the same
    # relation as a crop, pointing the other way. May be a function of the
    # seed key, for hosts whose canvas varies.
    coverage: object = 1.0

    # Pixels rearranged rather than removed. A shuffled tiling has every
    # pixel of the original and is not the original: any pair whose scramble
    # keys differ is DIFFERENT however much content they share. This is the
    # trap for anything that reduces an image to a global colour summary.
    scramble: str = ""

    # Feed this variant the un-flattened image. Two seeds have an alpha
    # channel, and flattening it away before the transform runs would quietly
    # delete the thing being tested.
    wants_alpha: bool = False

    # Write the file with an external encoder instead of Pillow. Called as
    # writer(image, out_path, key) and owns the output entirely, so a run is
    # not measuring how well a tool has learned one library's JPEG.
    writer: Optional[Callable] = None

# --------------------------------------------------------------------------
# per-seed amounts
# --------------------------------------------------------------------------


def seed_key_of(im):
    """A small deterministic integer that differs between source images.

    Host layouts have to decorate the canvas with *something*, and if every
    seed gets the same decoration then two negatives literally contain the
    same page furniture — a large, identical, highly-structured region. A
    local-feature matcher is then right to say they share content, and the pair
    stops being a clean negative and starts being a mislabelled positive. The
    first version of this catalogue made exactly that mistake: 65 of its false
    positives were two magazine spreads, two browser windows or two contact
    sheets agreeing about the furniture rather than the photographs.

    So the chrome varies with the image it surrounds. Derived from the pixels,
    so it is stable across regeneration and needs nothing passed in.
    """
    if _KEY is not None:
        return _KEY
    thumb = im.convert("L").resize((16, 16), Image.BILINEAR)
    return sum(v * (i + 1) for i, v in enumerate(thumb.getdata())) % 9973


# Set by `_oriented` while it runs a transform on a turned or flipped copy of
# the seed: everything inside draws from the seed's own key, as the manifest
# records it, not from a hash of the turned copy.
_KEY = None


def _jit(key, salt, lo, hi):
    """A value in [lo, hi] chosen by the seed, not by the catalogue.

    Catalogues A and B apply one fixed parameter to every seed: `scale_50` is
    exactly 0.50 for all 38 of them. That makes the 38 measurements of a
    transform 38 samples of a *single point* on its axis, when they could have
    been 38 samples of a range — and it lets a threshold settle just past a
    value it will never be asked to straddle.

    Here the amount varies per seed instead. `salt` keeps two transforms from
    moving together, so a seed that gets a gentle crop does not also get a
    gentle blur. Still deterministic: the key comes from the seed's own pixels,
    so regenerating the corpus produces the same bytes it did before.

    Ranges are chosen to *straddle* the fixed value A and B would have used,
    not to replace it with a different fixed value.
    """
    h = (int(key) * 2_654_435_761 + int(salt) * 40_503 + 0x9E37) & 0xFFFF_FFFF
    h ^= h >> 15
    h = (h * 2_246_822_519) & 0xFFFF_FFFF
    h ^= h >> 13
    return lo + (hi - lo) * (((h >> 8) & 0xFFFF) / 65535.0)


def _jit_int(key, salt, lo, hi):
    """A whole number in [lo, hi], each value equally likely.

    It was `round(_jit(...))`, which gave the two ends half the weight of the
    values between: a three-up contact sheet put the photograph in its middle
    column half the time."""
    return lo + min(hi - lo, int(_jit(key, salt, 0.0, hi - lo + 1)))


def _pick(key, salt, choices):
    """One of `choices`, each equally likely, chosen by the seed."""
    return choices[_jit_int(key, salt, 0, len(choices) - 1)]


def _sign(key, salt):
    return _pick(key, salt, (-1.0, 1.0))


def _jit_kw(salt, field, lo, hi, **fixed):
    """Save keyword arguments whose quality (or similar) varies per seed."""
    def make(key):
        out = dict(fixed)
        out[field] = _jit_int(key, salt, lo, hi)
        return out
    return make


def _amount(salt, lo, hi, make):
    """Apply `make(amount)` with the amount drawn per seed from [lo, hi]."""
    return lambda im: make(_jit(seed_key_of(im), salt, lo, hi))(im)


# --------------------------------------------------------------------------
# which side, drawn per seed
#
# An amount drawn per seed still left every transform facing one way: a strip
# crop was always the top, a keystone always pulled the top edge in, every
# rotation turned the same way, every shear leaned the same way. A matcher (or
# a threshold fitted on one) never saw the other sides. `_oriented` runs a
# transform on a turned or flipped copy of the seed and turns the result back,
# the turn chosen per seed from a group of them: a top strip becomes a strip
# off any side, a clockwise swirl either way round. The region is mapped back
# through the same turn, so the ground truth follows the pixels.
#
# Not for anything that draws text or a symbol: it would come back mirrored.
# Those place themselves (`_corner`).
# --------------------------------------------------------------------------

# name: (PIL transpose, where a normalised point of the seed lands in the copy)
_DIHEDRAL = {
    "id": (None, lambda x, y: (x, y)),
    "fliph": (Image.FLIP_LEFT_RIGHT, lambda x, y: (1 - x, y)),
    "flipv": (Image.FLIP_TOP_BOTTOM, lambda x, y: (x, 1 - y)),
    "rot180": (Image.ROTATE_180, lambda x, y: (1 - x, 1 - y)),
    "transpose": (Image.TRANSPOSE, lambda x, y: (y, x)),
    "transverse": (Image.TRANSVERSE, lambda x, y: (1 - y, 1 - x)),
    "rot90": (Image.ROTATE_90, lambda x, y: (y, 1 - x)),
    "rot270": (Image.ROTATE_270, lambda x, y: (1 - y, x)),
}
_UNDO = {"rot90": "rot270", "rot270": "rot90"}
_SWAPS = {"transpose", "transverse", "rot90", "rot270"}

ALL_SIDES = tuple(_DIHEDRAL)                       # all eight
FLIPS = ("id", "fliph", "flipv", "rot180")         # same shape, any corner
TURNS = ("id", "rot90", "rot180", "rot270")        # any side, no mirror
LEFT_RIGHT = ("id", "fliph")
TOP_BOTTOM = ("id", "flipv")
AXES = ("id", "transpose")                         # along x or along y


def _turn(im, d):
    op = _DIHEDRAL[d][0]
    return im if op is None else im.transpose(op)


def _turn_back_rect(rect, d):
    """A normalised rectangle of the turned copy, in the seed's coordinates."""
    back = _DIHEDRAL[_UNDO.get(d, d)][1]
    x0, y0, x1, y1 = rect
    xs, ys = zip(*(back(x, y) for x, y in ((x0, y0), (x1, y1))))
    return (min(xs), min(ys), max(xs), max(ys))


def _oriented(salt, choices, go, region=None):
    """`go` applied to the seed turned by a per-seed member of `choices`, and
    turned back. Returns the transform and its region, as the factories do."""
    def which(key):
        return _pick(key, salt, choices)

    def fn(im):
        global _KEY
        key = seed_key_of(im)
        d = which(key)
        outer, _KEY = _KEY, key
        try:
            out = go(_turn(im, d))
        finally:
            _KEY = outer
        return _turn(out, _UNDO.get(d, d))

    def where(w, h, key):
        d = which(key)
        if region is None:
            return FULL
        tw, th = (h, w) if d in _SWAPS else (w, h)
        r = region(tw, th, key) if callable(region) else region
        return _turn_back_rect(r, d)

    return fn, where


def _compose(outer, inner):
    """`inner`, a region of an image that was itself `outer` of the seed, in
    the seed's coordinates."""
    ox0, oy0, ox1, oy1 = outer
    ix0, iy0, ix1, iy1 = inner
    w, h = ox1 - ox0, oy1 - oy0
    return (ox0 + ix0 * w, oy0 + iy0 * h, ox0 + ix1 * w, oy0 + iy1 * h)


def _chain_regions(*steps):
    """Chain transforms that each come with a region; the regions compose."""
    def go(im):
        for fn, _ in steps:
            im = fn(im)
        return im

    go = _keyed(go)

    def region(w, h, key):
        r = FULL
        for _, reg in steps:
            if reg is None:
                continue
            here = reg(w, h, key) if callable(reg) else reg
            r = _compose(r, here)
            # the next step sees the cropped image; its region is normalised,
            # so only an aspect-dependent region needs the new size
            w, h = max(1, w * (here[2] - here[0])), max(1, h * (here[3] - here[1]))
        return r

    return go, region



# --------------------------------------------------------------------------
# external encoders
# --------------------------------------------------------------------------


def _via_tool(argv, quality=None):
    """Run `argv` with {src}, {dst} and {q} substituted, from a temporary PNG.

    `quality` is `(salt, lo, hi)` when the encoder's quality should vary with
    the seed; `{q}` in `argv` is then that value.
    """
    def write(image, out_path, key=0):
        q = _jit_int(key, *quality) if quality else 0
        fd, tmp_png = tempfile.mkstemp(suffix=".png")
        os.close(fd)
        try:
            image.convert("RGB").save(tmp_png, "PNG")
            cmd = [a.format(src=tmp_png, dst=out_path, q=q) for a in argv]
            proc = subprocess.run(cmd, capture_output=True, text=True)
            if proc.returncode != 0 or not os.path.exists(out_path):
                raise RuntimeError(f"{cmd[0]}: {proc.stderr.strip()[:200]}")
        finally:
            os.unlink(tmp_png)
    return write


def _magick(*args, quality=None):
    return _via_tool(["convert", "{src}", *args, "{dst}"], quality)


def _ffmpeg(*args, quality=None):
    return _via_tool(["ffmpeg", "-y", "-loglevel", "error", "-i", "{src}",
                      *args, "{dst}"], quality)



# --------------------------------------------------------------------------
# resampling and cropping
# --------------------------------------------------------------------------


def _crop_box(im, x0, y0, x1, y1):
    w, h = im.size
    return im.crop((int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h)))


def _centre_aspect(ratio):
    """Centre crop to a width:height ratio, keeping as much as fits."""
    def region(w, h):
        if w / h > ratio:                      # too wide, trim sides
            nw = h * ratio
            x0 = (w - nw) / 2 / w
            return (x0, 0.0, 1.0 - x0, 1.0)
        nh = w / ratio                         # too tall, trim top/bottom
        y0 = (h - nh) / 2 / h
        return (0.0, y0, 1.0, 1.0 - y0)

    def go(im):
        w, h = im.size
        x0, y0, x1, y1 = region(w, h)
        return _crop_box(im, x0, y0, x1, y1)
    return go, region


# Largest output a resize may produce. `upscale` reaches 3x, and 3x of a
# 13-megapixel seed is a hundred-megapixel frame that costs 800 MB to build --
# the old catalogue's fixed 1.5x never got near it. Capping the *output* is the
# right end here: capping the input would be the opposite of what upscaling is.
_RESIZE_PIXELS = 32_000_000


def _resize_jit(salt, lo, hi, resample):
    def go(im):
        f = _jit(seed_key_of(im), salt, lo, hi)
        w, h = im.size
        if w * h * f * f > _RESIZE_PIXELS:
            f = (_RESIZE_PIXELS / float(w * h)) ** 0.5
        return im.convert("RGB").resize(
            (max(1, round(w * f)), max(1, round(h * f))), resample)
    return go


def _long_edge_jit(salt, lo, hi, resample=Image.BICUBIC):
    def go(im):
        t = _jit_int(seed_key_of(im), salt, lo, hi)
        w, h = im.size
        f = t / max(w, h)
        return im.convert("RGB").resize(
            (max(1, round(w * f)), max(1, round(h * f))), resample)
    return go


def _stretch_jit(salt, lo, hi, axis="y"):
    def go(im):
        f = _jit(seed_key_of(im), salt, lo, hi)
        w, h = im.size
        nw, nh = (w, round(h * f)) if axis == "y" else (round(w * f), h)
        return im.convert("RGB").resize((max(1, nw), max(1, nh)), Image.BICUBIC)
    return go



# --------------------------------------------------------------------------
# geometry
# --------------------------------------------------------------------------


def _perspective_coeffs(src, dst):
    import numpy as np
    matrix = []
    for (sx, sy), (dx, dy) in zip(src, dst):
        matrix.append([dx, dy, 1, 0, 0, 0, -sx * dx, -sx * dy])
        matrix.append([0, 0, 0, dx, dy, 1, -sy * dx, -sy * dy])
    A = np.array(matrix, dtype=float)
    B = np.array(src, dtype=float).reshape(8)
    return np.linalg.solve(A, B).tolist()


def _rotate_inscribed(deg):
    """Rotate and crop to the largest axis-aligned rect fully inside the result.

    A photographed or scanned print is rotated a degree or two and then cropped
    back to a rectangle; this is that, and the shrunken region is why such a
    file is a CROP of its original rather than a SAME.
    """
    import math

    def inscribed(w, h, angle):
        a = math.radians(abs(angle))
        cos, sin = math.cos(a), math.sin(a)
        if w <= 0 or h <= 0:
            return 0, 0
        long_side, short_side = (w, h) if w >= h else (h, w)
        if short_side <= 2 * sin * cos * long_side or abs(sin - cos) < 1e-10:
            x = 0.5 * short_side
            wr, hr = (x / sin, x / cos) if w >= h else (x / cos, x / sin)
        else:
            c = cos * cos - sin * sin
            wr = (w * cos - h * sin) / c
            hr = (h * cos - w * sin) / c
        return wr, hr

    def go(im):
        w, h = im.size
        rotated = im.rotate(deg, resample=Image.BICUBIC, expand=False)
        wr, hr = inscribed(w, h, deg)
        x0, y0 = (w - wr) / 2, (h - hr) / 2
        return rotated.crop((int(x0), int(y0), int(x0 + wr), int(y0 + hr)))

    def region_for(w, h):
        wr, hr = inscribed(w, h, deg)
        x0, y0 = (w - wr) / 2 / w, (h - hr) / 2 / h
        return (x0, y0, 1 - x0, 1 - y0)

    return go, region_for


def _perspective(strength):
    """Keystone, as if the image were photographed off-axis.

    The warp leaves black wedges where the output samples outside the source.
    A real photo of a screen has no such wedges — you frame the screen — so
    they are cropped off, which is why these variants are a CROP of their
    original rather than a SAME: content at the edges really is gone.
    """
    def go(im):
        w, h = im.size
        dx = int(w * strength)
        # pull the top edge in, as if photographing a screen from below
        coeffs = _perspective_coeffs(
            [(0, 0), (w, 0), (w, h), (0, h)],
            [(dx, 0), (w - dx, 0), (w, h), (0, h)])
        out = im.transform((w, h), Image.PERSPECTIVE, coeffs, Image.BICUBIC)
        # [dx, 0, w-dx, h] is the largest axis-aligned rect inside the trapezoid
        return out.crop((dx, 0, w - dx, h))
    return go


def _radial_distort(k, keep=0.88):
    """Barrel (k<0) or pincushion (k>0), then crop away the empty rim.

    Not an affine transform, so nothing that normalises by resampling to a
    fixed grid undoes it. A phone camera's uncorrected wide lens does this.
    """
    def go(im):
        import numpy as np
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS))
        h, w = src.shape[:2]
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        cx, cy = (w - 1) / 2.0, (h - 1) / 2.0
        nx, ny = (xx - cx) / cx, (yy - cy) / cy
        r2 = nx * nx + ny * ny
        f = 1.0 + k * r2
        sx = np.clip(nx * f * cx + cx, 0, w - 1).astype(np.int32)
        sy = np.clip(ny * f * cy + cy, 0, h - 1).astype(np.int32)
        out = Image.fromarray(src[sy, sx], "RGB")
        m = (1.0 - keep) / 2
        return _crop_box(out, m, m, 1 - m, 1 - m)
    return go


def _wave(amp_frac=0.025, periods=3.0):
    """Sinusoidal horizontal displacement, then trim the ragged edges."""
    def go(im):
        import numpy as np
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS))
        h, w = src.shape[:2]
        amp = max(1, int(w * amp_frac))
        yy, xx = np.mgrid[0:h, 0:w]
        shift = (amp * np.sin(2 * np.pi * periods * yy / h)).astype(np.int32)
        sx = np.clip(xx + shift, 0, w - 1)
        out = Image.fromarray(src[yy, sx], "RGB")
        m = amp_frac
        return _crop_box(out, m, 0.0, 1 - m, 1.0)
    return go


def _shear(factor=0.12):
    """Affine shear, cropped back to a rectangle of real content."""
    def go(im):
        im = im.convert("RGB")
        w, h = im.size
        out = im.transform((w, h), Image.AFFINE, (1, factor, -factor * h / 2, 0, 1, 0),
                           Image.BICUBIC)
        m = factor / 2
        return _crop_box(out, m, 0.0, 1 - m, 1.0)
    return go


def _keystone_side(strength):
    """Keystone about the vertical axis: the left edge pulled in.

    A's `perspective` pulls the top edge in; this is the same idea rotated 90
    degrees, so the surviving rectangle loses top and bottom rather than left
    and right.
    """
    def go(im):
        im = im.convert("RGB")
        w, h = im.size
        dy = int(h * strength)
        coeffs = _perspective_coeffs(
            [(0, 0), (w, 0), (w, h), (0, h)],
            [(0, dy), (w, 0), (w, h), (0, h - dy)])
        out = im.transform((w, h), Image.PERSPECTIVE, coeffs, Image.BICUBIC)
        return out.crop((0, dy, w, h - dy))
    return go


def _remap(fn, margin):
    """Resample through a coordinate map, then trim `margin` off every side.

    `fn(nx, ny)` takes normalised coordinates in [-1, 1] and returns where to
    read from, in the same space. Nearest-neighbour on purpose: it keeps the
    warp from also being a blur, so the warp is what is being tested.
    """
    def go(im):
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS))
        h, w = src.shape[:2]
        cx, cy = (w - 1) / 2.0, (h - 1) / 2.0
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        nx, ny = (xx - cx) / cx, (yy - cy) / cy
        sx, sy = fn(nx, ny)
        px = np.clip(sx * cx + cx, 0, w - 1).astype(np.int32)
        py = np.clip(sy * cy + cy, 0, h - 1).astype(np.int32)
        out = Image.fromarray(src[py, px], "RGB")
        if margin <= 0:
            return out
        return _crop_box(out, margin, margin, 1 - margin, 1 - margin)
    return go


def _swirl(degrees, margin=0.04):
    """Rotate by an angle that falls off with radius. Not affine anywhere."""
    def fn(nx, ny):
        r = np.sqrt(nx * nx + ny * ny)
        t = math.radians(degrees) * np.clip(1.0 - r, 0.0, 1.0) ** 2
        cos, sin = np.cos(t), np.sin(t)
        return nx * cos - ny * sin, nx * sin + ny * cos
    return _remap(fn, margin)


def _pinch(amount, margin=0.06):
    """Radial power warp: the centre is magnified, the rim compressed."""
    def fn(nx, ny):
        r = np.sqrt(nx * nx + ny * ny)
        safe = np.maximum(r, 1e-6)
        scale = np.where(r < 1.0, safe ** (1.0 + amount) / safe, 1.0)
        return nx * scale, ny * scale
    return _remap(fn, margin)


def _fisheye(k, margin=0.07):
    """A lens curve with a different sign and strength from B's two."""
    def fn(nx, ny):
        r2 = nx * nx + ny * ny
        f = 1.0 + k * r2 + 0.5 * k * r2 * r2
        return nx * f, ny * f
    return _remap(fn, margin)


def _wave_vertical(amp_frac=0.035, periods=2.5):
    """Sinusoidal *vertical* displacement; B's wave is horizontal."""
    def go(im):
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS))
        h, w = src.shape[:2]
        amp = max(1, int(h * amp_frac))
        yy, xx = np.mgrid[0:h, 0:w]
        shift = (amp * np.sin(2 * np.pi * periods * xx / w)).astype(np.int32)
        sy = np.clip(yy + shift, 0, h - 1)
        out = Image.fromarray(src[sy, xx], "RGB")
        return _crop_box(out, 0.0, amp_frac, 1.0, 1 - amp_frac)
    return go



# --------------------------------------------------------------------------
# geometry, drawn per seed
# --------------------------------------------------------------------------


def _crop_jit(salt, x0, y0, x1, y1, spread=0.06):
    """A crop whose four edges each wander by up to `spread`, per seed."""
    def rect(key):
        a = min(max(x0 + _jit(key, salt + 0, -spread, spread), 0.0), 0.96)
        b = min(max(y0 + _jit(key, salt + 1, -spread, spread), 0.0), 0.96)
        c = min(max(x1 + _jit(key, salt + 2, -spread, spread), a + 0.04), 1.0)
        d = min(max(y1 + _jit(key, salt + 3, -spread, spread), b + 0.04), 1.0)
        return (a, b, c, d)

    def go(im):
        return _crop_box(im.convert("RGB"), *rect(seed_key_of(im)))

    return go, (lambda w, h, key: rect(key))


def _crop_free(salt, w_lo, w_hi, h_lo=None, h_hi=None):
    """A crop of per-seed width and height (as fractions of the seed's), put
    anywhere it fits: the place is drawn too, not only the size."""
    h_lo = w_lo if h_lo is None else h_lo
    h_hi = w_hi if h_hi is None else h_hi

    def rect(key):
        fw = _jit(key, salt, w_lo, w_hi)
        fh = _jit(key, salt + 1, h_lo, h_hi)
        x0 = _jit(key, salt + 2, 0.0, 1.0 - fw)
        y0 = _jit(key, salt + 3, 0.0, 1.0 - fh)
        return (x0, y0, x0 + fw, y0 + fh)

    def go(im):
        return _crop_box(im.convert("RGB"), *rect(seed_key_of(im)))

    return go, (lambda w, h, key: rect(key))


def _crop_edges(salt, lo, hi):
    """Each of the four edges trimmed by its own per-seed amount in [lo, hi]."""
    def rect(key):
        return (_jit(key, salt, lo, hi), _jit(key, salt + 1, lo, hi),
                1.0 - _jit(key, salt + 2, lo, hi), 1.0 - _jit(key, salt + 3, lo, hi))

    def go(im):
        return _crop_box(im.convert("RGB"), *rect(seed_key_of(im)))

    return go, (lambda w, h, key: rect(key))


def _strip_jit(salt, lo, hi):
    """A strip the full width of the seed and a per-seed share of its height,
    off the top; `_oriented` puts it off any side."""
    def rect(key):
        return (0.0, 0.0, 1.0, _jit(key, salt, lo, hi))

    def go(im):
        return _crop_box(im.convert("RGB"), *rect(seed_key_of(im)))

    return go, (lambda w, h, key: rect(key))


def _aspect_jit(salt, lo, hi, either_way=False):
    """Crop to an aspect ratio drawn per seed from [lo, hi] (or, `either_way`,
    its reciprocal half the time), keeping as much as fits, at a per-seed
    place along the side it trims: not always the middle."""
    def ratio(key):
        r = _jit(key, salt, lo, hi)
        return 1.0 / r if either_way and _pick(key, salt + 2000, (False, True)) else r

    def region(w, h, key):
        r, t = ratio(key), _jit(key, salt + 1000, 0.0, 1.0)
        if w / h > r:                          # too wide, trim the sides
            fw = h * r / w
            x0 = t * (1.0 - fw)
            return (x0, 0.0, x0 + fw, 1.0)
        fh = w / r / h                         # too tall, trim top and bottom
        y0 = t * (1.0 - fh)
        return (0.0, y0, 1.0, y0 + fh)

    def go(im):
        im = im.convert("RGB")
        return _crop_box(im, *region(im.size[0], im.size[1], seed_key_of(im)))

    return go, region


def _rotate_jit(salt, lo, hi):
    """Rotate by an angle drawn per seed, either way round, cropped to the
    inscribed rectangle (which is centred, so the region is the same both ways)."""
    def angle(key):
        return _sign(key, salt + 1000) * _jit(key, salt, lo, hi)

    def region(w, h, key):
        return _rotate_inscribed(angle(key))[1](w, h)

    def go(im):
        return _rotate_inscribed(angle(seed_key_of(im)))[0](im)

    return go, region


def _keystone_jit(salt, lo, hi):
    """Keystone about the vertical axis, by a per-seed amount."""
    def region(w, h, key):
        s = _jit(key, salt, lo, hi)
        return (0.0, s, 1.0, 1.0 - s)

    def go(im):
        return _keystone_side(_jit(seed_key_of(im), salt, lo, hi))(im)

    return go, region


def _wave_jit(salt, amp_lo, amp_hi, per_lo, per_hi):
    """Vertical sinusoid: amplitude and period both drawn per seed."""
    def region(w, h, key):
        a = _jit(key, salt, amp_lo, amp_hi)
        return (0.0, a, 1.0, 1.0 - a)

    def go(im):
        key = seed_key_of(im)
        return _wave_vertical(_jit(key, salt, amp_lo, amp_hi),
                              _jit(key, salt + 1, per_lo, per_hi))(im)

    return go, region


def _warp_jit(make, salt, lo, hi, margin):
    """A radial warp whose strength varies per seed; the trimmed rim does not,
    so the surviving region stays exactly `margin` in from every edge."""
    def go(im):
        return make(_jit(seed_key_of(im), salt, lo, hi), margin)(im)
    return go, (margin, margin, 1 - margin, 1 - margin)



# --------------------------------------------------------------------------
# hosts: the image inside something larger
# --------------------------------------------------------------------------


# A host multiplies the photograph's area — `embed_tiny` by twenty-five, a
# four-up contact sheet by sixteen. On the largest seed that is a third of a
# gigapixel, which this laptop does not have the memory to build. Capping the
# photograph's working size first bounds every host at once, and costs the
# ground truth nothing: coverage is an area *ratio*, so scaling the image
# before a host sizes its canvas from it leaves coverage exactly where it was,
# and the whole image still survives, so the region stays FULL.
_HOST_PIXELS = 2_200_000

# The numpy transforms — the warps, the grain, the blurs — hold several
# float32 copies of the frame at once, and `_motion_blur` holds one per step.
# On a 13-megapixel seed that is half a gigabyte for a single variant. They
# used to run only on the eight small `archive/` seeds, so it never showed;
# one catalogue over every seed is what exposed it. Capping the frame bounds
# them all, and normalised regions are unaffected by a uniform scale, so the
# ground truth does not notice.
_PIXEL_PIXELS = 4_000_000


def _fit(im, budget=_HOST_PIXELS):
    """Downscale an image destined for a host, if it is large enough to matter.

    Always take `seed_key_of` from the image as it arrived, *before* this:
    hashing a 16x16 thumbnail of a downscaled copy does not give the same
    answer, and a host that drew its amount from one key while the manifest
    recorded another would put the ground truth back out of step with the
    pixels. `contact_sheet` did exactly that for one revision, declaring a
    ninth of the page while building a sixteenth.
    """
    w, h = im.size
    if w * h <= budget:
        return im
    f = (budget / float(w * h)) ** 0.5
    return im.resize((max(1, round(w * f)), max(1, round(h * f))), Image.LANCZOS)


def _canvas(sx, sy, bg):
    def make(w, h):
        return Image.new("RGB", (max(w + 2, int(w * sx)), max(h + 2, int(h * sy))), bg)
    return make


def _scales(salt, sx_lo, sx_hi, sy_lo, sy_hi):
    """Canvas size per seed, and the coverage that follows from it.

    Returned together so the declared coverage is always exactly the one the
    canvas produced — how much smaller the photograph sits inside its host is
    one of the amounts worth varying, and the ground truth has to track it.
    """
    def of(key):
        return _jit(key, salt, sx_lo, sx_hi), _jit(key, salt + 1, sy_lo, sy_hi)

    def coverage(key):
        sx, sy = of(key)
        return 1.0 / (sx * sy)

    return of, coverage


_MAGAZINE, _MAGAZINE_COV = _scales(301, 2.1, 2.8, 1.20, 1.55)


_BROWSER, _BROWSER_COV = _scales(311, 1.04, 1.12, 1.18, 1.42)


_POLAROID, _POLAROID_COV = _scales(321, 1.14, 1.34, 1.30, 1.64)


_PIP, _PIP_COV = _scales(331, 2.2, 3.2, 2.2, 3.2)


_EMAIL, _EMAIL_COV = _scales(341, 1.34, 1.72, 1.66, 2.24)


_POSTER, _POSTER_COV = _scales(351, 1.70, 2.20, 1.70, 2.20)


def _sheet_grid(key):
    """Three-up or four-up proof sheet, per seed."""
    return _jit_int(key, 361, 3, 4)


def _sheet_cov(key):
    g = _sheet_grid(key)
    return 1.0 / (g * g)


_HEADLINES = ["FIELD NOTES", "THE LONG VIEW", "DISPATCHES", "AFTERIMAGE",
              "SECOND LOOK", "IN PASSING", "THE RECORD", "OFF SEASON"]


def _magazine_spread(im):
    """Two facing pages; the photograph fills most of the right-hand one."""
    k = seed_key_of(im)
    im = _fit(im.convert("RGB"))
    w, h = im.size
    out = _canvas(*_MAGAZINE(k), bg=(250, 248, 243))(w, h)
    cw, ch = out.size
    d = ImageDraw.Draw(out)
    d.rectangle([cw // 2 - 1, 0, cw // 2 + 1, ch], fill=(226, 222, 214))
    lead = 20 + k % 13
    for i in range(20):                       # body text on the left page
        y = int(ch * (0.10 + (k % 7) * 0.006)) + i * max(6, ch // lead)
        if y > ch - ch // 9:
            break
        ragged = (i * (3 + k % 5) + k) % 5
        d.rectangle([cw // 14, y, cw // 2 - cw // 9 - ragged * cw // 26,
                     y + max(2, ch // 240)], fill=(70, 68, 66))
    out.paste(im, (cw // 2 + (cw // 2 - w) // 2, (ch - h) // 2))
    d.text((cw // 14, int(ch * 0.05)), _HEADLINES[k % len(_HEADLINES)],
           fill=(110 + k % 90, 40, 30))
    return out


def _contact_sheet(im):
    """One cell of a proof sheet; every other cell is not it.

    Three-up or four-up depending on the seed, so the photograph is a ninth of
    the page for some and a sixteenth for others.
    """
    key = seed_key_of(im)
    im = _fit(im.convert("RGB"))
    w, h = im.size
    g = _sheet_grid(key)
    out = Image.new("RGB", (w * g, h * g), (24, 24, 26))
    spot = (_jit_int(key, 362, 0, g - 1), _jit_int(key, 363, 0, g - 1))
    out.paste(im, (spot[0] * w, spot[1] * h))
    for k, (cx, cy) in enumerate((x, y) for y in range(g) for x in range(g)
                                 if (x, y) != spot):
        cell = Image.new("RGB", (w, h))
        dd = ImageDraw.Draw(cell)
        j = k + key
        for i in range(0, w, max(3, w // (18 + key % 24))):
            v = (i * (23 + 41 * j) + 7 * key) % 256
            dd.line([(i, 0), (i + w // (2 + j % 4), h)],
                    fill=((v * 2) % 256, v, (200 - v + key) % 256),
                    width=2 + j % 5)
        out.paste(cell, (cx * w, cy * h))
    return out


_URLS = ["example.org/gallery", "notes.example.net/archive/7",
         "static.example.com/img?id=41", "example.org/2019/summer",
         "photos.example.io/album/kestrel", "example.org/wiki/Index",
         "cdn.example.net/assets/full", "example.com/search?q=river"]


def _browser_window(im):
    """A desktop browser: tab strip, address bar, and a scrollbar gutter."""
    k = seed_key_of(im)
    im = _fit(im.convert("RGB"))
    w, h = im.size
    out = _canvas(*_BROWSER(k), bg=(252, 252, 253))(w, h)
    cw, ch = out.size
    chrome = ch - h
    d = ImageDraw.Draw(out)
    d.rectangle([0, 0, cw, chrome // 2], fill=(222, 225, 232))
    d.rounded_rectangle([cw // 40, chrome // 10, cw // (2 + k % 3), chrome // 2 - chrome // 12],
                        radius=max(2, chrome // 24), fill=(252, 252, 253))
    d.rounded_rectangle([cw // 40, chrome * 6 // 10, cw - cw // (6 + k % 6), chrome - chrome // 12],
                        radius=max(2, chrome // 22), fill=(238, 240, 244))
    d.text((cw // 20, chrome * 7 // 10), _URLS[k % len(_URLS)], fill=(90, 94, 104))
    out.paste(im, (0, chrome))
    d.rectangle([cw - cw // 90, chrome, cw, ch], fill=(232, 234, 238))
    return out


def _polaroid(im):
    """An instant print: thin border, deep chin, faintly warm paper."""
    im = im.convert("RGB")
    w, h = im.size
    out = _canvas(*_POLAROID(seed_key_of(im)), bg=(247, 244, 236))(w, h)
    cw, ch = out.size
    out.paste(im, ((cw - w) // 2, (cw - w) // 2))
    d = ImageDraw.Draw(out)
    d.text((cw // 8, ch - (ch - h - (cw - w) // 2) // 2), "aug '09",
           fill=(60, 58, 54))
    return out


def _picture_in_picture(im):
    """A small inset over an unrelated synthetic background."""
    _key = seed_key_of(im)
    im = _fit(im.convert("RGB"))
    w, h = im.size
    _psx, _psy = _PIP(_key)
    out = Image.new("RGB", (int(w * _psx), int(h * _psy)), (12, 14, 20))
    cw, ch = out.size
    d = ImageDraw.Draw(out)
    k = _key
    for i in range(0, ch, max(5, ch // (30 + k % 40))):   # synthetic gradient
        v = int(200 * i / ch + k) % 256
        d.line([(0, i), (cw, i)], fill=(v // 3, v // 2, v), width=3 + k % 7)
    d.ellipse([cw // (4 + k % 5), ch // 5, cw // 2, ch // (2 + k % 3)],
              fill=(38, (44 + k) % 200, 58))
    left, top = _pick(k, 430, ((False, False), (True, False), (False, True), (True, True)))
    out.paste(im, (cw // 24 if left else cw - w - cw // 24,
                   ch // 24 if top else ch - h - ch // 24))
    return out


def _email_quote(im):
    """An inline image in a quoted reply, rule down the left."""
    im = im.convert("RGB")
    w, h = im.size
    out = _canvas(*_EMAIL(seed_key_of(im)), bg=(255, 255, 255))(w, h)
    cw, ch = out.size
    d = ImageDraw.Draw(out)
    k = seed_key_of(im)
    d.text((cw // 20, ch // 40),
           "On %s, %s wrote:" % (["Tue", "Wed", "Fri", "Mon", "Thu"][k % 5],
                                 ["someone", "a colleague", "the editor",
                                  "R.", "the archive"][k % 5]),
           fill=(80, 84, 92))
    top = int(ch * 0.12)
    d.rectangle([cw // 24, top, cw // 24 + max(2, cw // 200), ch - ch // 12],
                fill=(190, 194, 204))
    out.paste(im, (cw // 12, top + ch // 40))
    for i in range(8):
        y = top + ch // 40 + h + ch // 30 + i * max(6, ch // (28 + k % 12))
        if y > ch - ch // 16:
            break
        d.rectangle([cw // 12, y, cw - cw // 6 - ((i + k) % 4) * cw // 12,
                     y + max(2, ch // 260)], fill=(110, 114, 122))
    return out


def _wall_poster(im):
    """The image printed and photographed on a wall, at an angle."""
    _key = seed_key_of(im)
    im = _fit(im.convert("RGB"))
    w, h = im.size
    _wsx, _wsy = _POSTER(_key)
    cw, ch = int(w * _wsx), int(h * _wsy)
    wall = Image.new("RGB", (cw, ch), (176, 172, 164))
    d = ImageDraw.Draw(wall)
    k = _key
    for i in range(0, ch, max(9, ch // (18 + k % 24))):   # faint wall texture
        d.line([(0, i), (cw, i + ch // (25 + k % 30))],
               fill=(168, (164 + k) % 210, 157), width=2 + k % 4)
    # each corner of the print wanders about the wall, so the angle it was
    # photographed at differs from seed to seed
    nominal = [(.18, .16), (.82, .16), (.82, .84), (.18, .84)]
    quad = [(int(cw * (x + _jit(k, 440 + 2 * i, -.07, .07))),
             int(ch * (y + _jit(k, 441 + 2 * i, -.07, .07))))
            for i, (x, y) in enumerate(nominal)]
    coeffs = _perspective_coeffs([(0, 0), (w, 0), (w, h), (0, h)], quad)
    warped = im.transform((cw, ch), Image.PERSPECTIVE, coeffs, Image.BICUBIC)
    mask = Image.new("L", (cw, ch), 0)
    ImageDraw.Draw(mask).polygon(quad, fill=255)
    wall.paste(warped, (0, 0), mask)
    return wall


def _embed(frac, bg=(255, 255, 255), where="center"):
    """Paste the original onto a larger canvas of flat colour."""
    def go(im):
        im = _fit(im.convert("RGB"))
        w, h = im.size
        cw, ch = max(w + 2, int(w / frac)), max(h + 2, int(h / frac))
        out = Image.new("RGB", (cw, ch), bg)
        if isinstance(where, tuple):           # fractions of the free room
            pos = (int((cw - w) * where[0]), int((ch - h) * where[1]))
        elif where == "center":
            pos = ((cw - w) // 2, (ch - h) // 2)
        elif where == "topleft":
            pos = (cw // 24, ch // 24)
        else:
            pos = (cw - w - cw // 24, ch - h - ch // 24)
        out.paste(im, pos)
        return out
    return go


def _blur_fill(ratio=9 / 16):
    """Vertical-video framing: the image sharp over a blurred, zoomed copy."""
    def go(im):
        im = _fit(im.convert("RGB"))
        w, h = im.size
        ch = max(h, int(w / ratio))
        cw = int(ch * ratio)
        if cw < w:
            cw, ch = w, int(w / ratio)
        bg = im.resize((cw, int(cw * h / w)), Image.LANCZOS)
        bg = bg.filter(ImageFilter.GaussianBlur(max(4, cw // 40)))
        out = Image.new("RGB", (cw, ch), (0, 0, 0))
        out.paste(bg, (0, (ch - bg.size[1]) // 2))
        fw = int(cw * 0.92)
        fh = max(1, int(h * fw / w))
        out.paste(im.resize((fw, fh), Image.LANCZOS), ((cw - fw) // 2, (ch - fh) // 2))
        return out
    return go



# --------------------------------------------------------------------------
# occlusion
# --------------------------------------------------------------------------


def _watermark(im):
    k = seed_key_of(im)
    out = im.convert("RGB").copy()
    w, h = out.size
    layer = Image.new("RGBA", out.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    size = max(12, w // 22)
    text = "(c) %s" % ["example", "studio", "archive", "press", "atlas"][k % 5]
    ox, oy = _jit(k, 401, 0.35, 0.60), _jit(k, 402, 0.10, 0.22)
    inset = _jit(k, 403, 0.01, 0.05)
    # the box as it sits in the bottom-right corner, then put in the seed's corner
    bx0, by0 = w - int(w * (ox + inset)), h - int(h * (oy + inset))
    bx1, by1 = w - int(w * inset), h - int(h * inset)
    left, top = _pick(k, 404, ((False, False), (True, False), (False, True), (True, True)))
    if left:
        bx0, bx1 = w - bx1, w - bx0
    if top:
        by0, by1 = h - by1, h - by0
    d.rectangle([bx0, by0, bx1, by1], fill=(0, 0, 0, _jit_int(k, 864, 70, 150)),
                outline=(255, 255, 255, 90), width=max(1, size // 12))
    d.text((bx0 + int(w * inset), by0 + int(h * inset)), text,
           fill=(255, 255, 255, _jit_int(k, 405, 170, 240)))
    return Image.alpha_composite(out.convert("RGBA"), layer).convert("RGB")


_SLIDE_TITLES = ["Quarterly review", "Field notes", "Appendix B", "Results",
                 "Site survey", "Draft for comment", "Summary", "Handover"]

_CAPTIONS = ["when the benchmark finally runs", "nobody told him yet",
             "the ferry leaves at six", "it was colder than it looked",
             "she never answered that", "third time this week",
             "and then it stopped raining", "worth every minute"]


def _CAPTION_SHARE(k):
    return _jit(k, 420, 0.18, 0.42)


def _caption_bar(im):
    """Meme framing: the whole image, with a white caption strip under it."""
    k = seed_key_of(im)
    w, h = im.size
    bar = max(28, int(h * _CAPTION_SHARE(k)))
    out = Image.new("RGB", (w, h + bar), "white")
    over = _pick(k, 421, (False, True))
    out.paste(im.convert("RGB"), (0, bar if over else 0))
    d = ImageDraw.Draw(out)
    d.text((int(w * 0.04), (0 if over else h) + bar // 3),
           _CAPTIONS[k % len(_CAPTIONS)], fill="black")
    return out


def _letterbox(ratio):
    """Pad to a wider aspect with black bars. All content survives."""
    def go(im):
        w, h = im.size
        tw = max(w, int(h * ratio))
        th = max(h, int(w / ratio))
        if tw / th > ratio:
            th = int(tw / ratio)
        else:
            tw = int(th * ratio)
        out = Image.new("RGB", (tw, th), "black")
        out.paste(im.convert("RGB"), ((tw - w) // 2, (th - h) // 2))
        return out
    return go


def _screenshot_chrome(im):
    """The image inside a window frame, as a screenshot would capture it."""
    w, h = im.size
    pad, title = max(8, w // 90), max(22, h // 22)
    out = Image.new("RGB", (w + 2 * pad, h + pad + title), (238, 238, 240))
    d = ImageDraw.Draw(out)
    d.rectangle([0, 0, out.width, title], fill=(222, 222, 226))
    for i, colour in enumerate([(255, 95, 86), (255, 189, 46), (39, 201, 63)]):
        cx = pad + i * (title // 2)
        d.ellipse([cx, title // 3, cx + title // 4, title // 3 + title // 4], fill=colour)
    out.paste(im.convert("RGB"), (pad, title))
    return out


def _boxes(rects, fill=(0, 0, 0)):
    """Opaque rectangles in normalised coordinates: redaction, stickers."""
    def go(im):
        out = im.convert("RGB").copy()
        w, h = out.size
        d = ImageDraw.Draw(out)
        for x0, y0, x1, y1 in rects:
            d.rectangle([int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h)], fill=fill)
        return out
    return go


def _stickers(im):
    """Bright opaque blobs, the way a chat app decorates a photo."""
    out = im.convert("RGB").copy()
    w, h = out.size
    d = ImageDraw.Draw(out)
    r = max(8, min(w, h) // 12)
    for (cx, cy), colour in [((0.18, 0.22), (255, 206, 40)),
                             ((0.74, 0.31), (255, 90, 120)),
                             ((0.46, 0.78), (90, 200, 255))]:
        x, y = int(cx * w), int(cy * h)
        d.ellipse([x - r, y - r, x + r, y + r], fill=colour)
        d.ellipse([x - r // 3, y - r // 4, x - r // 8, y], fill=(30, 30, 30))
        d.ellipse([x + r // 8, y - r // 4, x + r // 3, y], fill=(30, 30, 30))
    return out


def _blur_patch(rect, radius_frac=0.06):
    """A censored region: everything else untouched."""
    x0, y0, x1, y1 = rect

    def go(im):
        out = im.convert("RGB").copy()
        w, h = out.size
        box = (int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h))
        patch = out.crop(box).filter(
            ImageFilter.GaussianBlur(max(3, int(min(w, h) * radius_frac))))
        out.paste(patch, box)
        return out
    return go


def _timestamp(im):
    """A camcorder date burnt into the corner."""
    out = im.convert("RGB").copy()
    w, h = out.size
    d = ImageDraw.Draw(out)
    d.text((int(w * 0.62), int(h * 0.92)), "2004-08-21  19:42", fill=(255, 160, 40))
    return out


def _corner_logo(im):
    """A broadcaster's bug, opaque, top-left."""
    out = im.convert("RGB").copy()
    w, h = out.size
    d = ImageDraw.Draw(out)
    s = max(16, min(w, h) // 11)
    d.rounded_rectangle([w // 40, h // 40, w // 40 + s, h // 40 + s],
                        radius=s // 5, fill=(200, 20, 30))
    d.text((w // 40 + s // 4, h // 40 + s // 3), "N", fill=(255, 255, 255))
    return out


def _boxes_jit(salt, lo=1, hi=3):
    """Opaque bars, how many, how big and where all drawn per seed.

    Two black bars at identical coordinates on two different photographs are an
    identical region, which makes a negative pair share real content and stops
    being a clean negative. The same reasoning as the host chrome above.
    """
    def go(im):
        k = seed_key_of(im)
        bars = []
        for i in range(_jit_int(k, salt, lo, hi)):
            t = salt + 1 + 4 * i
            bw, bh = _jit(k, t, 0.25, 0.75), _jit(k, t + 1, 0.05, 0.15)
            if _pick(k, t + 2, (False, False, True)):       # now and then upright
                bw, bh = bh, bw
            x0, y0 = _jit(k, t + 3, 0.0, 1.0 - bw), _jit(k, t + 700, 0.0, 1.0 - bh)
            bars.append((x0, y0, x0 + bw, y0 + bh))
        return _boxes(bars)(im)
    return go


def _blur_patch_jit(salt, lo=0.04, hi=0.12):
    """A censored region whose size, place and softness vary with the seed."""
    def go(im):
        k = seed_key_of(im)
        fw, fh = _jit(k, salt, 0.35, 0.70), _jit(k, salt + 1, 0.35, 0.70)
        x0, y0 = _jit(k, salt + 2, 0.0, 1.0 - fw), _jit(k, salt + 3, 0.0, 1.0 - fh)
        return _blur_patch((x0, y0, x0 + fw, y0 + fh), _jit(k, salt + 4, lo, hi))(im)
    return go


def _tiled_watermark(im):
    """A repeating stamp over the whole frame, not one corner.

    Deliberately heavy: a faint mark is just a re-encode wearing a hat, and
    would test nothing a JPEG quality does not already test.
    """
    k = seed_key_of(im)
    out = _fit(im, _PIXEL_PIXELS).convert("RGBA")
    w, h = out.size
    step = max(40, min(w, h) // (3 + k % 3))
    layer = Image.new("RGBA", (w * 2, h * 2), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    stroke = max(2, step // (7 + k % 6))
    for gy in range(0, h * 2, step):
        for gx in range(0, w * 2, step):
            d.line([(gx, gy + step * 3 // 4), (gx + step // 2, gy + step // 6)],
                   fill=(255, 255, 255, 150), width=stroke)
            d.line([(gx + step // 2, gy + step // 6),
                    (gx + step * 5 // 6, gy + step * 3 // 4)],
                   fill=(255, 255, 255, 150), width=stroke)
            d.ellipse([gx + step // 6, gy + step // 6,
                       gx + step // 6 + stroke * 2, gy + step // 6 + stroke * 2],
                      fill=(20, 20, 20, 130))
    if _pick(k, 406, (False, True)):           # some stamps lean the other way
        layer = layer.transpose(Image.FLIP_LEFT_RIGHT)
    alpha = _jit(k, 407, 0.7, 1.25)
    layer.putalpha(layer.getchannel("A").point(lambda v: min(255, int(v * alpha))))
    layer = layer.rotate(18 + k % 40, resample=Image.BICUBIC, expand=False)
    layer = layer.crop((w // 2, h // 2, w // 2 + w, h // 2 + h))
    return Image.alpha_composite(out, layer).convert("RGB")


def _qr_sticker(im):
    """A scannable-looking block pasted over one corner."""
    k = seed_key_of(im)
    out = im.convert("RGB").copy()
    w, h = out.size
    s = max(40, int(min(w, h) * _jit(k, 410, 0.30, 0.55)))
    ix, iy = int(w * _jit(k, 411, 0.015, 0.15)), int(h * _jit(k, 412, 0.015, 0.15))
    left, top = _pick(k, 413, ((False, False), (True, False), (False, True), (True, True)))
    x0 = ix if left else w - s - ix
    y0 = iy if top else h - s - iy
    d = ImageDraw.Draw(out)
    d.rectangle([x0 - 4, y0 - 4, x0 + s + 4, y0 + s + 4], fill=(255, 255, 255))
    cell = max(2, s // 21)
    for r in range(21):
        for c in range(21):
            if ((r * (5 + k % 9) + c * (11 + k % 7) + r * c + k) % 3) == 0:
                d.rectangle([x0 + c * cell, y0 + r * cell,
                             x0 + (c + 1) * cell, y0 + (r + 1) * cell], fill=(0, 0, 0))
    for cx, cy in [(0, 0), (14, 0), (0, 14)]:
        d.rectangle([x0 + cx * cell, y0 + cy * cell,
                     x0 + (cx + 7) * cell, y0 + (cy + 7) * cell], fill=(0, 0, 0))
        d.rectangle([x0 + (cx + 2) * cell, y0 + (cy + 2) * cell,
                     x0 + (cx + 5) * cell, y0 + (cy + 5) * cell], fill=(255, 255, 255))
    return out


def _subtitle_band(im):
    """An opaque caption band across the lower fifth."""
    out = im.convert("RGB").copy()
    w, h = out.size
    d = ImageDraw.Draw(out)
    d.rectangle([0, int(h * 0.80), w, int(h * 0.96)], fill=(8, 8, 10))
    k = seed_key_of(im)
    d.text((int(w * 0.06), int(h * 0.855)),
           ["- and then it stopped raining", "- nobody had told him yet",
            "- the ferry leaves at six", "- she never answered that",
            "- it was colder than it looked"][k % 5],
           fill=(248, 248, 240))
    return out


def _torn_corner(im):
    """A wedge of the print missing, as if torn away."""
    out = im.convert("RGB").copy()
    w, h = out.size
    d = ImageDraw.Draw(out)
    d.polygon([(w, 0), (w, int(h * 0.42)), (int(w * 0.58), 0)], fill=(252, 250, 246))
    d.line([(int(w * 0.58), 0), (w, int(h * 0.42))], fill=(214, 208, 198),
           width=max(2, w // 200))
    return out


def _lens_blob(im):
    """A finger or strap over part of the lens: a big soft dark intrusion."""
    out = im.convert("RGB")
    w, h = out.size
    mask = Image.new("L", (w, h), 0)
    d = ImageDraw.Draw(mask)
    d.ellipse([-w // 6, int(h * 0.52), int(w * 0.46), int(h * 1.25)], fill=255)
    mask = mask.filter(ImageFilter.GaussianBlur(max(3, w // 60)))
    dark = Image.new("RGB", (w, h), (26, 20, 18))
    return Image.composite(dark, out, mask)



# --------------------------------------------------------------------------
# colour
# --------------------------------------------------------------------------


def _lut(rf, gf, bf):
    def go(im):
        im = im.convert("RGB")
        table = ([max(0, min(255, int(rf(i)))) for i in range(256)]
                 + [max(0, min(255, int(gf(i)))) for i in range(256)]
                 + [max(0, min(255, int(bf(i)))) for i in range(256)])
        return im.point(table)
    return go


def _teal_orange(a):
    """The colour grade every action film shipped for a decade."""
    def curve(v, lo, hi):
        t = v / 255.0
        return 255 * (lo * a + (1 + (hi - 1) * a - lo * a) * (t ** (1 + 0.08 * a)))
    return _lut(lambda v: curve(v, -0.03, 1.06),
                lambda v: curve(v, 0.01, 0.99),
                lambda v: curve(v, 0.10, 0.90))


def _cross_process(a):
    """Opposed S-curves per channel, the way a mismatched developer reads."""
    def curve(v, k):
        t = v / 255.0
        return 255 * (t + a * k * t * (1 - t) * (t - 0.5) * 4)
    return _lut(lambda v: curve(v, 0.55),
                lambda v: curve(v, 0.10),
                lambda v: curve(v, -0.50))


def _tungsten(a):
    return _lut(lambda v: v * (1 - 0.20 * a),
                lambda v: v * (1 - 0.03 * a),
                lambda v: v * (1 + 0.28 * a))


def _levels_crush(a):
    lo, hi = 34 * a, 255 - (255 - 226) * a
    return _lut(*(3 * [lambda v: (v - lo) * 255.0 / max(1.0, hi - lo)]))


def _split_tone(a):
    """Cool shadows, warm highlights, weighted by luminance."""
    def go(im):
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS), dtype=np.float32)
        lum = src.mean(axis=2, keepdims=True) / 255.0
        shadow = np.array([-14.0, -2.0, 26.0], dtype=np.float32) * a
        highlight = np.array([22.0, 6.0, -18.0], dtype=np.float32) * a
        out = src + shadow * (1.0 - lum) + highlight * lum
        return Image.fromarray(out.clip(0, 255).astype("uint8"), "RGB")
    return go


def _channel_perm(order):
    def go(im):
        ch = im.convert("RGB").split()
        return Image.merge("RGB", tuple(ch[i] for i in order))
    return go


def _local_contrast(percent):
    """Large-radius unsharp: the flat, over-processed HDR look."""
    def go(im):
        im = im.convert("RGB")
        r = max(6, min(im.size) // 18)
        return im.filter(ImageFilter.UnsharpMask(
            radius=r, percent=int(percent), threshold=2))
    return go


def _sepia(im):
    grey = ImageOps.grayscale(im)
    return ImageOps.colorize(grey, (34, 20, 8), (255, 236, 205)).convert("RGB")


def _channel_shift(r, g, b):
    def go(im):
        rc, gc, bc = im.convert("RGB").split()
        return Image.merge("RGB", (
            rc.point(lambda v: min(255, int(v * r))),
            gc.point(lambda v: min(255, int(v * g))),
            bc.point(lambda v: min(255, int(v * b))),
        ))
    return go


def _gamma(g):
    table = [min(255, int(((i / 255.0) ** g) * 255)) for i in range(256)] * 3
    return lambda im: im.convert("RGB").point(table)


def _duotone(dark, light):
    return lambda im: ImageOps.colorize(ImageOps.grayscale(im), dark, light).convert("RGB")


def _hue_rotate(degrees):
    def go(im):
        hsv = im.convert("RGB").convert("HSV")
        hh, ss, vv = hsv.split()
        shift = int(degrees / 360 * 255) % 255
        hh = hh.point(lambda v: (v + shift) % 255)
        return Image.merge("HSV", (hh, ss, vv)).convert("RGB")
    return go


def _quantise(colours):
    return lambda im: im.convert("RGB").quantize(
        colors=colours, method=Image.MEDIANCUT).convert("RGB")


def _dither_1bit(im):
    return im.convert("RGB").convert("1").convert("RGB")


def _flatten(bg):
    """Composite an alpha channel onto a flat background.

    For the two RGBA seeds the white and black versions genuinely differ,
    everywhere the image was transparent, and are the same image. For the
    opaque ones this is a re-encode; `identical_bytes` in pairs.csv flags
    that rather than hiding it.
    """
    def go(im):
        if im.mode not in ("RGBA", "LA", "P"):
            return im.convert("RGB")
        rgba = im.convert("RGBA")
        out = Image.new("RGB", rgba.size, bg)
        out.paste(rgba, mask=rgba.split()[3])
        return out
    return go



# --------------------------------------------------------------------------
# degradation
# --------------------------------------------------------------------------


def _noise(sigma, seed):
    def go(im):
        import numpy as np
        rng = np.random.default_rng(seed)
        arr = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS), dtype=np.int16)
        arr = arr + rng.normal(0, sigma, arr.shape).astype(np.int16)
        return Image.fromarray(arr.clip(0, 255).astype("uint8"), "RGB")
    return go


def _motion_blur(frac, degrees):
    """Average along a line: a moved camera, not a defocused one."""
    def go(im):
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS), dtype=np.float32)
        h, w = src.shape[:2]
        n = max(3, int(max(w, h) * frac))
        dx = math.cos(math.radians(degrees))
        dy = math.sin(math.radians(degrees))
        acc = np.zeros_like(src)
        for i in range(n):
            t = i - (n - 1) / 2.0
            acc += np.roll(np.roll(src, int(round(t * dy)), axis=0),
                           int(round(t * dx)), axis=1)
        return Image.fromarray((acc / n).clip(0, 255).astype("uint8"), "RGB")
    return go


def _film_grain(sigma=9.0, seed=20260916):
    """Luminance noise that scales with exposure, plus the softness that
    always comes with it. A's `noise` is flat additive RGB."""
    def go(im):
        rng = np.random.default_rng(seed)
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS), dtype=np.float32)
        lum = src.mean(axis=2, keepdims=True) / 255.0
        grain = rng.normal(0, sigma, src.shape[:2] + (1,)).astype(np.float32)
        out = src + grain * (0.35 + 0.65 * np.sqrt(lum))
        img = Image.fromarray(out.clip(0, 255).astype("uint8"), "RGB")
        return img.filter(ImageFilter.GaussianBlur(0.6))
    return go


def _halftone(angle=45.0, pitch=6.0):
    """A clustered-dot print screen, not an error-diffused bitmap."""
    def go(im):
        grey = np.asarray(ImageOps.grayscale(_fit(im, _PIXEL_PIXELS)), dtype=np.float32) / 255.0
        h, w = grey.shape
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        a = math.radians(angle)
        u = (xx * math.cos(a) + yy * math.sin(a)) * (2 * math.pi / pitch)
        v = (-xx * math.sin(a) + yy * math.cos(a)) * (2 * math.pi / pitch)
        screen = 0.5 + 0.5 * np.sin(u) * np.sin(v)
        out = np.where(grey > screen, 255, 0).astype("uint8")
        return Image.fromarray(out, "L").convert("RGB")
    return go


def _chromatic_aberration(spread=0.004, margin=0.012):
    """Channels scaled apart about the centre, as a cheap lens does."""
    def go(im):
        im = _fit(im.convert("RGB"), _PIXEL_PIXELS)
        w, h = im.size
        out = []
        for ch, f in zip(im.split(), (1.0 + spread, 1.0, 1.0 - spread)):
            nw, nh = max(1, round(w * f)), max(1, round(h * f))
            scaled = ch.resize((nw, nh), Image.BICUBIC)
            plane = Image.new("L", (w, h), 0)
            plane.paste(scaled, ((w - nw) // 2, (h - nh) // 2))
            out.append(plane)
        merged = Image.merge("RGB", tuple(out))
        return _crop_box(merged, margin, margin, 1 - margin, 1 - margin)
    return go


def _vignette(strength=0.78):
    def go(im):
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS), dtype=np.float32)
        h, w = src.shape[:2]
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        nx = (xx - (w - 1) / 2) / ((w - 1) / 2)
        ny = (yy - (h - 1) / 2) / ((h - 1) / 2)
        r2 = np.clip((nx * nx + ny * ny) / 2.0, 0, 1)[:, :, None]
        return Image.fromarray(
            (src * (1.0 - strength * r2 ** 1.4)).clip(0, 255).astype("uint8"), "RGB")
    return go


def _scanline_moire(im, period=3.7, depth=0.18):
    """A phone photograph of an LCD: banding plus a slight colour beat."""
    src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS), dtype=np.float32)
    h, w = src.shape[:2]
    yy = np.arange(h, dtype=np.float32)[:, None, None]
    xx = np.arange(w, dtype=np.float32)[None, :, None]
    band = 1.0 - depth * (0.5 + 0.5 * np.sin(yy * (2 * np.pi / period)))
    beat = 1.0 + 0.05 * np.sin(xx * (2 * np.pi / 2.9) + np.array([0.0, 2.1, 4.2]))
    return Image.fromarray((src * band * beat).clip(0, 255).astype("uint8"), "RGB")


def _moire_jit(salt):
    """Banding of a per-seed period and depth, across or along the picture."""
    def go(im):
        k = seed_key_of(im)
        return _scanline_moire(im, _jit(k, salt, 3.0, 5.0), _jit(k, salt + 1000, 0.10, 0.24))
    return _oriented(salt + 2000, AXES, go)[0]



# --------------------------------------------------------------------------
# rearrangement: every pixel survives, the photograph does not
# --------------------------------------------------------------------------


def _tile_shuffle(cols, rows, order, key):
    """Rearrange the tiles. Every pixel survives; the image does not."""
    def go(im):
        im = im.convert("RGB")
        w, h = im.size
        tw, th = w // cols, h // rows
        out = Image.new("RGB", (tw * cols, th * rows))
        tiles = [im.crop((c * tw, r * th, (c + 1) * tw, (r + 1) * th))
                 for r in range(rows) for c in range(cols)]
        for dest, src in enumerate(order):
            out.paste(tiles[src], ((dest % cols) * tw, (dest // cols) * th))
        return out
    go.scramble_key = key
    return go


def _derangement(key, salt, n):
    """A per-seed order of n tiles that leaves none where it was."""
    for attempt in range(64):
        order = list(range(n))
        for i in range(n - 1, 0, -1):          # Fisher-Yates, drawn from the key
            j = _jit_int(key, salt + 97 * attempt + i, 0, i)
            order[i], order[j] = order[j], order[i]
        if all(o != i for i, o in enumerate(order)):
            return order
    return order[1:] + order[:1]


def _tile_shuffle_jit(salt):
    """Two-by-two or three-by-three, in an order drawn per seed."""
    def go(im):
        k = seed_key_of(im)
        g = _jit_int(k, salt, 2, 3)
        return _tile_shuffle(g, g, _derangement(k, salt + 1000, g * g), "shuffle")(im)
    return go


def _quadrant_rotate(im):
    im = im.convert("RGB")
    w, h = im.size
    hw, hh = w // 2, h // 2
    out = im.copy()
    for x0, y0 in [(0, 0), (hw, 0), (0, hh), (hw, hh)]:
        box = (x0, y0, x0 + hw, y0 + hh)
        out.paste(im.crop(box).transpose(Image.ROTATE_180), box)
    return out


def _swap_halves(im):
    im = im.convert("RGB")
    w, h = im.size
    hw = w // 2
    out = Image.new("RGB", (hw * 2, h))
    out.paste(im.crop((hw, 0, hw * 2, h)), (0, 0))
    out.paste(im.crop((0, 0, hw, h)), (hw, 0))
    return out


def _column_roll(frac):
    def go(im):
        src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS))
        return Image.fromarray(
            np.roll(src, int(src.shape[1] * frac), axis=1), "RGB")
    return go



# --------------------------------------------------------------------------
# chains
# --------------------------------------------------------------------------


def _keyed(fn):
    """Run `fn` with every step inside drawing from the key of the image it
    was handed: a later step of a chain sees a cropped or turned image, whose
    own hash is not the seed's, and its region was worked out from the seed's."""
    def go(im):
        global _KEY
        outer = _KEY
        _KEY = seed_key_of(im)
        try:
            return fn(im)
        finally:
            _KEY = outer
    return go


def _chain(*fns):
    def go(im):
        for fn in fns:
            im = fn(im)
        return im
    return _keyed(go)


def _jpeg_roundtrip(quality, times=1):
    """Actually re-encode through JPEG in memory, for generation loss."""
    import io

    def go(im):
        im = im.convert("RGB")
        for _ in range(times):
            buf = io.BytesIO()
            im.save(buf, "JPEG", quality=quality)
            buf.seek(0)
            im = Image.open(buf).convert("RGB")
        return im
    return go


def _jpeg_gen(quality, times):
    def go(im):
        im = im.convert("RGB")
        for _ in range(times):
            buf = io.BytesIO()
            im.save(buf, "JPEG", quality=quality)
            buf.seek(0)
            im = Image.open(buf).convert("RGB")
        return im
    return go


def _jpeg_gen_jit(salt, q_lo, q_hi, t_lo, t_hi):
    """Generation loss: both the quality and the number of rounds per seed."""
    def go(im):
        key = seed_key_of(im)
        return _jpeg_gen(_jit_int(key, salt, q_lo, q_hi),
                         _jit_int(key, salt + 1, t_lo, t_hi))(im)
    return go


def _mirror(im):
    return ImageOps.mirror(im.convert("RGB"))


def _bilevel(im):
    """Down to one bit per pixel, which is what a fax codec demands."""
    return im.convert("L").point(lambda v: 255 if v > 127 else 0, mode="1")


# --------------------------------------------------------------------------
# hosts whose canvas is drawn per seed
#
# These five came from the older catalogues with their proportions hard-coded:
# a slide was always 1.6 times the width of the photograph on it, a PDF page
# always left the same margin. They are factories now, so the photograph sits
# in a different fraction of its page for every seed.
#
# Their declared `coverage` stays a constant, as it always was. It feeds one
# decision — whether a pair is SAME or a containment — and that turns on
# whether coverage is under 0.9, which every value in these ranges is by a wide
# margin. Being approximate there has never mattered and still does not.


def _slide(s):
    """The image on a presentation slide: title above, margins around."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(im.convert("RGB"))
        w, h = im.size
        cw = int(w * s)
        ch = int(cw * 9 / 16)
        if ch < h * 1.35:
            ch = int(h * 1.35)
            cw = int(ch * 16 / 9)
        out = Image.new("RGB", (cw, ch), (247, 247, 250))
        d = ImageDraw.Draw(out)
        d.rectangle([0, 0, cw, ch // (7 + k % 5)], fill=(31, (62 + k) % 160, 110))
        d.text((cw // 30, ch // 28), _SLIDE_TITLES[k % len(_SLIDE_TITLES)],
               fill=(255, 255, 255))
        out.paste(im, ((cw - w) // 2, int(ch * (0.15 + (k % 6) * 0.012))))
        d.text((cw // 30, ch - ch // 14), "figure %d" % (1 + k % 9),
               fill=(110, 110, 120))
        return out
    return go


def _phone_screenshot(ratio):
    """The image in a phone viewer: status bar, nav bar, dark surround."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(im.convert("RGB"))
        w, h = im.size
        cw = w
        ch = max(int(w * ratio), h + w // 4)
        out = Image.new("RGB", (cw, ch), (16, 16, 18))
        d = ImageDraw.Draw(out)
        bar = max(18, ch // 38)
        d.rectangle([0, 0, cw, bar], fill=(0, 0, 0))
        d.text((cw // 24, bar // 5), "%d:%02d" % (1 + k % 12, k % 60),
               fill=(255, 255, 255))
        d.text((cw - cw // 5, bar // 5), "%d%%" % (40 + k % 60),
               fill=(255, 255, 255))
        out.paste(im, (0, (ch - h) // 2))
        navy = ch - bar
        d.rectangle([0, navy, cw, ch], fill=(0, 0, 0))
        d.rounded_rectangle([cw // 3, navy + bar // 3, cw - cw // 3, navy + bar // 3 + 4],
                            radius=2, fill=(200, 200, 200))
        return out
    return go


def _social_card(s):
    """Avatar, handle, and a row of action icons around the image."""
    def go(im):
        im = im.convert("RGB")
        w, h = im.size
        head = max(40, int(h / s))
        foot = max(34, int(h / (s * 1.25)))
        out = Image.new("RGB", (w, h + head + foot), (255, 255, 255))
        d = ImageDraw.Draw(out)
        pad = head // 5
        d.ellipse([pad, pad, head - pad, head - pad], fill=(190, 200, 215))
        d.text((head, head // 3), "@someone", fill=(20, 20, 25))
        out.paste(im, (0, head))
        y = h + head + foot // 3
        for i in range(4):
            x = pad + i * (w // 7)
            d.ellipse([x, y, x + foot // 3, y + foot // 3], outline=(120, 125, 135), width=2)
        return out
    return go


def _collage(cols, rows):
    """One cell of a collage; every other cell is synthetic and is not it."""
    def go(im):
        key = seed_key_of(im)
        im = _fit(im.convert("RGB"))
        w, h = im.size
        out = Image.new("RGB", (w * cols, h * rows), (20, 20, 22))
        spot = (_jit_int(key, 902, 0, cols - 1), _jit_int(key, 903, 0, rows - 1))
        out.paste(im, (spot[0] * w, spot[1] * h))
        for k, (cx, cy) in enumerate((x, y) for y in range(rows) for x in range(cols)
                                     if (x, y) != spot):
            cell = Image.new("RGB", (w, h))
            dd = ImageDraw.Draw(cell)
            j = k + key
            for i in range(0, h, max(4, h // (24 + key % 30))):
                v = (i * (37 + 29 * j) + 5 * key) % 256
                dd.line([(0, i), (w, i)],
                        fill=(v, (v * 3 + 60 * j) % 256, (255 - v + key) % 256),
                        width=2 + j % 4)
            out.paste(cell, (cx * w, cy * h))
        return out
    return go


def _pdf_page(margin):
    """Rasterised document page: the figure on white with generous margins."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(im.convert("RGB"))
        w, h = im.size
        cw = int(w / margin)
        ch = int(cw * 297 / 210)
        if ch < h * 1.2:
            ch = int(h * 1.2)
            cw = int(ch * 210 / 297)
        out = Image.new("RGB", (cw, ch), (255, 255, 255))
        out.paste(im, ((cw - w) // 2, int(ch * 0.14)))
        d = ImageDraw.Draw(out)
        for i in range(4 + k % 6):
            y = int(ch * 0.14) + h + int(ch * 0.03) + i * max(9, ch // (55 + k % 30))
            if y < ch - ch // 20:
                d.rectangle([cw // 8, y, cw - cw // 8 - ((i + k) % 4) * cw // 9,
                             y + max(3, ch // 300)], fill=(60, 60, 66))
        return out
    return go


def _embed_jit(salt, lo, hi, bg=(255, 255, 255), where="center"):
    """Paste onto a flat canvas whose size — and so the coverage — varies."""
    def go(im):
        k = seed_key_of(im)
        at = (_jit(k, salt + 2, 0.0, 1.0), _jit(k, salt + 3, 0.0, 1.0)) \
            if where == "anywhere" else where
        return _embed(_jit(k, salt, lo, hi), bg, at)(im)

    def coverage(key):
        f = _jit(key, salt, lo, hi)
        return f * f

    return go, coverage


def _persp_top_jit(salt, lo, hi):
    """Keystone about the horizontal axis: A's direction, per seed."""
    def region(w, h, key):
        s = _jit(key, salt, lo, hi)
        return (s, 0.0, 1.0 - s, 1.0)

    def go(im):
        return _perspective(_jit(seed_key_of(im), salt, lo, hi))(im)

    return go, region


def _shear_jit(salt, lo, hi):
    """Affine shear, cropped back to a rectangle of real content."""
    def region(w, h, key):
        m = _jit(key, salt, lo, hi) / 2
        return (m, 0.0, 1.0 - m, 1.0)

    def go(im):
        return _shear(_jit(seed_key_of(im), salt, lo, hi))(im)

    return go, region


def _wave_h_jit(salt, amp_lo, amp_hi, per_lo, per_hi):
    """Horizontal sinusoid: amplitude and period both per seed."""
    def region(w, h, key):
        a = _jit(key, salt, amp_lo, amp_hi)
        return (a, 0.0, 1.0 - a, 1.0)

    def go(im):
        key = seed_key_of(im)
        return _wave(_jit(key, salt, amp_lo, amp_hi),
                     _jit(key, salt + 1, per_lo, per_hi))(im)

    return go, region

# --------------------------------------------------------------------------
# the catalogue
#
# One entry per *axis*, not per point on it. The older catalogues needed
# jpeg_q90, q75, q50, q30, q15 and q08 to cover JPEG quality; three entries
# cover it here, each spanning a range, because the quality is drawn per seed.
# That is why 88 entries replace 196 and still test more of every axis.

def _noise_jit(salt, lo, hi):
    """Noise of a per-seed strength, and its own pattern: one generator seed for
    every photograph put the same grain on all of them."""
    def go(im):
        k = seed_key_of(im)
        return _noise(_jit(k, salt, lo, hi), 1_000_003 * k + salt)(im)
    return go


def _motion_blur_jit(salt, lo, hi):
    """A moved camera: length and direction per seed (it was always 20 degrees)."""
    def go(im):
        k = seed_key_of(im)
        return _motion_blur(_jit(k, salt, lo, hi), _jit(k, salt + 1000, 0.0, 180.0))(im)
    return go


def _halftone_jit(salt, lo, hi):
    def go(im):
        k = seed_key_of(im)
        return _halftone(_jit(k, salt + 1000, 0.0, 90.0), _jit(k, salt, lo, hi))(im)
    return go


def _stretch_either(salt, lo, hi):
    """Stretched or squashed along one axis, which one drawn per seed."""
    def go(im):
        k = seed_key_of(im)
        return _stretch_jit(salt, lo, hi, axis=_pick(k, salt + 1000, ("x", "y")))(im)
    return go


def _pinch_jit(salt, lo, hi, bulge_lo, bulge_hi):
    """The centre magnified (pinch) or, for some seeds, shrunk (a bulge read
    the other way): sign and strength per seed. The rim trimmed does not vary."""
    def go(im):
        k = seed_key_of(im)
        a = _jit(k, salt, lo, hi) if _pick(k, salt + 1000, (True, False)) \
            else -_jit(k, salt + 2000, bulge_lo, bulge_hi)
        return _pinch(a, 0.06)(im)
    return go, (0.06, 0.06, 0.94, 0.94)


def _quarter_turn(salt):
    """A quarter turn one way or the other."""
    def go(im):
        k = seed_key_of(im)
        return im.convert("RGB").transpose(
            _pick(k, salt, (Image.ROTATE_90, Image.ROTATE_270)))

    def geometry(key):
        return _pick(key, salt, ("rot90", "rot270"))

    return go, geometry


def _transpose_either(salt):
    """Mirrored about one diagonal or the other."""
    def go(im):
        k = seed_key_of(im)
        return im.convert("RGB").transpose(
            _pick(k, salt, (Image.TRANSPOSE, Image.TRANSVERSE)))
    return go


def _mirror_jit(salt):
    """Mirrored left-right or top-bottom (a top-bottom mirror is the left-right
    one turned half way round: a mirror image either way)."""
    def go(im):
        k = seed_key_of(im)
        return im.convert("RGB").transpose(
            _pick(k, salt, (Image.FLIP_LEFT_RIGHT, Image.FLIP_TOP_BOTTOM)))
    return go



# --------------------------------------------------------------------------
# harder in kind (IMGS-ALL2, second pass)
#
# A transform img-fp found for every seed of the first pass was made harder
# the way that transform gets harder in use, not by stacking an unrelated
# one: a turned or mirrored copy is a copy that was also sent on (smaller,
# re-saved), a black-and-white conversion goes through a colour filter, a
# negative is graded, a blur's radius is the picture's, not a pixel count.
# Salts 801-899 (820 skipped).

def _shared(salt, lo, hi):
    """Sent on: smaller by a per-seed factor, through a per-seed kernel."""
    def go(im):
        k = seed_key_of(im)
        f = _jit(k, salt, lo, hi)
        w, h = im.size
        kernel = _pick(k, salt + 1000, (Image.BILINEAR, Image.BICUBIC,
                                        Image.LANCZOS, Image.BOX))
        return im.convert("RGB").resize((max(1, round(w * f)), max(1, round(h * f))),
                                        kernel)
    return go


def _upscale_from_small(salt, d_lo, d_hi, u_lo, u_hi):
    """Enlarged from a small copy, as an upscale found in the wild is: down to
    `d` of the seed, then up to `u` of it."""
    def go(im):
        k = seed_key_of(im)
        im = im.convert("RGB")
        w, h = im.size
        d = _jit(k, salt, d_lo, d_hi)
        u = _jit(k, salt + 1, u_lo, u_hi)
        if w * h * u * u > _RESIZE_PIXELS:
            u = (_RESIZE_PIXELS / float(w * h)) ** 0.5
        small = im.resize((max(1, round(w * d)), max(1, round(h * d))), Image.BOX)
        return small.resize((max(1, round(w * u)), max(1, round(h * u))), Image.BICUBIC)
    return go


def _blur_rel(salt, lo, hi):
    """Gaussian blur whose radius is a share of the picture's long side."""
    def go(im):
        im = im.convert("RGB")
        r = _jit(seed_key_of(im), salt, lo, hi) * max(im.size)
        return im.filter(ImageFilter.GaussianBlur(max(0.3, r)))
    return go


def _unsharp_jit(salt):
    """Over-sharpened: an unsharp mask of per-seed radius and strength."""
    def go(im):
        im = im.convert("RGB")
        k = seed_key_of(im)
        r = max(1.0, _jit(k, salt, 0.002, 0.007) * max(im.size))
        return im.filter(ImageFilter.UnsharpMask(
            radius=r, percent=_jit_int(k, salt + 1, 250, 650), threshold=0))
    return go


_FILTERS = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0),
            (0.6, 0.4, 0.0), (0.0, 0.4, 0.6), (1 / 3, 1 / 3, 1 / 3))


def _grey_mix(k, salt, im):
    """Black and white through a colour filter drawn per seed (red, green,
    blue, orange, cyan or plain average), then a tone curve: contrast and
    gamma per seed."""
    r, g, b = _pick(k, salt, _FILTERS)
    src = np.asarray(_fit(im.convert("RGB"), _PIXEL_PIXELS), dtype=np.float32)
    grey = (src[..., 0] * r + src[..., 1] * g + src[..., 2] * b) / 255.0
    c = _jit(k, salt + 1, 0.75, 1.7)
    grey = np.clip((grey - 0.5) * c + 0.5, 0.0, 1.0) ** _jit(k, salt + 2, 0.65, 1.5)
    return Image.fromarray((grey * 255).astype("uint8"), "L")


def _greyscale_jit(salt):
    def go(im):
        return _grey_mix(seed_key_of(im), salt, im).convert("RGB")
    return go


_DUO = (((16, 24, 70), (250, 238, 200)), ((60, 10, 40), (255, 210, 120)),
        ((0, 40, 30), (200, 255, 220)), ((40, 0, 0), (255, 245, 235)),
        ((10, 10, 10), (120, 200, 255)), ((70, 30, 0), (240, 240, 140)))


def _duotone_jit(salt):
    """Two inks drawn per seed, over a filtered black and white."""
    def go(im):
        k = seed_key_of(im)
        dark, light = _pick(k, salt + 10, _DUO)
        return ImageOps.colorize(_grey_mix(k, salt, im), dark, light).convert("RGB")
    return go


_PERMS = ((2, 1, 0), (1, 0, 2), (0, 2, 1), (1, 2, 0), (2, 0, 1))


def _channel_jit(salt):
    """Channels in another order (any of the five), and for some seeds one
    of them inverted."""
    def go(im):
        k = seed_key_of(im)
        ch = list(im.convert("RGB").split())
        ch = [ch[i] for i in _pick(k, salt, _PERMS)]
        flip = _pick(k, salt + 1, (None, None, 0, 1, 2))
        if flip is not None:
            ch[flip] = ImageOps.invert(ch[flip])
        return Image.merge("RGB", tuple(ch))
    return go


def _hue_sat_jit(salt):
    """Hue turned by a per-seed angle, then saturation pushed."""
    def go(im):
        k = seed_key_of(im)
        im = _hue_rotate(_jit(k, salt, 25, 335))(im)
        return ImageEnhance.Color(im).enhance(_jit(k, salt + 1, 1.2, 2.6))
    return go


def _invert_graded(salt):
    """A negative, then a per-seed gamma: inverted and edited, not inverted."""
    def go(im):
        im = ImageOps.invert(im.convert("RGB"))
        return _gamma(_jit(seed_key_of(im), salt, 0.5, 2.0))(im)
    return _keyed(go)


def _zoom_free(salt, lo, hi):
    """A crop of one per-seed share of both sides (the aspect kept), anywhere."""
    def rect(key):
        f = _jit(key, salt, lo, hi)
        x0 = _jit(key, salt + 2, 0.0, 1.0 - f)
        y0 = _jit(key, salt + 3, 0.0, 1.0 - f)
        return (x0, y0, x0 + f, y0 + f)

    def go(im):
        return _crop_box(im.convert("RGB"), *rect(seed_key_of(im)))

    return go, (lambda w, h, key: rect(key))


_crop_slight_fn, _crop_slight_reg = _crop_edges(100, 0.05, 0.25)
_crop_half_fn, _crop_half_reg = _crop_free(104, 0.58, 0.84)
_crop_quarter_fn, _crop_quarter_reg = _crop_free(108, 0.38, 0.60)
_crop_micro_fn, _crop_micro_reg = _crop_free(112, 0.14, 0.27)
_crop_off_fn, _crop_off_reg = _oriented(
    501, ALL_SIDES, *_crop_jit(116, .33, .27, .70, .65, spread=.15))
_crop_strip_fn, _crop_strip_reg = _oriented(502, TURNS, *_strip_jit(120, 0.28, 0.60))
_mircrop_fn, _mircrop_reg = _crop_free(132, 0.55, 0.84)
_call_fn, _call_reg = _crop_edges(136, 0.08, 0.25)

_sq_fn, _sq_reg = _chain_regions(_zoom_free(801, 0.55, 0.92), _aspect_jit(150, 0.90, 1.12))
_wide_fn, _wide_reg = _aspect_jit(155, 1.62, 2.50, either_way=True)

_rot_small_fn, _rot_small_reg = _rotate_jit(160, 2.0, 12.0)
_rot_big_fn, _rot_big_reg = _rotate_jit(164, 12.0, 45.0)
_rot_scan_fn, _rot_scan_reg = _rotate_jit(168, 1.0, 8.0)

# one edge pulled in, any of the four
_persp_fn, _persp_reg = _oriented(504, TURNS, *_persp_top_jit(170, 0.07, 0.24))
# two: top or bottom, and left or right, each by its own amount
_persp2_fn, _persp2_reg = _chain_regions(
    _oriented(505, TOP_BOTTOM, *_persp_top_jit(172, 0.05, 0.16)),
    _oriented(506, LEFT_RIGHT, *_keystone_jit(174, 0.05, 0.16)))
_screen_fn, _screen_reg = _oriented(507, TURNS, *_persp_top_jit(176, 0.05, 0.16))
_shear_fn, _shear_reg = _oriented(508, ALL_SIDES, *_shear_jit(178, 0.30, 0.50))
_wave_fn, _wave_reg = _oriented(509, ALL_SIDES, *_wave_jit(180, 0.022, 0.060, 1.4, 4.2))

_swirl_fn, _swirl_reg = _oriented(510, LEFT_RIGHT, *_warp_jit(_swirl, 182, 25.0, 80.0, 0.04))
_pinch_fn, _pinch_reg = _pinch_jit(511, 0.22, 0.60, 0.15, 0.35)
_lens_region = (0.06, 0.06, 0.94, 0.94)

_embed_small_fn, _embed_small_cov = _embed_jit(190, 0.42, 0.66, where="anywhere")
_embed_tiny_fn, _embed_tiny_cov = _embed_jit(192, 0.18, 0.35, bg=(20, 20, 24),
                                             where="anywhere")
_quarter_fn, _quarter_geometry = _quarter_turn(515)

DYNAMIC_REGIONS = {
    "crop_slight": _crop_slight_reg,
    "crop_half": _crop_half_reg,
    "crop_quarter": _crop_quarter_reg,
    "crop_micro": _crop_micro_reg,
    "crop_off_centre": _crop_off_reg,
    "crop_strip": _crop_strip_reg,
    "mirror_crop": _mircrop_reg,
    "video_call_frame": _call_reg,
    "crop_square": _sq_reg,
    "crop_panoramic": _wide_reg,
    "rotate_small": _rot_small_reg,
    "rotate_large": _rot_big_reg,
    "print_scan": _rot_scan_reg,
    "perspective": _persp_reg,
    "perspective_two": _persp2_reg,
    "photo_of_screen": _screen_reg,
    "shear": _shear_reg,
    "wave": _wave_reg,
    "swirl": _swirl_reg,
}

V = Variant

CATALOGUE = [
    # -- container and metadata, pixels untouched ---------------------------
    V("copy_exact", None, "RAW_COPY",
      note="byte-identical copy under another name; the floor every tool should find"),
    V("strip_metadata", None, "EXIF_STRIP",
      note="same compressed stream, EXIF removed: different bytes, identical pixels"),
    V("to_png", lambda im: im.convert("RGB"), "PNG", note="lossless container change"),
    V("to_tiff", lambda im: im.convert("RGB"), "TIFF", {"compression": "tiff_lzw"}),

    # -- lossy re-encode, three encoders, ranges not points -----------------
    V("jpeg_high", _shared(830, 0.08, 0.35), "JPEG", _jit_kw(230, "quality", 72, 90),
      note="sent on: smaller, then saved well"),
    V("jpeg_mid", _shared(831, 0.08, 0.35), "JPEG", _jit_kw(231, "quality", 10, 35)),
    V("jpeg_low", _shared(832, 0.10, 0.40), "JPEG", _jit_kw(232, "quality", 1, 8),
      note="where perceptual hashes start to disagree"),
    V("jpeg_magick", _shared(833, 0.12, 0.50), "JPEG",
      writer=_magick("-quality", "{q}", quality=(234, 8, 40)),
      note="ImageMagick, not Pillow: different quantisation tables"),
    V("jpeg_ffmpeg", _shared(834, 0.12, 0.50), "JPEG",
      writer=_ffmpeg("-q:v", "{q}", quality=(235, 15, 31)), note="a third JPEG encoder"),
    V("jpeg_generations", _chain(_shared(835, 0.15, 0.50), _jpeg_gen_jit(236, 20, 55, 10, 40)),
      "JPEG", _jit_kw(238, "quality", 20, 55),
      note="ten to forty rounds of re-encoding, of a smaller copy"),
    V("webp_lossy", _shared(836, 0.10, 0.45), "WEBP",
      _jit_kw(239, "quality", 0, 20, method=5)),
    V("webp_lossless", lambda im: im.convert("RGB"), "WEBP", {"lossless": True}),
    V("to_avif", _shared(837, 0.10, 0.45), "AVIF", _jit_kw(240, "quality", 1, 15)),
    V("to_heif", _shared(838, 0.10, 0.45), "HEIF", _jit_kw(241, "quality", 1, 12)),
    V("to_jxl", _shared(839, 0.10, 0.45), "JXL", _jit_kw(242, "quality", 5, 25)),

    # -- resampling, four kernels ------------------------------------------
    V("scale_large", _resize_jit(200, 0.25, 0.55, Image.LANCZOS), "JPEG", _jit_kw(802, "quality", 8, 35)),
    V("scale_mid", _resize_jit(201, 0.10, 0.30, Image.BICUBIC), "JPEG", _jit_kw(803, "quality", 25, 60)),
    V("scale_small", _resize_jit(202, 0.07, 0.27, Image.BOX), "PNG"),
    V("scale_nearest", _resize_jit(203, 0.10, 0.35, Image.NEAREST), "PNG",
      note="no filtering, so the aliasing is the artefact"),
    V("upscale", _upscale_from_small(204, 0.06, 0.20, 1.2, 3.0), "JPEG",
      _jit_kw(804, "quality", 30, 70), note="enlarged from a small copy"),
    V("thumbnail", _long_edge_jit(205, 32, 96, Image.HAMMING), "JPEG", {"quality": 88}),
    V("aspect_distort", _stretch_either(206, 0.42, 2.10), "JPEG", {"quality": 90},
      note="one axis only, either one, so the aspect ratio breaks"),

    # -- crops --------------------------------------------------------------
    V("crop_slight", _crop_slight_fn, "JPEG", _jit_kw(885, "quality", 40, 75)),
    V("crop_half", _crop_half_fn, "JPEG", {"quality": 90}),
    V("crop_quarter", _crop_quarter_fn, "JPEG", {"quality": 90}),
    V("crop_micro", _crop_micro_fn, "PNG", note="a window of a seventh to a quarter a side, anywhere"),
    V("crop_off_centre", _crop_off_fn, "JPEG", {"quality": 90}),
    V("crop_strip", _crop_strip_fn, "JPEG", {"quality": 90}, note="off any side"),
    V("crop_square", _sq_fn, "JPEG", {"quality": 90}),
    V("crop_panoramic", _wide_fn, "JPEG", {"quality": 90}, note="wide or tall"),

    # -- containment: the image inside something larger --------------------
    V("embed_small", _embed_small_fn, "PNG", coverage=_embed_small_cov),
    V("embed_tiny", _embed_tiny_fn, "PNG", coverage=_embed_tiny_cov),
    V("slide_deck", _amount(300, 1.40, 1.95, _slide), "JPEG", {"quality": 88},
      coverage=0.35),
    V("phone_screenshot", _chain(_amount(302, 1.60, 2.10, _phone_screenshot),
                               _shared(886, 0.20, 0.45)), "JPEG",
      _jit_kw(887, "quality", 40, 75),
      coverage=0.5),
    V("collage_cell", _chain(_collage(2, 2), _shared(883, 0.20, 0.50)), "JPEG",
      _jit_kw(884, "quality", 30, 65), coverage=0.25),
    V("blur_fill", _chain(_shared(841, 0.15, 0.45), _amount(306, 0.20, 0.40, _blur_fill)), "JPEG",
      _jit_kw(805, "quality", 30, 70),
      coverage=0.55, note="vertical-video framing over a blurred, zoomed copy"),
    V("pdf_page", _amount(308, 0.62, 0.82, _pdf_page), "PNG", coverage=0.5),
    V("magazine_spread", _magazine_spread, "JPEG", {"quality": 86},
      coverage=_MAGAZINE_COV),
    V("contact_sheet", _contact_sheet, "JPEG", {"quality": 84}, coverage=_sheet_cov,
      note="a ninth of the proof sheet, or a sixteenth"),
    V("browser_window", _browser_window, "PNG", coverage=_BROWSER_COV),
    V("picture_in_picture", _picture_in_picture, "JPEG", {"quality": 84},
      coverage=_PIP_COV),
    V("wall_poster", _wall_poster, "JPEG", {"quality": 82}, coverage=_POSTER_COV,
      geometry="perspective", note="printed, hung, photographed off-axis"),

    # -- occlusion ----------------------------------------------------------
    V("watermark_corner", _chain(_watermark, _shared(880, 0.20, 0.50)), "JPEG",
      _jit_kw(861, "quality", 30, 70)),
    V("tiled_watermark", _tiled_watermark, "JPEG", {"quality": 88}),
    V("caption_bar", _chain(_shared(870, 0.15, 0.45), _caption_bar), "JPEG",
      _jit_kw(871, "quality", 30, 70), coverage=lambda k: 1.0 / (1.0 + _CAPTION_SHARE(k)),
      note="meme framing: a caption strip under or over the whole image"),
    V("redaction_bars", _boxes_jit(600), "PNG"),
    V("blur_patch", _blur_patch_jit(394, 0.08, 0.20), "JPEG",
      _jit_kw(888, "quality", 40, 75)),
    V("qr_sticker", _qr_sticker, "PNG"),

    # -- colour -------------------------------------------------------------
    V("brightness", _chain(_amount(270, 0.25, 2.60,
                                   lambda a: lambda im: ImageEnhance.Brightness(im).enhance(a)),
                           _shared(881, 0.15, 0.45)),
      "JPEG", _jit_kw(882, "quality", 30, 70)),
    V("contrast", _chain(_amount(271, 0.20, 3.0,
                                 lambda a: lambda im: ImageEnhance.Contrast(im).enhance(a)),
                         _shared(874, 0.15, 0.45)),
      "JPEG", _jit_kw(875, "quality", 30, 70)),
    V("saturation", _amount(272, 3.5, 10.0,
                            lambda a: lambda im: ImageEnhance.Color(im.convert("RGB")).enhance(a)),
      "JPEG", {"quality": 92}),
    V("greyscale", _greyscale_jit(806), "JPEG", {"quality": 92},
      note="through a colour filter, then a tone curve"),
    V("temperature", _amount(273, -1.0, 1.0,
                             lambda a: _chain(_channel_shift(1 + 0.8 * a, 1.0, 1 - 0.8 * a),
                                               _shared(854, 0.08, 0.30))),
      "JPEG", _jit_kw(855, "quality", 25, 60), note="warm one way, cool the other"),
    V("gamma", _amount(274, 0.30, 3.0, _gamma), "JPEG", {"quality": 92}),
    V("hue_rotate", _hue_sat_jit(275), "JPEG", {"quality": 92}, note="and saturation pushed"),
    V("invert", _invert_graded(810), "JPEG", _jit_kw(811, "quality", 55, 90),
      note="a negative, graded"),
    V("posterize", _amount(276, 2, 12, lambda a: _quantise(int(a))), "PNG"),
    V("cinematic_grade", _chain(_amount(277, 3.0, 6.0, _teal_orange), _shared(850, 0.10, 0.35)),
      "JPEG", _jit_kw(851, "quality", 25, 60), note="graded, then exported small"),
    V("duotone", _duotone_jit(812), "PNG", note="inks per seed"),
    V("channel_swap", _channel_jit(814), "PNG"),

    # -- geometry -----------------------------------------------------------
    V("flip_h", _chain(_mirror, _shared(815, 0.30, 1.0)), "JPEG", _jit_kw(816, "quality", 35, 90),
      geometry="mirror"),
    V("rot90", _chain(_quarter_fn, _shared(817, 0.06, 0.25)), "JPEG",
      _jit_kw(818, "quality", 10, 45), geometry=_quarter_geometry,
      note="a quarter turn, either way"),
    V("rot180", _chain(lambda im: im.convert("RGB").transpose(Image.ROTATE_180),
                      _shared(819, 0.06, 0.25)), "JPEG",
      _jit_kw(821, "quality", 10, 45), geometry="rot180"),
    V("transpose_diag", _chain(_transpose_either(516), _shared(822, 0.30, 1.0)), "JPEG",
      _jit_kw(823, "quality", 35, 90), geometry="rot90",
      note="mirrored about either diagonal"),
    V("mirror_crop", _chain(_mircrop_fn, _mirror_jit(503)), "JPEG", {"quality": 88},
      geometry="mirror", note="cropped anywhere, then mirrored either way"),
    V("rotate_small", _chain(_rot_small_fn, _shared(842, 0.15, 0.50)), "JPEG",
      _jit_kw(843, "quality", 30, 80), geometry="rotate"),
    V("rotate_large", _chain(_rot_big_fn, _shared(878, 0.15, 0.45)), "JPEG",
      _jit_kw(879, "quality", 30, 70), geometry="rotate"),
    V("perspective", _persp_fn, "JPEG", {"quality": 86}, geometry="perspective",
      note="one edge pulled in, any of the four"),
    V("perspective_two", _persp2_fn, "JPEG", {"quality": 86}, geometry="perspective",
      note="pulled in along both axes"),
    V("shear", _chain(_shear_fn, _shared(862, 0.15, 0.45)), "JPEG",
      _jit_kw(863, "quality", 30, 75), geometry="rotate"),
    V("barrel_distort", _amount(184, -0.36, -0.12, lambda a: _radial_distort(a, 0.88)),
      "JPEG", {"quality": 88}, region=_lens_region, geometry="perspective"),
    V("pincushion", _chain(_amount(185, 0.20, 0.45, lambda a: _radial_distort(a, 0.88)),
                           _shared(876, 0.15, 0.45)),
      "JPEG", _jit_kw(877, "quality", 30, 70), region=_lens_region, geometry="perspective"),
    V("swirl", _swirl_fn, "JPEG", {"quality": 88}, geometry="perspective",
      note="either way round"),
    V("pinch", _pinch_fn, "JPEG", {"quality": 88}, region=_pinch_reg, geometry="perspective",
      note="centre magnified, or shrunk"),
    V("wave", _wave_fn, "JPEG", {"quality": 88}, geometry="perspective",
      note="across or along, either phase"),

    # -- degradation --------------------------------------------------------
    V("noise", _chain(_shared(840, 0.12, 0.40), _noise_jit(370, 30.0, 90.0)), "JPEG",
      {"quality": 92}),
    V("blur", _blur_rel(372, 0.008, 0.024), "JPEG", {"quality": 92},
      note="radius a share of the long side"),
    V("motion_blur", _motion_blur_jit(373, 0.020, 0.070),
      "JPEG", {"quality": 90}),
    V("sharpen", _chain(_shared(852, 0.15, 0.50), _unsharp_jit(374)), "JPEG",
      _jit_kw(853, "quality", 40, 80), note="a small copy, an unsharp mask overdone"),
    V("halftone", _halftone_jit(375, 3.5, 9.0), "PNG",
      note="clustered-dot print screen"),
    V("chromatic_aberration",
      _chain(_amount(376, 0.025, 0.060, lambda a: _chromatic_aberration(a, 0.04)),
             _shared(872, 0.20, 0.50)),
      "JPEG", _jit_kw(873, "quality", 40, 80), region=(0.04, 0.04, 0.96, 0.96)),
    V("vignette", _amount(377, 0.45, 0.95, _vignette), "JPEG", {"quality": 90}),

    # -- rearrangement: traps ----------------------------------------------
    V("tile_shuffle", _tile_shuffle_jit(530), "PNG", scramble="s22",
      note="every pixel of the original, in the wrong places: 2x2 or 3x3, any order"),
    V("quadrant_rotate", _quadrant_rotate, "PNG", scramble="qrot",
      note="every quadrant turned in place; same histogram, different picture"),
    V("column_roll", _oriented(517, AXES, _amount(378, 0.22, 0.48, _column_roll))[0],
      "PNG", scramble="roll", note="slid sideways or up, and wrapped"),

    # -- alpha --------------------------------------------------------------
    V("alpha_on_white", _flatten((255, 255, 255)), "JPEG", _jit_kw(824, "quality", 40, 85),
      wants_alpha=True),
    V("alpha_on_black", _flatten((0, 0, 0)), "JPEG", _jit_kw(825, "quality", 40, 85),
      wants_alpha=True),

    # -- pipelines ----------------------------------------------------------
    V("messaging_app", _long_edge_jit(380, 96, 320), "JPEG",
      writer=_magick("-quality", "{q}", "-sampling-factor", "4:2:0", "-strip",
                     quality=(381, 5, 25)),
      note="resized and re-encoded on upload, by a different encoder"),
    V("print_scan",
      _chain(_rot_scan_fn, _shared(844, 0.15, 0.40), _blur_rel(826, 0.001, 0.004),
             _noise_jit(382, 20.0, 45.0)),
      "JPEG", _jit_kw(383, "quality", 20, 50), geometry="rotate",
      note="printed, scanned slightly askew"),
    V("photo_of_screen",
      _chain(_screen_fn, _moire_jit(520)), "JPEG", _jit_kw(384, "quality", 50, 78),
      geometry="perspective", note="a phone photograph of an LCD"),
    V("video_call_frame",
      _chain(_call_fn, _long_edge_jit(385, 120, 320, Image.BILINEAR)), "WEBP",
      _jit_kw(386, "quality", 1, 15), coverage=0.9,
      note="a shared window, recompressed for bandwidth"),
]

BY_NAME = {v.name: v for v in CATALOGUE}
assert len(BY_NAME) == len(CATALOGUE), "duplicate variant name"


# --------------------------------------------------------------------------
# transparency (`make_variants.py --alpha`)
# --------------------------------------------------------------------------
#
# What happens to a picture with a transparent background: a logo, a sticker,
# a cut-out. Only a corpus of such seeds runs these (IMGS-ALPHA); on an opaque
# seed most would be plain copies. The background a picture is flattened onto
# is the one choice a tool reading such files has to make, and the corpus must
# not reward any single choice of it, so the standard catalogue runs on each
# seed flattened onto a background drawn per seed (`alpha_background`), and
# the variants here flatten onto white, black and a colour, or keep the alpha.

def alpha_background(rgba):
    """The background a seed's standard variants are flattened onto: white,
    black, light grey or a colour, drawn from the seed's own pixels."""
    k = seed_key_of(rgba)
    return [(255, 255, 255), (0, 0, 0), (240, 240, 240),
            ((k * 37) % 200 + 30, (k * 91) % 200 + 30, (k * 53) % 200 + 30)][k % 4]


def flatten_onto(rgba, bg):
    out = Image.new("RGB", rgba.size, bg)
    out.paste(rgba, mask=rgba.split()[3])
    return out


def _rgba(im):
    return im.convert("RGBA")


def _alpha_on_colour(im):
    """Flattened onto a colour of its own, as on a slide or a coloured page."""
    rgba = im.convert("RGBA")
    k = seed_key_of(rgba)
    return flatten_onto(rgba, ((k * 13) % 180 + 40, (k * 71) % 180 + 40, (k * 29) % 180 + 40))


def _alpha_scale(salt, lo, hi):
    """Resized with its alpha, by a factor drawn per seed."""
    def go(im):
        rgba = im.convert("RGBA")
        f = _jit(seed_key_of(rgba), salt, lo, hi)
        return rgba.resize((max(1, round(rgba.size[0] * f)), max(1, round(rgba.size[1] * f))), Image.LANCZOS)
    return go


def _alpha_crop(x0, y0, x1, y1):
    return lambda im: _crop_box(im.convert("RGBA"), x0, y0, x1, y1)


def _alpha_hidden_cleared(im):
    """The colour under fully transparent pixels set to black, as an editor
    that discards it writes the file: the same picture, other bytes."""
    a = np.asarray(im.convert("RGBA")).copy()
    a[a[:, :, 3] == 0, :3] = 0
    return Image.fromarray(a, "RGBA")


def _alpha_thumb_white(im):
    rgba = im.convert("RGBA")
    t = _jit_int(seed_key_of(rgba), 903, 120, 200)
    f = t / max(rgba.size)
    small = rgba.resize((max(1, round(rgba.size[0] * f)), max(1, round(rgba.size[1] * f))), Image.LANCZOS)
    return flatten_onto(small, (255, 255, 255))


ALPHA_CATALOGUE = [
    Variant("alpha_on_colour", _alpha_on_colour, "PNG", wants_alpha=True,
            note="flattened onto a colour drawn per seed"),
    Variant("alpha_on_white_png", lambda im: flatten_onto(im.convert("RGBA"), (255, 255, 255)), "PNG",
            wants_alpha=True, note="flattened onto white, lossless"),
    Variant("alpha_thumb_white", _alpha_thumb_white, "JPEG", {"quality": 85}, wants_alpha=True,
            note="a small JPEG on white, as a web page or a chat shows it"),
    Variant("alpha_resize", _alpha_scale(900, 0.4, 0.8), "PNG", wants_alpha=True,
            note="smaller, alpha kept"),
    Variant("alpha_webp_lossy", _rgba, "WEBP", _jit_kw(901, "quality", 55, 90), wants_alpha=True,
            note="lossy WebP with alpha"),
    Variant("alpha_webp_lossless", _rgba, "WEBP", {"lossless": True}, wants_alpha=True),
    Variant("alpha_gif", _rgba, "GIF", wants_alpha=True,
            note="256 colours and one-bit transparency"),
    Variant("alpha_crop", _alpha_crop(0.1, 0.08, 0.9, 0.92), "PNG", region=(0.1, 0.08, 0.9, 0.92),
            wants_alpha=True, note="cropped, alpha kept"),
    Variant("alpha_hidden_cleared", _alpha_hidden_cleared, "PNG", wants_alpha=True,
            note="the colour under transparent pixels cleared: the same picture"),
]
