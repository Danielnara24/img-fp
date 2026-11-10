"""Seed-original pairs stated with seeds alone (n0) against among the found photos (f):
how many each states, how many both, and of those whose evidence is identical."""
import json, sys
def pairs(p, keep):
    d = json.load(open(p)); out = {}
    for r in d['pairs']:
        if r['a'] in keep and r['b'] in keep:
            out[tuple(sorted((r['a'], r['b'])))] = (r['aligned_points'], r['frame_overlap'], r['pixel_correlation'], r['scale'])
    return out
for cfg in sys.argv[1:]:
    for tag in ('hdev', 'all', 'all2', 'i5'):
        seeds = set(l for l in open(f'{cfg}/{tag}-n0.list').read().split('\n') if l)
        a, b = pairs(f'{cfg}/{tag}-n0.json', seeds), pairs(f'{cfg}/{tag}-f.json', seeds)
        both = a.keys() & b.keys()
        same = sum(a[k] == b[k] for k in both)
        print(f'{cfg:<9} {tag:<5} alone {len(a):>4}  among found {len(b):>4}  both {len(both):>4}  same evidence {same:>4}  only alone {len(a.keys()-b.keys()):>3}  only found {len(b.keys()-a.keys()):>3}')
