"""The third catalogue (IMGS-ALL3): other transformations of the same seeds.

    python3 make_variants.py --catalogue third --corpus /home/daniel/Documents/IMGS-v3/IMGS

None of these is in `transforms.CATALOGUE`, in either generation: IMGS-ALL and
IMGS-ALL2 were what every bar and rule was measured on, so a third corpus of
the same kinds would test nothing new. Sixty transformations of kinds the
first two do not hold (other containers, other hosts, other occlusions, tone
and colour edits, warps, degradations, three rearrangement traps, four
pipelines), and thirty **chains of two** of them, each of two different kinds
(a crop then a grade, a rotation onto a canvas after a sticker, a letterboxed
video frame).

The amounts were chosen once, from what each transformation is in use, and
**never tuned against a tool's result**: IMGS-ALL2's were strengthened in five
rounds until img-fp no longer found every seed, which makes it a corpus img-fp
was looked at on. This one is not to be adjusted after a run.

Each transformation is a `Step` made by a factory taking a salt: its function,
the region of what it was handed that survives, the coverage and geometry.
A chain draws its two steps from salts of their own, so a seed's crop in
`crop_corner+fade_matte` is not its crop in `crop_corner`. The ground truth
comes from the steps (`_variant`): regions compose, coverages multiply.
A step that leaves less than the whole frame goes first in a chain, or after
a step that changes neither the frame nor its shape; a host goes after a crop,
never before one.

Salts: 100,000 + 5,000 a step (`_salt`), clear of both earlier generations'
(under 3,000) and of the offsets the shared factories add (up to 2,000).
"""
import math
import os
import subprocess
import tempfile
from dataclasses import dataclass
from typing import Callable

import numpy as np
from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageFont, ImageOps

import transforms as T
from transforms import (AXES, FULL, Variant, _HOST_PIXELS, _PIXEL_PIXELS,
                        _compose, _fit, _jit as J, _jit_int as JI, _jit_kw,
                        _keyed, _oriented, _pick as PK, seed_key_of)


@dataclass
class Step:
    fn: Callable
    region: object = None         # None: the whole frame; a rect; (w, h, key) -> rect
    coverage: object = 1.0        # a number, or key -> number
    geometry: str = "none"
    scramble: str = ""


_FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
_CORNERS = ((False, False), (True, False), (False, True), (True, True))


def _font(px):
    return ImageFont.truetype(_FONT, max(8, int(px)))


def _rgb(im):
    return im if im.mode == "RGB" else im.convert("RGB")


def _arr(im, budget=_PIXEL_PIXELS):
    return np.asarray(_fit(_rgb(im), budget), dtype=np.float32)


def _img(a):
    return Image.fromarray(np.clip(a, 0, 255).astype("uint8"), "RGB")


def _colour(k, s, lo=0, hi=255):
    return tuple(JI(k, s + i, lo, hi) for i in range(3))


def _rng(k, s):
    return np.random.default_rng(1_000_003 * int(k) + int(s))


# --------------------------------------------------------------------------
# containers
# --------------------------------------------------------------------------


def _palette_fs(im, colours):
    """An adaptive palette of `colours`, error-diffused (Pillow's `quantize`
    alone does not dither: `posterize` in the first catalogue is flat)."""
    im = _rgb(im)
    pal = im.quantize(colours, method=Image.Quantize.MEDIANCUT)
    return im.quantize(palette=pal, dither=Image.Dither.FLOYDSTEINBERG)


def to_gif(s):
    return Step(lambda im: _palette_fs(_fit(_rgb(im), _PIXEL_PIXELS), 256))


_EXIF_STORE = {3: Image.ROTATE_180, 6: Image.ROTATE_90, 8: Image.ROTATE_270}


def _exif_which(key, s):
    return PK(key, s, (3, 6, 8))


def exif_turned(s):
    """Stored turned, with the EXIF orientation that shows it upright: what a
    phone held sideways writes. As shown it is the seed."""
    def go(im):
        return _rgb(im).transpose(_EXIF_STORE[_exif_which(seed_key_of(im), s)])
    return Step(go, geometry="exif")


def _exif_kw(s, q_lo, q_hi):
    def make(key):
        exif = Image.Exif()
        exif[0x0112] = _exif_which(key, s)
        return {"quality": JI(key, s + 1, q_lo, q_hi), "exif": exif}
    return make


# --------------------------------------------------------------------------
# resolution and crops
# --------------------------------------------------------------------------


def pixelate(s):
    """A mosaic of the whole frame: 60 to 160 cells along the long side."""
    def go(im):
        im = _fit(_rgb(im), _PIXEL_PIXELS)
        w, h = im.size
        f = JI(seed_key_of(im), s, 60, 160) / max(w, h)
        small = im.resize((max(1, round(w * f)), max(1, round(h * f))), Image.BOX)
        return small.resize((w, h), Image.NEAREST)
    return Step(go)


def crop_corner(s):
    """One corner of the picture, 45-75% a side."""
    def rect(key):
        fw, fh = J(key, s, 0.45, 0.75), J(key, s + 1, 0.45, 0.75)
        left, top = PK(key, s + 2, _CORNERS)
        x0, y0 = (0.0 if left else 1.0 - fw), (0.0 if top else 1.0 - fh)
        return (x0, y0, x0 + fw, y0 + fh)
    return Step(lambda im: T._crop_box(_rgb(im), *rect(seed_key_of(im))),
                lambda w, h, key: rect(key))


def crop_band(s):
    """A band across the inside of the picture, a fifth to a third of it,
    across or down."""
    def rect(key):
        f = J(key, s, 0.18, 0.35)
        y0 = J(key, s + 1, 0.10, 0.90 - f)
        return (0.0, y0, 1.0, y0 + f)
    fn, where = _oriented(s + 5, AXES,
                          lambda im: T._crop_box(_rgb(im), *rect(seed_key_of(im))),
                          lambda w, h, key: rect(key))
    return Step(fn, where)


def digital_zoom(s):
    """Zoomed in on a phone: a part of the frame enlarged back to its size."""
    crop, reg = T._zoom_free(s, 0.35, 0.60)

    def go(im):
        im = _rgb(im)
        w, h = im.size
        if w * h > 8_000_000:
            f = (8_000_000 / (w * h)) ** 0.5
            w, h = round(w * f), round(h * f)
        return crop(im).resize((w, h), Image.BICUBIC)
    return Step(go, reg)


def crop_circle(s):
    """An avatar: a square of the picture anywhere, shown through a circle."""
    def rect(w, h, key):
        side = J(key, s, 0.60, 1.0) * min(w, h)
        x0, y0 = J(key, s + 1, 0.0, 1.0) * (w - side), J(key, s + 2, 0.0, 1.0) * (h - side)
        return (x0 / w, y0 / h, (x0 + side) / w, (y0 + side) / h)

    def go(im):
        k = seed_key_of(im)
        im = _rgb(im)
        sq = _fit(T._crop_box(im, *rect(im.size[0], im.size[1], k)))
        bg = PK(k, s + 3, ((255, 255, 255), (0, 0, 0), (240, 240, 240), _colour(k, s + 4, 30, 220)))
        mask = Image.new("L", sq.size, 0)
        ImageDraw.Draw(mask).ellipse([0, 0, sq.size[0] - 1, sq.size[1] - 1], fill=255)
        return Image.composite(sq, Image.new("RGB", sq.size, bg), mask)
    return Step(go, rect, coverage=math.pi / 4)


# --------------------------------------------------------------------------
# hosts
# --------------------------------------------------------------------------


def letterbox(s):
    """Bars along one pair of sides, 25-90% of that side in all."""
    def share(key):
        return J(key, s, 0.25, 0.90)

    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        bg = PK(k, s + 3, ((0, 0, 0), (0, 0, 0), (255, 255, 255), (16, 16, 18),
                           _colour(k, s + 4, 20, 230)))
        t = J(k, s + 2, 0.3, 0.7)
        if PK(k, s + 1, (True, False)):
            add = max(2, int(h * share(k)))
            out = Image.new("RGB", (w, h + add), bg)
            out.paste(im, (0, int(add * t)))
        else:
            add = max(2, int(w * share(k)))
            out = Image.new("RGB", (w + add, h), bg)
            out.paste(im, (int(add * t), 0))
        return out
    return Step(go, coverage=lambda key: 1.0 / (1.0 + share(key)))


