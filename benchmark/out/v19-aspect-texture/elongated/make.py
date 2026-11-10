"""A ground-truthed set of elongated pictures for finding A.

Each page stacks k different seed photographs (each used once, so nothing but
the stated pairs is the same picture): vertical pages are full-page
screenshots / long comics, horizontal ones panoramas. For each page, two
queries: one section saved on its own (as an image someone also has), and a
screen-shaped viewport crop of another part of the page. Positives are
(section, page) and (viewport, page); every other pair is a negative.
"""
import os, random, glob, json
from PIL import Image
random.seed(19)
OUT = "/home/daniel/Documents/IMGS-ELONG"
os.makedirs(OUT, exist_ok=True)
seeds = []
for c in ["IMGS2", "IMGS3", "IMGS4"]:
    seeds += sorted(p for p in glob.glob(f"/home/daniel/Documents/{c}/*") if os.path.isfile(p))
random.shuffle(seeds)
SW, SH = 1080, 810            # a section, 4:3
plan = [("v", k) for k in [2, 3, 4, 5, 6, 8, 10] for _ in range(3)] + [("h", k) for k in [2, 3, 4, 6] for _ in range(3)]
truth, partial, used = [], [], 0
for n, (o, k) in enumerate(plan):
    secs = []
    while len(secs) < k:
        p = seeds[used]; used += 1
        try:
            im = Image.open(p); im.load(); im = im.convert("RGB")
        except Exception:
            continue
        s = max(SW / im.width, SH / im.height)
        im = im.resize((max(SW, round(im.width * s)), max(SH, round(im.height * s))), Image.LANCZOS)
        l, t = (im.width - SW) // 2, (im.height - SH) // 2
        secs.append(im.crop((l, t, l + SW, t + SH)))
    W, H = (SW, SH * k) if o == "v" else (SW * k, SH)
    page = Image.new("RGB", (W, H))
    for i, s in enumerate(secs):
        page.paste(s, (0, i * SH) if o == "v" else (i * SW, 0))
    name = f"p{n:02d}_{o}{k}"
    page.save(f"{OUT}/{name}_page.jpg", quality=90)
    i = random.randrange(k)
    secs[i].resize((SW * 3 // 4, SH * 3 // 4), Image.LANCZOS).save(f"{OUT}/{name}_section.jpg", quality=85)
    # A viewport elsewhere on the page: 9:16 on a vertical page, 16:9 on a
    # horizontal one, kept off section i.
    others = [j for j in range(k) if j != i]
    j = random.choice(others)
    if o == "v":
        vw, vh = SW, min(H, SW * 16 // 9)
        y = min(max(0, j * SH + SH // 2 - vh // 2), H - vh)
        if i * SH < y + vh and y < (i + 1) * SH:      # touching section i: slide away
            y = (i + 1) * SH if j > i else max(0, i * SH - vh)
        box, sec = (0, y, vw, y + vh), (0, i * SH, SW, (i + 1) * SH)
        vp = page.crop(box)
    else:
        vh, vw = SH, min(W, SH * 16 // 9)
        x = min(max(0, j * SW + SW // 2 - vw // 2), W - vw)
        if i * SW < x + vw and x < (i + 1) * SW:
            x = (i + 1) * SW if j > i else max(0, i * SW - vw)
        box, sec = (x, 0, x + vw, vh), (i * SW, 0, (i + 1) * SW, SH)
        vp = page.crop(box)
    inter = max(0, min(box[2], sec[2]) - max(box[0], sec[0])) * max(0, min(box[3], sec[3]) - max(box[1], sec[1]))
    if inter >= SW * SH * 0.95:
        truth.append((f"{name}_section.jpg", f"{name}_viewport.png"))
    elif inter > 0:
        partial.append((f"{name}_section.jpg", f"{name}_viewport.png"))
    vp.save(f"{OUT}/{name}_viewport.png")
    truth += [(f"{name}_section.jpg", f"{name}_page.jpg"), (f"{name}_viewport.png", f"{name}_page.jpg")]
json.dump(dict(positives=truth, partial=partial), open(f"{OUT}/truth.json", "w"))
print(len(plan), "pages,", used, "seeds,", len(truth), "positives,", len(partial), "partial")
