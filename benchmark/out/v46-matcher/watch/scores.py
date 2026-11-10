import sys, re
sys.argv=['x']; exec(open('watch/cross.py').read().split("for t in ('all', 'i5'):")[0])
def sc(rs):
    v=[(float(re.search(r'score ([\d.]+)', '\t'.join(x)).group(1)), float(re.search(r'top ([\d.]+)', '\t'.join(x)).group(1)), int(x[1]), int(x[2].split()[1])) for x in rs if x[1].isdigit()]
    return max(v) if v else None
for t in ('all', 'i5'):
    n0, fu = fate(f'{t}-n0-320'), fate(f'{t}-full-320')
    print('==', t, ' score alone -> full (rank, of)')
    for k in n0:
        if n0[k][0] in ('stated', 'claim graph: stands, not stated'):
            a, b = sc(n0[k][3]), sc(fu[k][3])
            print(f"  {k[0].split('/')[-1]:18} {k[1].split('/')[-1]:18} {a[0]:.4f} (rank {a[2]:4} of {a[3]:5}) -> " + (f"{b[0]:.4f} (rank {b[2]:4} of {b[3]:5})  x{b[0]/a[0]:.2f}" if b else 'no shared word'))
