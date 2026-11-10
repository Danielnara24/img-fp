import json, csv, sys, collections
man = {r['path']: r for r in csv.DictReader(open(sys.argv[1]))}
KEEPV = {'ORIGINAL', 'copy_exact', 'strip_metadata', 'to_png', 'to_tiff', 'webp_lossless'}
def kept(p):
    r = json.load(open(p))
    act = {}
    for g in r['groups']:
        for f in g['files']:
            act[f['path']] = f.get('action')
    seeds = collections.defaultdict(list)
    for p, m in man.items(): seeds[m['seed']].append(p)
    ok = {s for s, ps in seeds.items() if any(man[p]['variant'] in KEEPV and act.get(p) != 'DELETE' for p in ps)}
    return ok, act
b, ab = kept(sys.argv[2])
for p in sys.argv[3:]:
    o, ao = kept(p)
    for s in sorted(b - o):
        print(p, 'lost', s, [(man[q]['variant'], ab.get(q), ao.get(q)) for q in man if man[q]['seed'] == s and man[q]['variant'] in KEEPV])
    for s in sorted(o - b): print(p, 'gained', s)
