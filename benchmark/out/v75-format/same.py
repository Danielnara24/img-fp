"""Two img-fp JSON reports state the same pairs once identical sets are expanded:
each pair by its two names, with its points, overlap, correlation and flags
(scale and the order of a and b may turn with the pair). And the same groups."""
import json, sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import imgfp_json
FLAGS = ('mirrored', 'inverted', 'identical', 'same_pixels', 'propagated', 'corroborated')
def read(path):
    d = json.load(open(path))
    out = {}
    for p in imgfp_json.pairs(d):
        k = tuple(sorted((p['a'], p['b'])))
        v = (p['aligned_points'], p['frame_overlap'], p['pixel_correlation']) + tuple(bool(p.get(f)) for f in FLAGS)
        assert k not in out, ('twice', k)
        out[k] = v
    return d, out
(da, a), (db, b) = read(sys.argv[1]), read(sys.argv[2])
print(f'{sys.argv[1]}: {len(da["pairs"]):,} written, {len(a):,} stated; {sys.argv[2]}: {len(db["pairs"]):,} written, {len(b):,} stated, {len(db.get("identical", [])):,} identical sets')
diff = [k for k in a.keys() | b.keys() if a.get(k) != b.get(k)]
print('pairs differ:', len(diff), diff[:5])
key = 'groups' if 'groups' in da else 'files'
print(f'{key} equal:', da[key] == db[key])
print('sizes:', os.path.getsize(sys.argv[1]), os.path.getsize(sys.argv[2]))
