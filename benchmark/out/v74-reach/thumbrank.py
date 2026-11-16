#!/usr/bin/env python3
"""thumbrank.py: would a whole-frame thumbnail find the 51? Each reference of
shard 0 as a 16x16 (and 8x8) grey thumbnail, zero mean and unit norm; each of
the 51 queries (and its mirror) ranked against all of them by correlation."""
import re, os, numpy as np
from PIL import Image
from concurrent.futures import ProcessPoolExecutor
H = '/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out'; D = '/home/daniel/Documents/DISC21'
def thumb(p):
    try:
        im = Image.open(p); im.draft('L', (64, 64)); im = im.convert('L').resize((16, 16), Image.BOX)
        return np.asarray(im, dtype=np.float32)
    except Exception:
        return np.zeros((16, 16), np.float32)
def norm(a):
    a = a.reshape(len(a), -1); a = a - a.mean(1, keepdims=True)
    return a / np.maximum(np.linalg.norm(a, axis=1, keepdims=True), 1e-6)
def small(a):
    return a.reshape(len(a), 8, 2, 8, 2).mean((2, 4))
if __name__ == '__main__':
    pairs = []
    for l in open(f'{H}/v73-disc-all/why/classify.out'):
        m = re.match(r'^(Q\d+) (R\d+) (.*?) (None|\(.*?\)) (True|False|None) (None|\(.*\))$', l.strip())
        if m and m.group(5) == 'True':
            pairs.append((m.group(1), m.group(2)))
    refs = sorted(os.listdir(f'{D}/refs0/images/references'))
    with ProcessPoolExecutor(8) as ex:
        R = np.stack(list(ex.map(thumb, [f'{D}/refs0/images/references/{r}' for r in refs], chunksize=256)))
    Q = np.stack([thumb(f'{D}/queries/images/queries/{q}.jpg') for q, _ in pairs])
    at = {r[:-4]: i for i, r in enumerate(refs)}
    for name, f in (('16x16', lambda a: a), ('8x8', small)):
        r, q, qm = norm(f(R)), norm(f(Q)), norm(f(Q[:, :, ::-1]))
        s = np.maximum(q @ r.T, qm @ r.T)
        rk = [(s[i] > s[i, at[p[1]]]).sum() for i, p in enumerate(pairs)]
        print(name, 'first', sum(x == 0 for x in rk), '<10', sum(x < 10 for x in rk), '<150', sum(x < 150 for x in rk), '<1000', sum(x < 1000 for x in rk))
        print('  ', ' '.join(f'{p[0]}:{x}' for p, x in zip(pairs, rk)))
