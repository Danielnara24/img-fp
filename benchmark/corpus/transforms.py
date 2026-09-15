"""The transformation catalogue for the generated half of the corpus.

Each entry is a `Variant`: a name, the function that produces the image, the
output format, and the *region* of the original that survives — expressed as a
normalised (x0, y0, x1, y1) rectangle. The region is what makes the pair-level
ground truth computable rather than guessed: two derived files that both carry
the whole original are SAME, one whose region contains the other's is CROP, and
a pair whose regions do not overlap is DIFFERENT no matter that both descend
from the same photograph.

`geometry` records a flip or rotation separately, so scoring can answer "found
it, but only because the tool is mirror-invariant" rather than folding that
into one recall number.

Nothing here is random at call time. Every parameter is fixed in the catalogue,
so regenerating the corpus produces the same bytes.
"""
from dataclasses import dataclass, field
from typing import Callable, Optional, Tuple

from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageOps

FULL = (0.0, 0.0, 1.0, 1.0)


@dataclass
class Variant:
    name: str
    fn: Callable[[Image.Image], Image.Image]
    fmt: str                          # PIL format name, or "JXL" / "RAW_COPY"
    save_kwargs: dict = field(default_factory=dict)
    region: Tuple[float, float, float, float] = FULL
    geometry: str = "none"            # none|mirror|rot90|rot180|rot270|rotate|perspective
    note: str = ""
    # Some variants are deliberately *not* duplicates of their own siblings.
    # `family` groups those that must be judged against each other by region.
    exif_orientation: Optional[int] = None


# --------------------------------------------------------------------------
# helpers


def _crop_box(im, x0, y0, x1, y1):
    w, h = im.size
    return im.crop((int(x0 * w), int(y0 * h), int(x1 * w), int(y1 * h)))


def _crop(x0, y0, x1, y1):
    return lambda im: _crop_box(im, x0, y0, x1, y1)


def _scale(factor, resample=Image.LANCZOS):
    def go(im):
        w, h = im.size
        return im.resize((max(1, int(w * factor)), max(1, int(h * factor))), resample)
    return go


def _long_edge(target):
    def go(im):
        w, h = im.size
        f = target / max(w, h)
        return im.resize((max(1, round(w * f)), max(1, round(h * f))), Image.LANCZOS)
    return go


def _enhance(cls, factor):
    return lambda im: cls(im).enhance(factor)


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


def _noise(sigma, seed):
    def go(im):
        import numpy as np
        rng = np.random.default_rng(seed)
        arr = np.asarray(im.convert("RGB"), dtype=np.int16)
        arr = arr + rng.normal(0, sigma, arr.shape).astype(np.int16)
        return Image.fromarray(arr.clip(0, 255).astype("uint8"), "RGB")
    return go


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


def _perspective_coeffs(src, dst):
    import numpy as np
    matrix = []
    for (sx, sy), (dx, dy) in zip(src, dst):
        matrix.append([dx, dy, 1, 0, 0, 0, -sx * dx, -sx * dy])
        matrix.append([0, 0, 0, dx, dy, 1, -sy * dx, -sy * dy])
    A = np.array(matrix, dtype=float)
    B = np.array(src, dtype=float).reshape(8)
    return np.linalg.solve(A, B).tolist()


