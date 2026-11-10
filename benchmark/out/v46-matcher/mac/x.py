import csv, sys, collections
man = {r['path']: (r['seed'], r['variant']) for r in csv.DictReader(open('/home/daniel/Documents/IMGS5/derived/manifest.csv'))}
S = {'Macizo1.jpg', 'Macizo2.jpg'}
for f in sys.argv[1:]:
    rows = []
    kinds = collections.Counter()
    for r in csv.DictReader(open(f)):
        kinds[r['kind']] += 1
        a, b = man.get(r['a']), man.get(r['b'])
        if not a or not b: continue
        if {a[0], b[0]} == S:
            rows.append((r, a, b))
    print('==', f, dict(kinds))
    anch = [x for x in rows if x[0]['kind'] != 'propagated' and int(x[0]['n_in']) >= 10 and float(x[0]['blk']) >= 0.7 and max(float(x[0]['ov_a']), float(x[0]['ov_b'])) >= 0.85 and x[0]['centred'] == '1']
    print(' cross verdicts', len(rows), 'anchor-like', len(anch), 'clean-like', sum(float(x[0]['blk_min']) >= 0.9 for x in anch))
    for r, a, b in sorted(anch, key=lambda x: -float(x[0]['blk_min']))[:12]:
        print(f"  {r['kind']:<10} {a[0]}/{a[1]:<16} {b[0]}/{b[1]:<16} n_in {r['n_in']:>4} ov {float(r['ov_a']):.2f}/{float(r['ov_b']):.2f} blk {float(r['blk']):.3f} min {float(r['blk_min']):.3f}")