def rotate_canvas(s):
    """Turned by any angle and nothing cut off: the canvas grows to hold it.
    10-80 or 100-170 degrees, either way (`rotate_large` cuts to the inscribed
    rectangle and stops at 45)."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        a = T._sign(k, s + 1) * (J(k, s, 10.0, 80.0) + PK(k, s + 2, (0.0, 90.0)))
        bg = PK(k, s + 3, ((255, 255, 255), (0, 0, 0), _colour(k, s + 4, 20, 235)))
        return im.rotate(a, resample=Image.BICUBIC, expand=True, fillcolor=bg)
    return Step(go, coverage=0.6, geometry="rotate")


def tiled_repeat(s):
    """The picture repeated two or three times each way, as a wallpaper."""
    def n(key):
        return JI(key, s, 2, 3)

    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        g = n(k)
        small = im.resize((max(1, im.size[0] // g), max(1, im.size[1] // g)), Image.LANCZOS)
        out = Image.new("RGB", (small.size[0] * g, small.size[1] * g))
        for y in range(g):
            for x in range(g):
                out.paste(small, (x * small.size[0], y * small.size[1]))
        return out
    return Step(go, coverage=lambda key: 1.0 / n(key) ** 2)


def mirror_extend(s):
    """The canvas extended past every edge by the picture's own reflection."""
    def pads(key):
        return [J(key, s + i, 0.12, 0.40) for i in range(4)]

    def go(im):
        a = np.asarray(_fit(_rgb(im)))
        h, w = a.shape[:2]
        l, r, t, b = pads(seed_key_of(im))
        return Image.fromarray(np.pad(a, ((int(t * h), int(b * h)), (int(l * w), int(r * w)),
                                          (0, 0)), mode="reflect"), "RGB")

    def cov(key):
        l, r, t, b = pads(key)
        return 1.0 / ((1 + l + r) * (1 + t + b))
    return Step(go, coverage=cov)


