"""MIRFAM: each IMGS-ALL seed with two copies, and a turned version of it with two copies
of its own, so that both sides have two anchors and neither is re-asked today."""
import csv, os, sys, hashlib
from PIL import Image, ImageOps
try: import pillow_avif  # noqa
except ImportError: pass
try:
    from pillow_heif import register_heif_opener; register_heif_opener()
except ImportError: pass
OUT = '/home/daniel/Documents/MIRFAM'
rows = [r for r in csv.DictReader(open('/home/daniel/Documents/IMGS-ALL/derived/manifest.csv')) if r['variant'] == 'ORIGINAL']
truth = []
for r in rows:
    try:
        im = ImageOps.exif_transpose(Image.open(r['path'])).convert('RGB')
    except Exception as e:
        print('skip', r['rel_path'], e, file=sys.stderr); continue
    im.thumbnail((1600, 1600))
    h = int(hashlib.md5(r['rel_path'].encode()).hexdigest(), 16)
    turn = ['flip_h', 'flip_h', 'flip_v', 'rot180'][h % 4]
    t = {'flip_h': ImageOps.mirror, 'flip_v': ImageOps.flip, 'rot180': lambda x: x.rotate(180)}[turn](im)
    name = r['rel_path'].replace('/', '_').rsplit('.', 1)[0]
    d = f'{OUT}/{name}'; os.makedirs(d, exist_ok=True)
    def save(img, f, q, s=1.0):
        if s != 1.0: img = img.resize((max(1, int(img.width * s)), max(1, int(img.height * s))), Image.LANCZOS)
        p = f'{d}/{f}.jpg'; img.save(p, quality=q); truth.append((name, 'o' if f[0] == 'o' else 'm', turn, p))
    save(im, 'o', 95); save(im, 'o_q80', 80); save(im, 'o_s70', 90, 0.7)
    save(t, 'm', 92); save(t, 'm_q78', 78); save(t, 'm_s60', 90, 0.6)
with open(f'{OUT}/truth.csv', 'w') as f:
    csv.writer(f).writerows([('seed', 'side', 'turn', 'path')] + truth)
print(len(truth), 'files')
