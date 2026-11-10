"""True DISC21 shard-0 copies: stated by the report, verified (in the dump) but not stated, or never in the dump."""
import csv, json, os, sys
X = '/home/daniel/Documents/DISC21'; B = sys.argv[1]
ident = lambda p: os.path.splitext(os.path.basename(p))[0]
refs = {l.strip() for l in open(f'{X}/refs0.ids')}
gt = {(r[0], r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r) > 1 and r[1] in refs}
def qr(a, b):
    a, b = ident(a), ident(b)
    if {a[0], b[0]} != {'Q', 'R'}: return None
    return (a, b) if a[0] == 'Q' else (b, a)
rep = {qr(p['a'], p['b']) for p in json.load(open(f'{B}/report.json'))['pairs']}
dump = {}
for r in csv.DictReader(open(f'{B}/dump.csv')):
    k = qr(r['a'], r['b'])
    if k in gt: dump.setdefault(k, []).append((int(r['n_in']), int(r['n_match']), float(r['blk']), r['kind']))
st = gt & rep; dv = {k for k in gt - rep if k in dump}; nv = gt - rep - set(dump)
print(f'true {len(gt)}: stated {len(st)}, verified not stated {len(dv)}, never in dump {len(nv)}')
with open(f'{os.path.basename(B.rstrip("/"))}.missing', 'w') as f:
    for q, r in sorted(nv): f.write(f'{q} {r} none\n')
    for k in sorted(dv): f.write(f'{k[0]} {k[1]} ' + ';'.join(f'{a}/{b}/{c:.2f}/{d}' for a, b, c, d in dump[k]) + '\n')