def before_after(s):
    """Beside an edited copy of itself, as a before / after post."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        edit = PK(k, s, (
            lambda x: ImageOps.grayscale(x).convert("RGB"),
            lambda x: ImageEnhance.Color(ImageEnhance.Contrast(x).enhance(1.6)).enhance(1.8),
            lambda x: ImageEnhance.Brightness(x).enhance(1.7),
            lambda x: x.filter(ImageFilter.GaussianBlur(max(2, max(w, h) // 90)))))(im)
        gap = max(2, int(min(w, h) * J(k, s + 1, 0.01, 0.03)))
        bg = PK(k, s + 2, ((255, 255, 255), (0, 0, 0)))
        first = PK(k, s + 3, (True, False))
        a, b = (im, edit) if first else (edit, im)
        if PK(k, s + 4, (True, False)):
            out = Image.new("RGB", (2 * w + gap, h), bg)
            out.paste(a, (0, 0)); out.paste(b, (w + gap, 0))
        else:
            out = Image.new("RGB", (w, 2 * h + gap), bg)
            out.paste(a, (0, 0)); out.paste(b, (0, h + gap))
        return out
    return Step(go, coverage=0.5)


_WORDS = ["harbour", "tuesday", "ticket", "window", "seven", "garden", "maybe",
          "later", "north", "paper", "station", "quiet", "orange", "second",
          "bridge", "almost", "letter", "copper", "winter", "signal"]


def _line(k, s, n):
    return " ".join(_WORDS[JI(k, s + i, 0, len(_WORDS) - 1)] for i in range(n))


def chat_bubble(s):
    """A screenshot of a conversation the picture was sent in."""
    of, cov = T._scales(s, 1.35, 1.90, 1.60, 2.40)

    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        sx, sy = of(k)
        cw, ch = int(w * sx), int(h * sy)
        dark = PK(k, s + 4, (True, False))
        bg = (18, 20, 24) if dark else (236, 229, 221)
        mine, theirs = _colour(k, s + 10, 40, 200), ((44, 48, 56) if dark else (255, 255, 255))
        ink = (235, 235, 235) if dark else (30, 30, 30)
        out = Image.new("RGB", (cw, ch), bg)
        d = ImageDraw.Draw(out)
        head, foot, pad = ch // 14, ch // 12, max(4, cw // 40)
        d.rectangle([0, 0, cw, head], fill=_colour(k, s + 20, 20, 120))
        d.text((pad * 2, head // 4), _line(k, s + 30, 2), fill=(255, 255, 255), font=_font(head * 0.45))
        d.rectangle([0, ch - foot, cw, ch], fill=theirs)
        incoming = PK(k, s + 5, (True, False))
        y0 = head + pad + int((ch - head - foot - h - 2 * pad) * J(k, s + 6, 0.15, 0.85))
        x0 = pad * 2 if incoming else cw - w - pad * 2
        out.paste(im, (x0, y0))
        fs = max(10, ch // 45)
        font = _font(fs)

        def bubbles(top, bottom, salt):
            y, i = top, 0
            while y + fs * 2.4 < bottom and i < 12:
                me = PK(k, salt + 3 * i, (True, False))
                bw = int(cw * J(k, salt + 3 * i + 1, 0.25, 0.7))
                x = cw - bw - pad * 2 if me else pad * 2
                d.rounded_rectangle([x, y, x + bw, y + int(fs * 2.2)], radius=fs // 2,
                                    fill=mine if me else theirs)
                d.text((x + fs // 2, y + fs // 2), _line(k, salt + 100 + 5 * i, 6),
                       fill=(255, 255, 255) if me else ink, font=font)
                d.rectangle([x + bw - fs // 4, y, x + bw + 2, y + int(fs * 2.2)],
                            fill=mine if me else theirs)
                y += int(fs * 3.0)
                i += 1
        bubbles(head + pad, y0 - pad, s + 200)
        bubbles(y0 + h + pad, ch - foot - pad, s + 400)
        return out
    return Step(go, coverage=cov)


def social_post(s):
    """A post: avatar and handle above, caption and actions below."""
    def shares(key):
        return J(key, s, 0.10, 0.20), J(key, s + 1, 0.10, 0.30)

    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        hf, ff = shares(k)
        head, foot = max(8, int(h * hf)), max(8, int(h * ff))
        dark = PK(k, s + 2, (True, False))
        bg, ink = ((0, 0, 0), (240, 240, 240)) if dark else ((255, 255, 255), (20, 20, 25))
        out = Image.new("RGB", (w, h + head + foot), bg)
        d = ImageDraw.Draw(out)
        pad = head // 5
        d.ellipse([pad, pad, head - pad, head - pad], fill=_colour(k, s + 10, 60, 230))
        d.text((head, head // 3), "@" + _line(k, s + 20, 1) + str(k % 97), fill=ink,
               font=_font(head * 0.3))
        out.paste(im, (0, head))
        y = h + head + foot // 8
        r = max(4, foot // 5)
        for i in range(JI(k, s + 3, 3, 4)):
            x = pad + i * (w // (6 + k % 4))
            d.ellipse([x, y, x + r, y + r], outline=ink, width=max(1, r // 6))
        font = _font(max(9, foot * 0.16))
        for i in range(JI(k, s + 4, 1, 3)):
            d.text((pad, y + r + pad // 2 + i * int(foot * 0.2)), _line(k, s + 40 + 9 * i, 7),
                   fill=ink, font=font)
        return out

    def cov(key):
        hf, ff = shares(key)
        return 1.0 / (1.0 + hf + ff)
    return Step(go, coverage=cov)


def desktop_window(s):
    """Open in a viewer's window on a desktop, screenshotted whole."""
    of, cov = T._scales(s, 1.25, 1.80, 1.30, 1.90)

    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        sx, sy = of(k)
        cw, ch = int(w * sx), int(h * sy)
        c0 = np.array(_colour(k, s + 10, 10, 200), dtype=np.float32)
        c1 = np.array(_colour(k, s + 20, 10, 200), dtype=np.float32)
        t = (np.linspace(0, 1, ch)[:, None] * J(k, s + 4, 0.3, 1.0)
             + np.linspace(0, 1, cw)[None, :] * J(k, s + 5, 0.0, 0.7))[:, :, None]
        out = _img(c0 + (c1 - c0) * np.clip(t, 0, 1))
        d = ImageDraw.Draw(out)
        task, tb, b = ch // 24, max(8, ch // 28), max(2, w // 200)
        d.rectangle([0, ch - task, cw, ch], fill=(24, 26, 30))
        for i in range(JI(k, s + 6, 3, 8)):
            x = task // 4 + i * int(task * 1.3)
            d.rounded_rectangle([x, ch - task + task // 6, x + task * 2 // 3, ch - task // 6],
                                radius=task // 8, fill=_colour(k, s + 30 + 3 * i, 60, 240))
        ww, wh = w + 2 * b, h + tb + b
        x0 = int((cw - ww) * J(k, s + 2, 0.0, 1.0))
        y0 = int((ch - task - wh) * J(k, s + 3, 0.0, 1.0))
        d.rectangle([x0, y0, x0 + ww, y0 + wh], fill=_colour(k, s + 60, 30, 235))
        d.text((x0 + tb, y0 + tb // 5), _line(k, s + 70, 2) + ".jpg",
               fill=(128, 128, 128), font=_font(tb * 0.55))
        for i, c in enumerate(((255, 95, 86), (255, 189, 46), (39, 201, 63))):
            cx = x0 + ww - (i + 1) * tb
            d.ellipse([cx, y0 + tb // 4, cx + tb // 2, y0 + tb * 3 // 4], fill=c)
        out.paste(im, (x0 + b, y0 + tb))
        return out
    return Step(go, coverage=cov)


def triptych(s):
    """Cut into two to four panels with gaps between: a printed set."""
    def of(key):
        return JI(key, s, 2, 4), J(key, s + 1, 0.03, 0.08)

    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        n, g = of(k)
        gap = max(2, int(w * g))
        bg = PK(k, s + 2, ((255, 255, 255), (0, 0, 0), _colour(k, s + 3, 30, 230)))
        out = Image.new("RGB", (w + (n - 1) * gap, h), bg)
        for i in range(n):
            x0, x1 = w * i // n, w * (i + 1) // n
            out.paste(im.crop((x0, 0, x1, h)), (x0 + i * gap, 0))
        return out
    fn, _ = _oriented(s + 5, AXES, go)
    return Step(fn, coverage=lambda key: 1.0 / (1.0 + (of(key)[0] - 1) * of(key)[1]))


# --------------------------------------------------------------------------
# occlusion
# --------------------------------------------------------------------------


def emoji_stickers(s):
    """Two to five opaque faces stuck on, where and how big per seed."""
    def go(im):
        k = seed_key_of(im)
        out = _rgb(im).copy()
        w, h = out.size
        d = ImageDraw.Draw(out)
        for i in range(JI(k, s, 2, 5)):
            t = s + 10 + 10 * i
            r = int(min(w, h) * J(k, t, 0.07, 0.14))
            x, y = int(w * J(k, t + 1, 0.05, 0.95)), int(h * J(k, t + 2, 0.05, 0.95))
            d.ellipse([x - r, y - r, x + r, y + r], fill=_colour(k, t + 3, 90, 255),
                      outline=(20, 20, 20), width=max(1, r // 12))
            e = max(1, r // 6)
            for sx in (-1, 1):
                d.ellipse([x + sx * r // 3 - e, y - r // 3 - e, x + sx * r // 3 + e, y - r // 3 + e],
                          fill=(25, 25, 25))
            a0 = PK(k, t + 6, ((20, 160), (200, 340)))
            d.arc([x - r // 2, y - r // 4, x + r // 2, y + r * 2 // 3], a0[0], a0[1],
                  fill=(25, 25, 25), width=max(2, r // 8))
        return out
    return Step(go)


_TOP = ["WHEN THE BUS", "NOBODY ASKED", "ONE DOES NOT", "ME AT SEVEN", "EVERY MONDAY",
        "THAT MOMENT", "STILL WAITING", "THEY SAID", "NOT AGAIN", "MEANWHILE"]
_BOTTOM = ["FINALLY ARRIVES", "BUT HERE WE ARE", "SIMPLY WALK AWAY", "IS NOT READY",
           "LOOKS LIKE THIS", "IT WAS FRIDAY", "FOR THE FERRY", "IT WOULD BE EASY",
           "AND YET", "IN THE GARDEN"]


def meme_text(s):
    """Bold outlined text over the top and bottom of the picture itself."""
    def go(im):
        k = seed_key_of(im)
        out = _fit(_rgb(im), _PIXEL_PIXELS).copy()
        w, h = out.size
        d = ImageDraw.Draw(out)
        for text, top, t in ((PK(k, s, _TOP), True, s + 2), (PK(k, s + 1, _BOTTOM), False, s + 3)):
            size = max(10, int(h * J(k, t, 0.07, 0.13)))
            font = _font(size)
            while size > 10 and d.textlength(text, font=font) > 0.94 * w:
                size = int(size * 0.9)
                font = _font(size)
            x = (w - d.textlength(text, font=font)) / 2
            y = h * 0.03 if top else h * 0.97 - size * 1.15
            d.text((x, y), text, font=font, fill=(255, 255, 255),
                   stroke_width=max(1, size // 12), stroke_fill=(0, 0, 0))
        return out
    return Step(go)


def broadcast_overlay(s):
    """A channel's bug, a clock and a lower-third banner, as on a news frame."""
    def go(im):
        k = seed_key_of(im)
        out = _fit(_rgb(im), _PIXEL_PIXELS).copy()
        w, h = out.size
        d = ImageDraw.Draw(out)
        m = min(w, h)
        left = PK(k, s, (True, False))
        b = int(m * J(k, s + 1, 0.08, 0.13))
        x = w // 30 if left else w - w // 30 - b
        d.rounded_rectangle([x, h // 30, x + b, h // 30 + b], radius=b // 5,
                            fill=_colour(k, s + 10, 30, 230))
        d.text((x + b // 4, h // 30 + b // 5), "ABCDEFGHKMNRSTXZ"[k % 16],
               fill=(255, 255, 255), font=_font(b * 0.6))
        d.text((w - w // 30 - b * 2 if left else w // 30, h // 30),
               "%02d:%02d" % (k % 24, (k * 7) % 60), fill=(255, 255, 255), font=_font(b * 0.4),
               stroke_width=max(1, b // 30), stroke_fill=(0, 0, 0))
        bh = int(h * J(k, s + 2, 0.10, 0.16))
        by = int(h * J(k, s + 3, 0.66, 0.80))
        bw = int(w * J(k, s + 4, 0.55, 0.90))
        bx = 0 if PK(k, s + 5, (True, False)) else w - bw
        d.rectangle([bx, by, bx + bw, by + bh], fill=_colour(k, s + 20, 10, 200))
        d.rectangle([bx, by, bx + bw, by + bh // 8], fill=_colour(k, s + 30, 120, 255))
        d.text((bx + bh // 3, by + bh // 4), _line(k, s + 40, 3).upper(), fill=(255, 255, 255),
               font=_font(bh * 0.42))
        if PK(k, s + 6, (True, False)):
            d.rectangle([0, h - h // 18, w, h], fill=(10, 10, 12))
            d.text((w // 40, h - h // 18 + h // 90), _line(k, s + 60, 9), fill=(240, 220, 60),
                   font=_font(h // 30))
        return out
    return Step(go)


def scribble(s):
    """Three to seven thick freehand strokes drawn over the picture."""
    def go(im):
        k = seed_key_of(im)
        out = _rgb(im).copy()
        w, h = out.size
        m = min(w, h)
        d = ImageDraw.Draw(out)
        rng = _rng(k, s)
        for i in range(JI(k, s, 3, 7)):
            x, y = rng.uniform(0.1, 0.9) * w, rng.uniform(0.1, 0.9) * h
            a = rng.uniform(0, 2 * math.pi)
            pts = [(x, y)]
            for _ in range(int(rng.integers(12, 30))):
                a += rng.normal(0, 0.6)
                step = rng.uniform(0.04, 0.10) * m
                x = min(max(x + step * math.cos(a), 0), w)
                y = min(max(y + step * math.sin(a), 0), h)
                pts.append((x, y))
            colour = PK(k, s + 10 + i, ((230, 30, 30), (30, 60, 230), (10, 10, 10),
                                         (250, 220, 30), (40, 180, 70), (255, 255, 255)))
            d.line(pts, fill=colour, width=max(2, int(m * rng.uniform(0.008, 0.02))),
                   joint="curve")
        return out
    return Step(go)


def mosaic_patch(s):
    """A part of the picture censored by pixelation (`blur_patch` blurs)."""
    def go(im):
        k = seed_key_of(im)
        out = _rgb(im).copy()
        w, h = out.size
        fw, fh = J(k, s, 0.30, 0.60), J(k, s + 1, 0.30, 0.60)
        x0, y0 = J(k, s + 2, 0.0, 1.0 - fw), J(k, s + 3, 0.0, 1.0 - fh)
        box = (int(x0 * w), int(y0 * h), int((x0 + fw) * w), int((y0 + fh) * h))
        cell = max(2, int(min(w, h) * J(k, s + 4, 0.025, 0.06)))
        patch = out.crop(box)
        small = patch.resize((max(1, patch.size[0] // cell), max(1, patch.size[1] // cell)),
                             Image.BOX)
        out.paste(small.resize(patch.size, Image.NEAREST), box)
        return out
    return Step(go)


def _ramp(h, w, side, extent):
    """1 at one side of the frame falling to 0 `extent` of the way across."""
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    t = {"l": xx / w, "r": 1 - xx / w, "t": yy / h, "b": 1 - yy / h}[side]
    return np.clip(1.0 - t / extent, 0.0, 1.0)


def light_leak(s):
    """A film camera's light leak: a warm wash from one edge."""
    def go(im):
        k = seed_key_of(im)
        a = _arr(im)
        h, w = a.shape[:2]
        ramp = _ramp(h, w, PK(k, s, "lrtb"), J(k, s + 1, 0.35, 0.75)) ** 1.5
        tilt = _ramp(h, w, PK(k, s + 2, "lrtb"), 2.5)
        colour = np.array(PK(k, s + 3, ((255, 140, 30), (255, 60, 40), (255, 80, 200),
                                        (255, 220, 90))), dtype=np.float32)
        leak = (ramp * (0.5 + 0.5 * tilt) * J(k, s + 4, 0.5, 0.95))[:, :, None] * colour
        return _img(255.0 - (255.0 - a) * (1.0 - leak / 255.0))
    return Step(go)


def glare_spot(s):
    """A flash or a window reflected in the print: a blown-out spot and haze."""
    def go(im):
        k = seed_key_of(im)
        a = _arr(im)
        h, w = a.shape[:2]
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        cx, cy = w * J(k, s, 0.15, 0.85), h * J(k, s + 1, 0.15, 0.85)
        r = min(w, h) * J(k, s + 2, 0.14, 0.30)
        d2 = ((xx - cx) ** 2 + (yy - cy) ** 2) / (r * r)
        alpha = np.clip(1.6 * np.exp(-1.6 * d2), 0, 1) + J(k, s + 3, 0.05, 0.15)
        alpha = np.clip(alpha, 0, 1)[:, :, None]
        return _img(a + (255.0 - a) * alpha)
    return Step(go)


_STAMPS = ["PROOF", "SAMPLE", "DRAFT", "PREVIEW", "COPY", "VOID", "DEMO", "SPECIMEN"]


def proof_stamp(s):
    """One word stamped across the whole picture, at an angle."""
    def go(im):
        k = seed_key_of(im)
        base = _fit(_rgb(im), _PIXEL_PIXELS).convert("RGBA")
        w, h = base.size
        text = PK(k, s, _STAMPS)
        layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        d = ImageDraw.Draw(layer)
        span = J(k, s + 1, 0.70, 0.95) * min(w, h * 1.6)
        size = 20
        while d.textlength(text, font=_font(size)) < span and size < 4000:
            size = int(size * 1.15) + 1
        font = _font(size)
        tw = d.textlength(text, font=font)
        colour = PK(k, s + 2, ((255, 255, 255), (220, 20, 30), (0, 0, 0)))
        alpha = JI(k, s + 3, 110, 190)
        x, y = (w - tw) / 2, (h - size * 1.2) / 2
        d.text((x, y), text, font=font, fill=colour + (alpha,))
        pad = size // 5
        d.rectangle([x - pad, y - pad // 2, x + tw + pad, y + size * 1.25 + pad // 2],
                    outline=colour + (alpha,), width=max(2, size // 14))
        layer = layer.rotate(T._sign(k, s + 4) * J(k, s + 5, 15.0, 40.0), resample=Image.BICUBIC)
        return Image.alpha_composite(base, layer).convert("RGB")
    return Step(go)


def grid_overlay(s):
    """Ruled lines over the whole frame: a viewfinder grid or graph paper."""
    def go(im):
        k = seed_key_of(im)
        base = _fit(_rgb(im), _PIXEL_PIXELS).convert("RGBA")
        w, h = base.size
        m = min(w, h)
        step = max(8, int(m * J(k, s, 0.05, 0.11)))
        width = max(1, int(m * J(k, s + 1, 0.003, 0.007)))
        colour = PK(k, s + 2, ((255, 255, 255), (0, 0, 0), (230, 30, 30), (40, 220, 230)))
        fill = colour + (JI(k, s + 3, 130, 220),)
        layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        d = ImageDraw.Draw(layer)
        for x in range(JI(k, s + 4, 0, step - 1), w, step):
            d.line([(x, 0), (x, h)], fill=fill, width=width)
        for y in range(JI(k, s + 5, 0, step - 1), h, step):
            d.line([(0, y), (w, y)], fill=fill, width=width)
        return Image.alpha_composite(base, layer).convert("RGB")
    return Step(go)


# --------------------------------------------------------------------------
# tone and colour
# --------------------------------------------------------------------------


def solarize(s):
    """Every tone above a per-seed level inverted."""
    return Step(lambda im: ImageOps.solarize(_rgb(im), JI(seed_key_of(im), s, 80, 170)))


def _grey(im, budget=_PIXEL_PIXELS):
    return np.asarray(ImageOps.grayscale(_fit(_rgb(im), budget)), dtype=np.float32)


def _cut(g, pct):
    """The tone at a percentile, or, where that leaves under a twentieth of
    the picture on one side (a page mostly one flat tone: it came out blank),
    half way between its darkest and lightest."""
    cut = np.percentile(g, pct)
    above = float((g > cut).mean())
    return cut if 0.05 <= above <= 0.95 else (float(g.min()) + float(g.max())) / 2


def threshold_bw(s):
    """Black or white, cut at a per-seed percentile of the picture's tones."""
    def go(im):
        g = _grey(im)
        cut = _cut(g, J(seed_key_of(im), s, 35.0, 65.0))
        return Image.fromarray(np.where(g > cut, 255, 0).astype("uint8"), "L").convert("RGB")
    return Step(go)


def dither_bw(s):
    """One bit a pixel, error-diffused, at 500-1,100 px."""
    def go(im):
        k = seed_key_of(im)
        im = _rgb(im)
        f = JI(k, s, 500, 1100) / max(im.size)
        if f < 1:
            im = im.resize((max(1, round(im.size[0] * f)), max(1, round(im.size[1] * f))),
                           Image.LANCZOS)
        return ImageOps.autocontrast(ImageOps.grayscale(im), cutoff=1).convert("1")
    return Step(go)


def palette_dither(s):
    """Four to sixteen colours, error-diffused."""
    return Step(lambda im: _palette_fs(_fit(_rgb(im), _PIXEL_PIXELS),
                                       JI(seed_key_of(im), s, 4, 16)))


def equalize(s):
    """Histogram equalisation, 60-100% of the way."""
    def go(im):
        im = _rgb(im)
        return Image.blend(im, ImageOps.equalize(im), J(seed_key_of(im), s, 0.6, 1.0))
    return Step(go)


def fade_matte(s):
    """The washed-out filter: blacks lifted, whites pulled down, colour drained
    and tinted."""
    def go(im):
        k = seed_key_of(im)
        im = ImageEnhance.Color(_rgb(im)).enhance(J(k, s + 2, 0.3, 0.7))
        lo, hi = J(k, s, 40, 90), J(k, s + 1, 185, 235)
        table = []
        for c in range(3):
            off = J(k, s + 3 + c, -12, 12)
            table += [max(0, min(255, int(lo + off + (hi - lo) * v / 255.0))) for v in range(256)]
        return im.point(table)
    return Step(go)


def curves(s):
    """A curve of its own for each channel: an S or its reverse, and a gamma.
    Monotone, so no tone is inverted."""
    def go(im):
        k = seed_key_of(im)
        table = []
        for c in range(3):
            a = J(k, s + c, 0.25, 0.5) if PK(k, s + 10 + c, (True, False)) \
                else -J(k, s + c, 0.4, 0.9)
            g = J(k, s + 20 + c, 0.7, 1.4)
            for v in range(256):
                t = v / 255.0
                t = t + a * 4 * t * (1 - t) * (t - 0.5)
                table.append(max(0, min(255, int(255 * max(0.0, t) ** g))))
        return _rgb(im).point(table)
    return Step(go)


def split_tone(s):
    """Shadows one way and highlights the other, cool / warm or warm / cool."""
    def go(im):
        k = seed_key_of(im)
        return T._split_tone(T._sign(k, s + 1) * J(k, s, 2.0, 4.5))(im)
    return Step(go)


def channel_drop(s):
    """Seen through a gel: one channel gone, or only one left."""
    def go(im):
        keep = PK(seed_key_of(im), s, ((0, 1, 1), (1, 0, 1), (1, 1, 0),
                                       (1, 0, 0), (0, 1, 0), (0, 0, 1)))
        ch = _rgb(im).split()
        return Image.merge("RGB", tuple(c if on else c.point(lambda v: 0)
                                        for c, on in zip(ch, keep)))
    return Step(go)


_MAPS = (
    ((0, 0, 0), (90, 0, 130), (220, 30, 30), (255, 160, 0), (255, 255, 120), (255, 255, 255)),
    ((0, 0, 140), (0, 120, 255), (0, 230, 200), (120, 255, 60), (255, 230, 0), (230, 20, 0)),
    ((68, 1, 84), (59, 82, 139), (33, 145, 140), (94, 201, 98), (253, 231, 37)),
    ((255, 255, 255), (150, 230, 255), (30, 110, 220), (10, 20, 90), (0, 0, 0)),
)


def false_colour(s):
    """Tones mapped through a colour scale: thermal, rainbow, green-blue, or
    one that runs from white down to black."""
    def go(im):
        stops = np.array(PK(seed_key_of(im), s, _MAPS), dtype=np.float32)
        x = np.linspace(0, len(stops) - 1, 256)
        lut = np.stack([np.interp(x, np.arange(len(stops)), stops[:, c]) for c in range(3)], 1)
        return Image.fromarray(lut.astype("uint8")[_grey(im).astype("uint8")], "RGB")
    return Step(go)


def exposure_gradient(s):
    """Exposure falling across the frame, in any direction: a quarter to a
    half at one side, 1.3 to 1.9 at the other."""
    def go(im):
        k = seed_key_of(im)
        a = _arr(im)
        h, w = a.shape[:2]
        th = J(k, s, 0.0, 2 * math.pi)
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        p = (xx / w - 0.5) * math.cos(th) + (yy / h - 0.5) * math.sin(th)
        t = (p - p.min()) / max(1e-6, p.max() - p.min())
        m0, m1 = J(k, s + 1, 0.25, 0.5), J(k, s + 2, 1.3, 1.9)
        return _img(a * (m0 + (m1 - m0) * t)[:, :, None])
    return Step(go)


# --------------------------------------------------------------------------
# geometry
# --------------------------------------------------------------------------


def affine_free(s):
    """Any affine map at once: turned 5-25 degrees, squeezed along one axis,
    sheared, on a canvas that holds all of it."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        th = math.radians(T._sign(k, s + 1) * J(k, s, 5.0, 25.0))
        sq = J(k, s + 2, 0.6, 1.0)
        sx, sy = (sq, 1.0) if PK(k, s + 3, (True, False)) else (1.0, sq)
        sh = T._sign(k, s + 5) * J(k, s + 4, 0.10, 0.30)
        rot = np.array([[math.cos(th), -math.sin(th)], [math.sin(th), math.cos(th)]])
        m = rot @ np.array([[1.0, sh], [0.0, 1.0]]) @ np.diag([sx, sy])
        corners = np.array([[0, 0], [w, 0], [w, h], [0, h]], dtype=float) @ m.T
        lo, hi = corners.min(0), corners.max(0)
        size = (int(math.ceil(hi[0] - lo[0])), int(math.ceil(hi[1] - lo[1])))
        inv = np.linalg.inv(m)
        off = inv @ lo
        bg = PK(k, s + 6, ((255, 255, 255), (0, 0, 0), _colour(k, s + 7, 20, 235)))
        return im.transform(size, Image.AFFINE,
                            (inv[0, 0], inv[0, 1], off[0], inv[1, 0], inv[1, 1], off[1]),
                            Image.BICUBIC, fillcolor=bg)
    return Step(go, coverage=0.6, geometry="rotate")


def cylinder(s):
    """Wrapped round a cylinder and seen from the front: the middle wide, the
    sides squeezed. Round either axis."""
    def go(im):
        a = math.radians(J(seed_key_of(im), s, 35.0, 65.0))
        return T._remap(lambda nx, ny: (np.arcsin(np.clip(nx, -1, 1) * math.sin(a)) / a, ny),
                        0)(im)
    return Step(_oriented(s + 5, AXES, go)[0], geometry="perspective")


def ripple(s):
    """Rings spreading from the middle, as through water."""
    def go(im):
        k = seed_key_of(im)
        amp, freq = J(k, s, 0.008, 0.020), J(k, s + 1, 3.0, 7.0)

        def fn(nx, ny):
            r = np.sqrt(nx * nx + ny * ny)
            scale = (r + amp * np.sin(2 * np.pi * freq * r)) / np.maximum(r, 1e-6)
            return nx * scale, ny * scale
        return T._remap(fn, 0.03)(im)
    return Step(go, (0.03, 0.03, 0.97, 0.97), geometry="perspective")


def liquify(s):
    """Pushed about smoothly, a few per cent here and there: three slow waves
    each way, their directions and phases per seed."""
    def go(im):
        k = seed_key_of(im)

        def field(nx, ny, t):
            out = 0.0
            for i in range(3):
                u = t + 10 * i
                fx, fy = J(k, u, -1.5, 1.5), J(k, u + 1, -1.5, 1.5)
                out = out + J(k, u + 2, 0.020, 0.045) * np.sin(
                    math.pi * (fx * nx + fy * ny) + J(k, u + 3, 0.0, 2 * math.pi))
            return out
        return T._remap(lambda nx, ny: (nx + field(nx, ny, s), ny + field(nx, ny, s + 100)),
                        0.07)(im)
    return Step(go, (0.07, 0.07, 0.93, 0.93), geometry="perspective")


def fold(s):
    """A print folded once and laid open: the two sides lean opposite ways
    from the crease, one in shadow."""
    def of(key):
        return J(key, s, 0.06, 0.16), J(key, s + 1, -0.3, 0.3)

    def margin(key):
        return 0.4 * of(key)[0] + 0.005

    def go(im):
        k = seed_key_of(im)
        amt, c = of(k)
        out = T._remap(lambda nx, ny: (nx, ny + amt * np.abs(nx - c) - amt / 2), 0)(im)
        a = np.asarray(out, dtype=np.float32)
        h, w = a.shape[:2]
        x = np.linspace(-1, 1, w, dtype=np.float32)
        side = (x > c) if PK(k, s + 2, (True, False)) else (x < c)
        shade = 1.0 - J(k, s + 3, 0.10, 0.30) * side * (1 - 0.5 * np.abs(x - c))
        m = margin(k)
        return T._crop_box(_img(a * shade[None, :, None]), m, m, 1 - m, 1 - m)

    fn, where = _oriented(s + 5, AXES, go,
                          lambda w, h, key: (margin(key), margin(key),
                                             1 - margin(key), 1 - margin(key)))
    return Step(fn, where, geometry="perspective")


# --------------------------------------------------------------------------
# degradation
# --------------------------------------------------------------------------


def film_grain(s):
    """Grain that follows exposure, and the softness that comes with it."""
    def go(im):
        k = seed_key_of(im)
        return T._film_grain(J(k, s, 18.0, 45.0), 1_000_003 * k + s)(im)
    return Step(go)


def salt_pepper(s):
    """Dead and hot pixels: 4-14% of a megapixel copy black or white."""
    def go(im):
        k = seed_key_of(im)
        a = np.asarray(_fit(_rgb(im), 1_000_000)).copy()
        r = _rng(k, s).random(a.shape[:2])
        p = J(k, s, 0.04, 0.14)
        a[r < p / 2] = 0
        a[r > 1 - p / 2] = 255
        return Image.fromarray(a, "RGB")
    return Step(go)


def zoom_blur(s):
    """Zoomed during the exposure: blurred outward from a point."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        w, h = im.size
        cx, cy = w * J(k, s, 0.3, 0.7), h * J(k, s + 1, 0.3, 0.7)
        z, n = J(k, s + 2, 0.04, 0.10), 12
        acc = np.zeros((h, w, 3), dtype=np.float32)
        for i in range(n):
            f = 1.0 / (1.0 + z * i / (n - 1))
            acc += np.asarray(im.transform((w, h), Image.AFFINE,
                                           (f, 0, cx * (1 - f), 0, f, cy * (1 - f)),
                                           Image.BILINEAR), dtype=np.float32)
        return _img(acc / n)
    return Step(go)


_GHOST = 0.06


def ghost(s):
    """A double image: itself again, 2-6% of the frame away, at 35-50%."""
    def go(im):
        k = seed_key_of(im)
        a = _arr(im)
        h, w = a.shape[:2]
        d, th = J(k, s, 0.02, _GHOST), J(k, s + 1, 0.0, 2 * math.pi)
        moved = np.roll(np.roll(a, int(round(d * h * math.sin(th))), axis=0),
                        int(round(d * w * math.cos(th))), axis=1)
        al = J(k, s + 2, 0.35, 0.50)
        return T._crop_box(_img(a * (1 - al) + moved * al), _GHOST, _GHOST, 1 - _GHOST, 1 - _GHOST)
    return Step(go, (_GHOST, _GHOST, 1 - _GHOST, 1 - _GHOST))


def bloom(s):
    """Highlights glowing into what is round them, and a little overexposed."""
    def go(im):
        k = seed_key_of(im)
        im = _fit(_rgb(im))
        soft = np.asarray(im.filter(ImageFilter.GaussianBlur(
            max(2.0, max(im.size) * J(k, s, 0.02, 0.05)))), dtype=np.float32)
        a = np.asarray(im, dtype=np.float32) * J(k, s + 2, 1.05, 1.20)
        a = np.clip(a, 0, 255)
        return _img(255.0 - (255.0 - a) * (1.0 - soft * J(k, s + 1, 0.5, 0.9) / 255.0))
    return Step(go)


def haze(s):
    """Fog: washed toward a pale colour, thicker to one side."""
    def go(im):
        k = seed_key_of(im)
        a = _arr(im)
        h, w = a.shape[:2]
        t = _ramp(h, w, PK(k, s, "lrtb"), 1.0)
        a0, a1 = J(k, s + 1, 0.25, 0.45), J(k, s + 2, 0.55, 0.80)
        amt = (a0 + (a1 - a0) * t)[:, :, None]
        fog = np.array(_colour(k, s + 3, 215, 250), dtype=np.float32)
        return _img(a * (1 - amt) + fog * amt)
    return Step(go)


def glitch_bands(s):
    """A damaged file: four to nine bands slid sideways 1-6%, some with their
    colour channels apart."""
    def go(im):
        k = seed_key_of(im)
        a = np.asarray(_fit(_rgb(im), _PIXEL_PIXELS)).copy()
        h, w = a.shape[:2]
        rng = _rng(k, s)
        n = JI(k, s, 4, 9)
        cuts = [0] + sorted(int(v) for v in rng.uniform(0.03, 0.97, n - 1) * h) + [h]
        for y0, y1 in zip(cuts, cuts[1:]):
            if y1 <= y0:
                continue
            shift = int(rng.choice((-1, 1)) * rng.uniform(0.01, 0.06) * w)
            a[y0:y1] = np.roll(a[y0:y1], shift, axis=1)
            if rng.random() < 0.34:
                c = max(1, int(rng.uniform(0.005, 0.015) * w))
                a[y0:y1, :, 0] = np.roll(a[y0:y1, :, 0], c, axis=1)
                a[y0:y1, :, 2] = np.roll(a[y0:y1, :, 2], -c, axis=1)
        return Image.fromarray(a, "RGB")
    return Step(go)


def h264_frame(s):
    """A frame of a video: 480-1,280 px, through libx264 at crf 34-44."""
    def go(im):
        k = seed_key_of(im)
        im = _rgb(im)
        f = min(1.0, JI(k, s, 480, 1280) / max(im.size))
        size = tuple(max(2, int(round(v * f / 2)) * 2) for v in im.size)
        with tempfile.TemporaryDirectory() as tmp:
            src, mp4, dst = (os.path.join(tmp, n) for n in ("in.png", "v.mp4", "out.png"))
            im.resize(size, Image.LANCZOS).save(src)
            for cmd in (["-i", src, "-c:v", "libx264", "-preset", "fast", "-crf",
                         str(JI(k, s + 1, 34, 44)), "-pix_fmt", "yuv420p", mp4],
                        ["-i", mp4, "-frames:v", "1", dst]):
                p = subprocess.run(["ffmpeg", "-y", "-loglevel", "error", *cmd],
                                   capture_output=True, text=True)
                if p.returncode != 0:
                    raise RuntimeError("ffmpeg: " + p.stderr.strip()[:200])
            with Image.open(dst) as out:
                return out.convert("RGB")
    return Step(go)


# --------------------------------------------------------------------------
# rearrangement: traps
# --------------------------------------------------------------------------


def strip_shuffle(s):
    """Four to seven strips, across or down, in another order."""
    def go(im):
        k = seed_key_of(im)
        im = _rgb(im)
        w, h = im.size
        n = JI(k, s, 4, 7)
        order = T._derangement(k, s + 1000, n)
        sw = w // n
        out = Image.new("RGB", (sw * n, h))
        for dest, src in enumerate(order):
            out.paste(im.crop((src * sw, 0, (src + 1) * sw, h)), (dest * sw, 0))
        return out
    return Step(_oriented(s + 5, AXES, go)[0], scramble="strips")


def cells_rotate(s):
    """Every cell of a three- or four-way grid turned half round in place."""
    def go(im):
        k = seed_key_of(im)
        im = _rgb(im)
        w, h = im.size
        g = JI(k, s, 3, 4)
        cw, ch = w // g, h // g
        out = im.crop((0, 0, cw * g, ch * g))
        for r in range(g):
            for c in range(g):
                box = (c * cw, r * ch, (c + 1) * cw, (r + 1) * ch)
                out.paste(im.crop(box).transpose(Image.ROTATE_180), box)
        return out
    return Step(go, scramble="cells")


def strip_mirror(s):
    """Three to six strips, each mirrored where it stands."""
    def go(im):
        k = seed_key_of(im)
        im = _rgb(im)
        w, h = im.size
        n = JI(k, s, 3, 6)
        sw = w // n
        out = im.crop((0, 0, sw * n, h))
        for i in range(n):
            box = (i * sw, 0, (i + 1) * sw, h)
            out.paste(im.crop(box).transpose(Image.FLIP_LEFT_RIGHT), box)
        return out
    return Step(_oriented(s + 5, AXES, go)[0], scramble="stripmir")


# --------------------------------------------------------------------------
# pipelines
# --------------------------------------------------------------------------


def _long_edge(im, target):
    f = target / max(im.size)
    if f >= 1:
        return im
    return im.resize((max(1, round(im.size[0] * f)), max(1, round(im.size[1] * f))),
                     Image.LANCZOS)


def fax(s):
    """Faxed: fed slightly askew, 600-1,100 px, one bit a pixel, speckled."""
    turn, reg = T._rotate_jit(s, 0.5, 3.0)

    def go(im):
        k = seed_key_of(im)
        g = np.asarray(ImageOps.autocontrast(ImageOps.grayscale(
            _long_edge(turn(_rgb(im)), JI(k, s + 1, 600, 1100))), cutoff=2), dtype=np.float32)
        cut = _cut(g, J(k, s + 3, 40.0, 60.0))
        g = g + _rng(k, s).normal(0, J(k, s + 2, 6.0, 18.0), g.shape)
        return Image.fromarray(np.where(g > cut, 255, 0).astype("uint8"), "L").convert("1")
    return Step(go, reg, geometry="rotate")


def photocopy(s):
    """Photocopied: grey, hard contrast, toner noise and streaks, a shadow
    down one edge."""
    def go(im):
        k = seed_key_of(im)
        g = np.asarray(ImageOps.grayscale(_long_edge(_rgb(im), JI(k, s, 700, 1400))),
                       dtype=np.float32)
        h, w = g.shape
        rng = _rng(k, s)
        g = (g - np.median(g)) * J(k, s + 1, 1.8, 3.2) + J(k, s + 2, 120.0, 175.0)
        g = g + rng.normal(0, J(k, s + 3, 8.0, 20.0), g.shape)
        for _ in range(JI(k, s + 4, 1, 3)):
            x0 = int(rng.uniform(0, 0.97) * w)
            g[:, x0:x0 + max(1, int(rng.uniform(0.004, 0.015) * w))] += rng.choice((-1, 1)) * \
                rng.uniform(25, 70)
        edge = _ramp(h, w, PK(k, s + 5, "lrtb"), J(k, s + 6, 0.04, 0.08))
        g = g * (1.0 - J(k, s + 7, 0.3, 0.6) * edge)
        return Image.fromarray(np.clip(g, 0, 255).astype("uint8"), "L").convert("RGB")
    return Step(go)


_INKS = (((34, 20, 8), (255, 236, 205)), ((20, 20, 30), (235, 230, 215)),
         ((45, 25, 5), (250, 225, 170)), ((25, 15, 15), (240, 215, 200)))


def old_photo(s):
    """A print from a shoebox: toned, faded, soft, darkened at the corners,
    grainy, scratched and dusty."""
    def go(im):
        k = seed_key_of(im)
        im = _long_edge(_rgb(im), JI(k, s, 900, 1600))
        dark, light = PK(k, s + 1, _INKS)
        im = ImageOps.colorize(ImageOps.grayscale(im), dark, light).convert("RGB")
        im = im.filter(ImageFilter.GaussianBlur(J(k, s + 2, 0.6, 1.4)))
        a = np.asarray(T._vignette(J(k, s + 3, 0.3, 0.6))(im), dtype=np.float32)
        lift = J(k, s + 4, 20.0, 55.0)
        a = lift + a * (235.0 - lift) / 255.0
        rng = _rng(k, s)
        a = a + rng.normal(0, J(k, s + 5, 8.0, 18.0), a.shape[:2] + (1,))
        out = _img(a)
        w, h = out.size
        d = ImageDraw.Draw(out)
        for _ in range(JI(k, s + 6, 6, 20)):
            x, y0 = rng.uniform(0, w), rng.uniform(0, h * 0.8)
            d.line([(x, y0), (x + rng.normal(0, w * 0.01), y0 + rng.uniform(0.1, 0.9) * h)],
                   fill=(245, 240, 225) if rng.random() < 0.7 else (40, 30, 20),
                   width=1 + int(rng.random() < 0.3))
        for _ in range(JI(k, s + 7, 20, 60)):
            x, y, r = rng.uniform(0, w), rng.uniform(0, h), rng.uniform(1, max(2, w * 0.004))
            d.ellipse([x - r, y - r, x + r, y + r],
                      fill=(250, 245, 230) if rng.random() < 0.6 else (30, 22, 15))
        return out
    return Step(go)


def vhs(s):
    """Off a video tape: 240-480 lines, soft across, colour smeared and
    shifted, noisy line by line, the bottom lines torn."""
    def go(im):
        k = seed_key_of(im)
        im = _rgb(im)
        f = min(1.0, JI(k, s, 240, 480) / im.size[1])
        w, h = max(8, round(im.size[0] * f)), max(8, round(im.size[1] * f))
        im = im.resize((w, h), Image.LANCZOS)
        im = im.resize((max(4, int(w * J(k, s + 1, 0.5, 0.7))), h), Image.BILINEAR) \
               .resize((w, h), Image.BILINEAR)
        im = ImageEnhance.Color(im).enhance(J(k, s + 2, 1.1, 1.4))
        y, cb, cr = im.convert("YCbCr").split()
        shift = JI(k, s + 3, 2, 6)
        smear = (max(2, w // JI(k, s + 4, 12, 28)), h)
        cb, cr = (np.roll(np.asarray(c.resize(smear, Image.BILINEAR).resize((w, h), Image.BILINEAR)),
                          shift, axis=1) for c in (cb, cr))
        rng = _rng(k, s)
        ya = np.asarray(y, dtype=np.float32)
        ya = ya + rng.normal(0, J(k, s + 5, 3.0, 8.0), (h, 1)) \
                + rng.normal(0, J(k, s + 6, 5.0, 12.0), (h, w))
        tear = max(1, int(h * J(k, s + 7, 0.03, 0.06)))
        ya[h - tear:] = np.roll(ya[h - tear:], int(w * J(k, s + 8, 0.05, 0.15)), axis=1) \
            + rng.normal(0, 25, (tear, w))
        out = np.stack([np.clip(ya, 0, 255).astype("uint8"), cb, cr], axis=2)
        return Image.fromarray(out, "YCbCr").convert("RGB")
    return Step(go)


# --------------------------------------------------------------------------
# the catalogue
# --------------------------------------------------------------------------

DYNAMIC_REGIONS = {}
CATALOGUE = []
_slot = [0]


def _salt():
    _slot[0] += 1
    return 100_000 + 5_000 * _slot[0]


def _of(value, key):
    return value(key) if callable(value) else value


def _variant(name, steps, fmt, save=None, note="", writer=None):
    """A catalogue entry from one step or a chain of two; the ground truth is
    read off the steps."""
    def go(im):
        for st in steps:
            im = st.fn(im)
        return im

    def region(w, h, key):
        r = FULL
        for st in steps:
            if st.region is None:
                continue
            here = st.region(w, h, key) if callable(st.region) else st.region
            r = _compose(r, here)
            w, h = max(1.0, w * (here[2] - here[0])), max(1.0, h * (here[3] - here[1]))
        return r

    def coverage(key):
        c = 1.0
        for st in steps:
            c *= _of(st.coverage, key)
        return c

    if any(st.region is not None for st in steps):
        DYNAMIC_REGIONS[name] = region
    CATALOGUE.append(Variant(
        name, _keyed(go), fmt, save if save is not None else {},
        geometry=next((st.geometry for st in steps if st.geometry != "none"), "none"),
        note=note, coverage=coverage,
        scramble=next((st.scramble for st in steps if st.scramble), ""), writer=writer))


def _q(lo, hi):
    return _jit_kw(_salt(), "quality", lo, hi)


def one(name, factory, fmt="JPEG", save=None, note="", writer=None):
    _variant(name, [factory(_salt())], fmt, save, note or (factory.__doc__ or "").strip()
             .split("\n\n")[0].replace("\n", " ").replace("    ", " "), writer)


def two(a, b, fmt="JPEG", save=None, salt_a=None):
    """`a` then `b`, each from a salt of its own (`salt_a`: the first step's,
    where the chain has to make the very file a single transformation does)."""
    first = a(salt_a if salt_a is not None else _salt())
    _variant(f"{a.__name__}+{b.__name__}", [first, b(_salt())], fmt, save,
             f"{a.__name__}, then {b.__name__}")


def _identity(s):
    return Step(_rgb)


# -- containers: the pixels as they were, or one lossy step -----------------
one("to_bmp", _identity, "BMP", note="lossless container change")
one("png_16bit", _identity, "PNG", writer=T._magick("-depth", "16", "-define", "png:bit-depth=16"),
    note="sixteen bits a channel, by ImageMagick: the same picture")
one("jxl_lossless", _identity, "JXL", {"quality": 100}, note="lossless JPEG XL")
one("to_gif", to_gif, "GIF", note="256 colours, error-diffused")
one("tiff_jpeg", _identity, "TIFF", _jit_kw(_salt(), "quality", 40, 85, compression="jpeg"),
    note="a TIFF holding a JPEG stream")
_exif_salt = _salt()
_variant("exif_turned", [exif_turned(_exif_salt)], "JPEG", _exif_kw(_exif_salt, 82, 95),
         exif_turned.__doc__.replace("\n", " ").replace("    ", " "))

# -- resolution and crops ---------------------------------------------------
one("pixelate", pixelate, "PNG")
one("crop_corner", crop_corner, save=_q(70, 92))
one("crop_band", crop_band, save=_q(70, 92))
one("digital_zoom", digital_zoom, save=_q(60, 85))
one("crop_circle", crop_circle, "PNG")

# -- hosts ------------------------------------------------------------------
one("letterbox", letterbox, save=_q(70, 92))
one("rotate_canvas", rotate_canvas, save=_q(70, 92))
one("tiled_repeat", tiled_repeat, save=_q(70, 92))
one("mirror_extend", mirror_extend, save=_q(70, 92))
one("before_after", before_after, save=_q(70, 92))
one("chat_bubble", chat_bubble, "PNG")
one("social_post", social_post, save=_q(60, 88))
one("desktop_window", desktop_window, "PNG")
one("triptych", triptych, save=_q(70, 92))

# -- occlusion --------------------------------------------------------------
one("emoji_stickers", emoji_stickers, save=_q(70, 92))
one("meme_text", meme_text, save=_q(50, 85))
one("broadcast_overlay", broadcast_overlay, save=_q(60, 88))
one("scribble", scribble, "PNG")
one("mosaic_patch", mosaic_patch, save=_q(70, 92))
one("light_leak", light_leak, save=_q(70, 92))
one("glare_spot", glare_spot, save=_q(70, 92))
one("proof_stamp", proof_stamp, save=_q(70, 92))
one("grid_overlay", grid_overlay, "PNG")

# -- tone and colour --------------------------------------------------------
one("solarize", solarize, save=_q(75, 92))
one("threshold_bw", threshold_bw, "PNG")
one("dither_bw", dither_bw, "PNG")
one("palette_dither", palette_dither, "PNG")
one("equalize", equalize, save=_q(75, 92))
one("fade_matte", fade_matte, save=_q(70, 92))
one("curves", curves, save=_q(75, 92))
one("split_tone", split_tone, save=_q(75, 92))
one("channel_drop", channel_drop, save=_q(75, 92))
one("false_colour", false_colour, "PNG")
one("exposure_gradient", exposure_gradient, save=_q(75, 92))

# -- geometry ---------------------------------------------------------------
one("affine_free", affine_free, save=_q(70, 92))
one("cylinder", cylinder, save=_q(75, 92))
one("ripple", ripple, save=_q(75, 92))
one("liquify", liquify, save=_q(75, 92))
one("fold", fold, save=_q(70, 92))

# -- degradation ------------------------------------------------------------
one("film_grain", film_grain, save=_q(80, 94))
one("salt_pepper", salt_pepper, "PNG")
one("zoom_blur", zoom_blur, save=_q(75, 92))
one("ghost", ghost, save=_q(75, 92))
one("bloom", bloom, save=_q(75, 92))
one("haze", haze, save=_q(75, 92))
one("glitch_bands", glitch_bands, save=_q(75, 92))
one("h264_frame", h264_frame, "PNG")

# -- rearrangement: traps ---------------------------------------------------
_cells_salt = _salt()
one("strip_shuffle", strip_shuffle, "PNG")
_variant("cells_rotate", [cells_rotate(_cells_salt)], "PNG",
         note="every cell of a three- or four-way grid turned half round in place")
one("strip_mirror", strip_mirror, "PNG")

# -- pipelines --------------------------------------------------------------
one("fax", fax, "PNG")
one("photocopy", photocopy, save=_q(50, 80))
one("old_photo", old_photo, save=_q(65, 90))
one("vhs", vhs, save=_q(60, 85))

N_SINGLE = len(CATALOGUE)

# -- two at once ------------------------------------------------------------
two(crop_corner, fade_matte, save=_q(60, 90))
two(crop_corner, social_post, save=_q(60, 88))
two(crop_band, ghost, save=_q(70, 92))
two(crop_band, channel_drop, save=_q(70, 92))
two(digital_zoom, meme_text, save=_q(50, 85))
two(digital_zoom, equalize, save=_q(70, 92))
two(crop_circle, haze, save=_q(70, 92))
two(emoji_stickers, rotate_canvas, save=_q(70, 92))
two(rotate_canvas, split_tone, save=_q(70, 92))
two(threshold_bw, rotate_canvas, "PNG")
two(old_photo, rotate_canvas, save=_q(65, 90))
two(affine_free, film_grain, save=_q(80, 94))
two(grid_overlay, affine_free, save=_q(70, 92))
two(cylinder, curves, save=_q(75, 92))
two(scribble, cylinder, save=_q(75, 92))
two(ripple, bloom, save=_q(75, 92))
two(liquify, palette_dither, "PNG")
two(fold, photocopy, save=_q(50, 80))
two(proof_stamp, fax, "PNG")
two(letterbox, h264_frame, "PNG")
two(broadcast_overlay, vhs, save=_q(60, 85))
two(meme_text, pixelate, save=_q(40, 80))
two(mirror_extend, glare_spot, save=_q(70, 92))
two(tiled_repeat, solarize, save=_q(75, 92))
two(before_after, light_leak, save=_q(70, 92))
two(chat_bubble, glitch_bands, save=_q(70, 92))
two(desktop_window, salt_pepper, "PNG")
two(triptych, film_grain, save=_q(80, 94))
two(false_colour, letterbox, save=_q(75, 92))
# the trap, edited: the same arrangement as `cells_rotate` (its salt), so the
# two are one rearranged picture and neither is the seed
two(cells_rotate, fade_matte, save=_q(70, 92), salt_a=_cells_salt)

BY_NAME = {v.name: v for v in CATALOGUE}
assert len(BY_NAME) == len(CATALOGUE), "duplicate variant name"
