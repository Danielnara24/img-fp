#!/usr/bin/env python3
"""small.py BINARY NAME [ENV=V ...]: the keep rule where families are an original and one or two variants
(`v51-families` lists, both corpora, three draws each): deletions, and those no file that stays holds."""
import subprocess, sys, os, csv, json, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
binary, name = sys.argv[1:3]; env = dict(a.split('=', 1) for a in sys.argv[3:])
V = os.path.dirname(os.path.abspath(__file__)); L = os.path.join(V, '..', 'v51-families'); D = os.path.expanduser('~/Documents')
os.makedirs(f'{V}/{name}', exist_ok=True)
tot = collections.Counter(); bad = collections.Counter()
for tag, corpus in (('all2', 'IMGS-ALL2'), ('all', 'IMGS-ALL')):
    man = {r['path']: r for r in csv.DictReader(open(f'{D}/{corpus}/derived/manifest.csv'))}
    G = ss.build({p: {'path': p} for p in man}, man)
    for sub in ('n1-d1', 'n1-d2', 'n1-d3', 'n2-d1', 'n2-d2', 'n2-d3'):
        out = f'{V}/{name}/{tag}-{sub}.json'
        if not os.path.exists(out):
            subprocess.run([binary, '--from-file', f'{L}/{tag}-{sub}.list', '-x', '*', '-t', '8', '--no-last', '--cache', f'{V}/../v67-keep/cache/c.db', '-o', out], env={**os.environ, **env}, capture_output=True)
        act = {}
        for g in json.load(open(out))['groups']:
            for f in g['files']:
                if f['path'] in G: act[f['path']] = f.get('action')
        lo, ex = ss.losses(act, G, None)
        tot[tag + ' ' + sub[:2]] += sum(a == 'DELETE' for a in act.values()); bad[tag + ' ' + sub[:2]] += sum(lo.values())
        for why, vs in ex.items():
            for v in vs: bad[f'   {tag} {sub} {why}: {v}'] += 1
print(name, ' '.join(f"{k}: {tot[k]} deleted, {bad[k]} lost;" for k in tot))
for k, v in bad.items():
    if k.startswith('   '): print(k, v)
