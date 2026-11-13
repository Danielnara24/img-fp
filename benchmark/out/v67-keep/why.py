#!/usr/bin/env python3
"""why.py LIST CACHE MANIFEST: files HEAD's chains delete and direct witnesses do not, and each kept file's refusal."""
import subprocess, sys, os, csv, json, collections
lst, cache, manf = sys.argv[1:4]
man = {r['path']: r for r in csv.DictReader(open(manf))}
B = 'target/iter/release/img-fp'
def run(env, B=B):
    r = subprocess.run([B, '--from-file', lst, '-x', '*', '--cache', cache, '--no-last', '-o', '/tmp/claude-1000/why.json'], env={**os.environ, **env}, capture_output=True)
    rep = json.load(open('/tmp/claude-1000/why.json')); act = {}
    for g in rep['groups']:
        for f in g['files']: act[f['path']] = f['action']
    return act, r.stderr.decode(errors='replace')
os.makedirs('/tmp/claude-1000', exist_ok=True)
base, _ = run({}, 'benchmark/out/v67-keep/base/img-fp')
new, err = run({'IMGFP_REF': '1', **dict(a.split('=') for a in sys.argv[4:])})
idx = {}; ref = collections.defaultdict(list)
for l in err.splitlines():
    w = l.split(' ', 2)
    if w[0] == 'IDX': idx[int(w[1])] = w[2]
    if w[0] == 'REF': y, x, rest = l.split(' ', 3)[1:]; ref[int(y)].append((int(x), rest))
name = lambda p: f"{man[p]['variant']} {man[p]['width']}x{man[p]['height']}" if p in man else p
print('base', dict(collections.Counter(base.values())), 'new', dict(collections.Counter(new.values())))
rid = {p: i for i, p in idx.items()}
for p in sorted(base, key=name):
    if base[p] == 'DELETE' and new.get(p) != 'DELETE':
        print(f'\n{name(p)}  -> {new.get(p)}')
        for x, rest in ref[rid[p]]: print(f'    {name(idx[x]):32s} {rest}')
