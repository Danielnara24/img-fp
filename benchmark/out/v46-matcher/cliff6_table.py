import re
V44 = '../v44-flat-contrast'
steps = ['default'] + 'points8 points6 points5 points4 points3 overlap0.75 overlap0.65 overlap0.6 overlap0.5 overlap0.4 overlap0.3 overlap0.25 corr0.9 corr0.8 corr0.6 corr0.5 corr0.4 corr0.3 corr0.2 k100 k50 k25 k10'.split()
def read(p):
    s = open(p).read()
    m = re.search(r'F1 ([0-9.]+)\s+precision ([0-9.]+)%\s+recall ([0-9.]+)%', s)
    cf = re.search(r'cross-family ([0-9,]+)', s)
    merges = [(re.sub(r'\.\w+$', '', a.split('/')[-1]) + '/' + re.sub(r'\.\w+$', '', b.split('/')[-1]).replace(re.sub(r'\d+\.\w+$', '', b.split('/')[-1]), ''), int(n.replace(',', '')))
              for a, b, n in re.findall(r'cross (.+?) <-> (.+?): ([0-9,]+) pairs', s)]
    big = [f'{k} {n:,}' for k, n in merges if n >= 300]
    top = max([n for _, n in merges], default=0)
    return float(m[1]), float(m[2]), float(m[3]), int(cf[1].replace(',', '')), big, top
for t in ['all', 'i5']:
    print(f'== {"IMGS-ALL" if t == "all" else "IMGS5"}  (0.72.0 | stack)')
    print(f'{"step":>12} {"F1":>15} {"precision":>15} {"recall":>15} {"cross-family":>15}  merges >= 300 (largest below if none)')
    for s in steps:
        a = read(f'{V44}/{t}-ssim-{s}.txt'); b = read(f'cliff6/{t}-{s}.txt')
        mg = lambda x: ', '.join(x[4]) if x[4] else f'none (largest {x[5]})'
        print(f'{s:>12} {a[0]:.4f}|{b[0]:.4f} {a[1]:6.2f}|{b[1]:6.2f} {a[2]:6.2f}|{b[2]:6.2f} {a[3]:>7,}|{b[3]:<7,}  {len(a[4])}|{len(b[4])}  0.72.0: {mg(a)}  ||  stack: {mg(b)}')
    print()
c = read('cliff6/check-0720-i5-overlap0.6.txt'); v = read(f'{V44}/i5-ssim-overlap0.6.txt')
print('check, 0.72.0 i5 overlap 0.6 from the list vs v44:', c[:4], v[:4])