def _watermark(im):
    out = im.convert("RGB").copy()
    w, h = out.size
    layer = Image.new("RGBA", out.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    size = max(12, w // 22)
    text = "(c) example"
    d.text((w - int(w * 0.32), h - int(h * 0.09)), text, fill=(255, 255, 255, 150))
    d.rectangle([w - int(w * 0.34), h - int(h * 0.11),
                 w - int(w * 0.02), h - int(h * 0.02)],
                outline=(255, 255, 255, 90), width=max(1, size // 12))
    return Image.alpha_composite(out.convert("RGBA"), layer).convert("RGB")


def _caption_bar(im):
    """Meme framing: the whole image, with a white caption strip under it."""
    w, h = im.size
    bar = max(28, h // 8)
    out = Image.new("RGB", (w, h + bar), "white")
    out.paste(im.convert("RGB"), (0, 0))
    d = ImageDraw.Draw(out)
    d.text((int(w * 0.04), h + bar // 3), "when the benchmark finally runs", fill="black")
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


def _chain(*fns):
    def go(im):
        for fn in fns:
            im = fn(im)
        return im
    return go


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


# --------------------------------------------------------------------------
# the catalogue

_sq_fn, _sq_region = _centre_aspect(1.0)
_wide_fn, _wide_region = _centre_aspect(16 / 9)
_tall_fn, _tall_region = _centre_aspect(9 / 16)
_rot3_fn, _rot3_region = _rotate_inscribed(3.0)
_rot15_fn, _rot15_region = _rotate_inscribed(1.5)

# Variants whose region depends on the source's aspect ratio are resolved per
# image by make_variants.py; it looks the callable up here.
DYNAMIC_REGIONS = {
    "crop_square": _sq_region,
    "crop_16_9": _wide_region,
    "crop_9_16": _tall_region,
    "instagram": _sq_region,
    "rotate_3deg": _rot3_region,
    "print_scan": _rot15_region,
}

CATALOGUE = [
    # -- container and metadata, pixels untouched ---------------------------
    Variant("copy_exact", None, "RAW_COPY",
            note="byte-identical copy under another name; the floor every tool should find"),
    Variant("strip_metadata", None, "EXIF_STRIP",
            note="same compressed stream, EXIF removed: different bytes, identical pixels"),
    Variant("to_png", lambda im: im.convert("RGB"), "PNG",
            note="lossless container change"),
    Variant("webp_lossless", lambda im: im.convert("RGB"), "WEBP", {"lossless": True}),
    Variant("to_tiff", lambda im: im.convert("RGB"), "TIFF"),

    # -- lossy re-encode ----------------------------------------------------
    Variant("jpeg_q90", lambda im: im.convert("RGB"), "JPEG", {"quality": 90}),
    Variant("jpeg_q75", lambda im: im.convert("RGB"), "JPEG", {"quality": 75}),
    Variant("jpeg_q50", lambda im: im.convert("RGB"), "JPEG", {"quality": 50}),
    Variant("jpeg_q30", lambda im: im.convert("RGB"), "JPEG", {"quality": 30}),
    Variant("jpeg_q15", lambda im: im.convert("RGB"), "JPEG", {"quality": 15}),
    Variant("jpeg_q08", lambda im: im.convert("RGB"), "JPEG", {"quality": 8},
            note="severe blocking; where perceptual hashes start to disagree"),
    Variant("jpeg_444", lambda im: im.convert("RGB"), "JPEG",
            {"quality": 90, "subsampling": 0}, note="4:4:4 chroma instead of 4:2:0"),
    Variant("jpeg_progressive", lambda im: im.convert("RGB"), "JPEG",
            {"quality": 85, "progressive": True}),
    Variant("jpeg_gen3", _jpeg_roundtrip(85, 3), "JPEG", {"quality": 85},
            note="three generations of re-encoding"),
    Variant("to_webp_q80", lambda im: im.convert("RGB"), "WEBP", {"quality": 80}),
    Variant("to_webp_q40", lambda im: im.convert("RGB"), "WEBP", {"quality": 40}),
    Variant("to_avif_q50", lambda im: im.convert("RGB"), "AVIF", {"quality": 50}),
    Variant("to_heif_q60", lambda im: im.convert("RGB"), "HEIF", {"quality": 60},
            note="the iPhone format; several tools cannot read it at all"),
    Variant("to_jxl_q85", lambda im: im.convert("RGB"), "JXL", {"quality": 85}),
    Variant("to_gif", lambda im: im.convert("RGB"), "GIF",
            note="256-colour quantisation"),

    # -- resolution ---------------------------------------------------------
    Variant("scale_75", _scale(0.75), "JPEG", {"quality": 92}),
    Variant("scale_50", _scale(0.50), "JPEG", {"quality": 92}),
    Variant("scale_25", _scale(0.25), "JPEG", {"quality": 92}),
    Variant("scale_12", _scale(0.125), "JPEG", {"quality": 92}),
    Variant("thumb_320", _long_edge(320), "JPEG", {"quality": 85}),
    Variant("thumb_128", _long_edge(128), "JPEG", {"quality": 85},
            note="contact-sheet thumbnail; the hardest honest positive here"),
    Variant("upscale_150", _scale(1.5), "JPEG", {"quality": 90}),
    Variant("scale_nearest_50", _scale(0.5, Image.NEAREST), "PNG",
            note="aliased downscale, no filtering"),
    Variant("squash_aspect", lambda im: im.resize(
        (int(im.size[0] * 0.8), im.size[1]), Image.LANCZOS), "JPEG", {"quality": 90},
        note="non-uniform scale: aspect ratio changes"),

    # -- crops --------------------------------------------------------------
    Variant("crop_center_90", _crop(.05, .05, .95, .95), "JPEG", {"quality": 92},
            region=(.05, .05, .95, .95)),
    Variant("crop_center_75", _crop(.125, .125, .875, .875), "JPEG", {"quality": 92},
            region=(.125, .125, .875, .875)),
    Variant("crop_center_50", _crop(.25, .25, .75, .75), "JPEG", {"quality": 92},
            region=(.25, .25, .75, .75)),
    Variant("crop_center_25", _crop(.375, .375, .625, .625), "JPEG", {"quality": 92},
            region=(.375, .375, .625, .625),
            note="6% of the original's area survives"),
    Variant("crop_square", _sq_fn, "JPEG", {"quality": 92}),
    Variant("crop_16_9", _wide_fn, "JPEG", {"quality": 92}),
    Variant("crop_9_16", _tall_fn, "JPEG", {"quality": 92},
            note="vertical 'story' recrop"),
    Variant("crop_50_upscaled", _chain(_crop(.25, .25, .75, .75), _scale(2.0)),
            "JPEG", {"quality": 88}, region=(.25, .25, .75, .75),
            note="crop then resize back: same pixel count, half the content"),

    # Two disjoint quadrants. Each is a CROP of the original; against *each
    # other* they share nothing, so a tool that links them is wrong. This is
    # the trap that separates containment from 'descended from the same file'.
    Variant("quadrant_tl", _crop(0, 0, .45, .45), "JPEG", {"quality": 92},
            region=(0, 0, .45, .45), note="disjoint from quadrant_br"),
    Variant("quadrant_br", _crop(.55, .55, 1, 1), "JPEG", {"quality": 92},
            region=(.55, .55, 1, 1), note="disjoint from quadrant_tl"),

    # -- additive framing: all content survives -----------------------------
    Variant("watermark", _watermark, "JPEG", {"quality": 90}),
    Variant("caption_bar", _caption_bar, "JPEG", {"quality": 88},
            note="meme framing: full image plus a caption strip"),
    Variant("letterbox_16_9", _letterbox(16 / 9), "JPEG", {"quality": 90},
            note="padded with bars, not cropped"),
    Variant("screenshot_chrome", _screenshot_chrome, "PNG",
            note="the image inside a window frame"),

    # -- colour and tone ----------------------------------------------------
    Variant("bright_up", _enhance(ImageEnhance.Brightness, 1.25), "JPEG", {"quality": 92}),
    Variant("bright_down", _enhance(ImageEnhance.Brightness, 0.75), "JPEG", {"quality": 92}),
    Variant("contrast_up", _enhance(ImageEnhance.Contrast, 1.4), "JPEG", {"quality": 92}),
    Variant("contrast_down", _enhance(ImageEnhance.Contrast, 0.65), "JPEG", {"quality": 92}),
    Variant("greyscale", lambda im: ImageOps.grayscale(im).convert("RGB"),
            "JPEG", {"quality": 92}),
    Variant("saturation_up", _enhance(ImageEnhance.Color, 1.8), "JPEG", {"quality": 92}),
    Variant("warm_shift", _channel_shift(1.12, 1.0, 0.88), "JPEG", {"quality": 92}),
    Variant("cool_shift", _channel_shift(0.88, 1.0, 1.15), "JPEG", {"quality": 92}),
    Variant("gamma_up", _gamma(0.7), "JPEG", {"quality": 92}),
    Variant("sepia", _sepia, "JPEG", {"quality": 92}),
    Variant("autocontrast", lambda im: ImageOps.autocontrast(im.convert("RGB")),
            "JPEG", {"quality": 92}),
    Variant("posterize", lambda im: ImageOps.posterize(im.convert("RGB"), 4),
            "PNG", note="4 bits per channel"),

    # -- geometry -----------------------------------------------------------
    Variant("flip_h", lambda im: ImageOps.mirror(im), "JPEG", {"quality": 92},
            geometry="mirror",
            note="only found by a tool that looks for mirrors on purpose"),
    Variant("rot90", lambda im: im.rotate(-90, expand=True), "JPEG", {"quality": 92},
            geometry="rot90"),
    Variant("rot180", lambda im: im.rotate(180, expand=True), "JPEG", {"quality": 92},
            geometry="rot180"),
    Variant("exif_rot90", lambda im: im.convert("RGB"), "JPEG",
            {"quality": 92}, exif_orientation=6,
            note="pixels unrotated, EXIF says rotate: a viewer shows it turned"),
    Variant("rotate_3deg", _rot3_fn, "JPEG", {"quality": 90}, geometry="rotate",
            note="off-axis by 3 degrees, cropped back to a rectangle"),
    Variant("perspective", _perspective(0.06), "JPEG", {"quality": 90},
            geometry="perspective", region=(0.06, 0.0, 0.94, 1.0)),

    # -- degradation --------------------------------------------------------
    Variant("noise", _noise(9.0, 1), "JPEG", {"quality": 92}),
    Variant("noise_heavy", _noise(22.0, 2), "JPEG", {"quality": 88}),
    Variant("blur", lambda im: im.filter(ImageFilter.GaussianBlur(1.8)),
            "JPEG", {"quality": 92}),
    Variant("sharpen", lambda im: im.filter(ImageFilter.UnsharpMask(3, 180, 3)),
            "JPEG", {"quality": 92}),

    # -- composite chains that actually happen ------------------------------
    Variant("whatsapp", _chain(_long_edge(1600), _jpeg_roundtrip(80)), "JPEG",
            {"quality": 80}, note="messaging-app resize and re-encode"),
    Variant("instagram", _chain(_sq_fn, _long_edge(1080),
                                _enhance(ImageEnhance.Color, 1.15)),
            "JPEG", {"quality": 85}, note="square crop, 1080px, a little punchier"),
    Variant("photo_of_screen",
            _chain(_perspective(0.05),
                   lambda im: im.filter(ImageFilter.GaussianBlur(0.9)),
                   _noise(7.0, 3), _channel_shift(1.05, 1.0, 0.95)),
            "JPEG", {"quality": 72}, geometry="perspective",
            region=(0.05, 0.0, 0.95, 1.0),
            note="a photo taken of a screen"),
    Variant("print_scan",
            _chain(_rot15_fn, _noise(6.0, 4),
                   lambda im: im.filter(ImageFilter.GaussianBlur(0.7)),
                   _channel_shift(1.06, 1.0, 0.93)),
            "JPEG", {"quality": 84}, geometry="rotate",
            note="printed, then scanned slightly askew"),
    Variant("heavy_chain",
            _chain(_crop(.125, .125, .875, .875), _scale(0.5),
                   lambda im: ImageOps.grayscale(im).convert("RGB"),
                   _jpeg_roundtrip(40)),
            "JPEG", {"quality": 40}, region=(.125, .125, .875, .875),
            note="crop, halve, desaturate, crush: everything at once"),
]

BY_NAME = {v.name: v for v in CATALOGUE}
assert len(BY_NAME) == len(CATALOGUE), "duplicate variant name"
