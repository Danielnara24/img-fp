import sys, collections, re
sys.argv=['x']; exec(open('watch/why.py').read().split('for run in sys.argv')[0])
def fate(run):
    rank, ver, stated, files = load(run)
    out = {}
    for a, b in sib:
        k = key(a, b)
        if a in files and b in files:
            w, same = why(k, rank, ver, stated)
            n_in = max([int(m.group(1)) for v in same for m in [re.search(r'n_in (\d+)', '\t'.join(v))] if m] or [-1])
            r = [int(x[1]) for x in rank[k] if x[1].isdigit()]
            out[k] = (w, n_in, min(r) if r else None, rank[k])
    return out
for t in ('all', 'i5'):
    n0, fu = fate(f'{t}-n0-320'), fate(f'{t}-full-320')
    c = collections.Counter((n0[k][0], fu[k][0]) for k in n0)
    print('==', t)
    for (x, y), v in sorted(c.items(), key=lambda z: -z[1]): print(f'  {v:4}  n0 {x:34} -> full {y}')
    print('  pairs standing or stated alone, in full:')
    for k in n0:
        if n0[k][0] in ('stated', 'claim graph: stands, not stated'):
            f = fu[k]
            print(f"    {k[0].split('/')[-1]:16} {k[1].split('/')[-1]:16} n0 n_in {n0[k][1]:4} rank {n0[k][2]} | full: {f[0]:30} n_in {f[1]:4} rank {f[2]} {[x[2] for x in f[3] if len(x)>2][:2]}")
