"""gl.py A B [bar]: true DISC21 pairs stated in one run only, at a correlation bar."""
import csv, json, os, sys
X = '/home/daniel/Documents/DISC21'
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f"{X}/{os.environ.get('REFS', 'refs0')}.ids")}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
bar = float(sys.argv[3]) if len(sys.argv) > 3 else 0.5
def load(d):
    out = {}
    for p in json.load(open(f'{d}/report.json'))['pairs']:
        a, b = ident(p['a']), ident(p['b'])
        k = (a, b) if a[0] == 'Q' else (b, a)
        if p['pixel_correlation'] >= bar: out[k] = (p['aligned_points'], p['pixel_correlation'])
    return out
A, B = load(sys.argv[1]), load(sys.argv[2])
ta, tb = {k for k in A if k in gt}, {k for k in B if k in gt}
fa, fb = {k for k in A if k not in gt}, {k for k in B if k not in gt}
print(f'true: A {len(ta)} B {len(tb)}; only A {len(ta - tb)}, only B {len(tb - ta)}; false: A {len(fa)} B {len(fb)}, only B {len(fb - fa)}, only A {len(fa - fb)}')
for k in sorted(ta - tb): print('  lost', k, A[k])
for k in sorted(tb - ta): print('  got ', k, B[k])
