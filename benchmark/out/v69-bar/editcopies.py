#!/usr/bin/env python3
"""Edits that have copies of their own: where the one-frame bar first deletes an original for an edit.

For every IMGS-ALL seed and every edit of it that keeps its frame, a family of four: the original, a faithful
copy of it (`jpeg_high`), the edit, and a re-save of the edit (JPEG q90, made here into ~/Documents/IMGS-EDITCOPY).
Both the original and the edit then have a copy, which no other corpus has. One scan a kind of edit, as
`pairs.py` does, scored with `suggest_score.holds`: a deletion is wrong when no file that stays holds it.

  editcopies.py make                      write the re-saves and the lists
  editcopies.py run BINARY NAME [ENV=V]   one report a kind of edit
  editcopies.py score NAME...             deletions, wrong deletions, and which edits
"""
import sys, os, csv, json, collections, subprocess, hashlib
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
HERE = os.path.dirname(os.path.abspath(__file__)); D = os.path.expanduser('~/Documents')
OUT = f'{D}/IMGS-EDITCOPY'; MAN = f'{D}/IMGS-ALL/derived/manifest.csv'; CACHE = f'{HERE}/../v67-keep/cache/c.db'
rows = list(csv.DictReader(open(MAN)))
full = lambda r: r['region'] == '[0.0, 0.0, 1.0, 1.0]' and r['geometry'] == 'none' and float(r['coverage']) >= 0.999 and not r['scramble']
by = collections.defaultdict(dict)
for r in rows:
    if os.path.exists(r['path']): by[r['seed']].setdefault(r['variant'], r)
EDITS = sorted({r['variant'] for r in rows if r['variant'] in ss.EDITED and full(r)})
def resave(r): return f"{OUT}/{r['variant']}/{hashlib.sha1(r['path'].encode()).hexdigest()[:16]}.jpg"
def families(v):
    for seed, vs in by.items():
        o, c, e = vs.get('ORIGINAL'), vs.get('jpeg_high'), vs.get(v)
        if o and c and e and full(c) and (c['width'], c['height']) == (o['width'], o['height']) == (e['width'], e['height']):
            yield o, c, e, resave(e)
if sys.argv[1] == 'make':
    from PIL import Image
    n = 0
    for v in EDITS:
        os.makedirs(f'{OUT}/{v}', exist_ok=True)
        with open(f'{HERE}/{v}.list', 'w') as f:
            for o, c, e, p in families(v):
                if not os.path.exists(p):
                    try: Image.open(e['path']).convert('RGB').save(p, quality=90)
                    except Exception as x: print('skip', e['path'], x); continue
                f.write(f"{o['path']}\n{c['path']}\n{e['path']}\n{p}\n"); n += 1
    print(len(EDITS), 'kinds of edit,', n, 'families')
elif sys.argv[1] == 'run':
    binary, name = sys.argv[2:4]; env = dict(a.split('=', 1) for a in sys.argv[4:]); os.makedirs(f'{HERE}/{name}', exist_ok=True)
    for v in EDITS:
        subprocess.run([binary, '--from-file', f'{HERE}/{v}.list', '-x', '*', '-t', '8', '--no-last', '--cache', CACHE, '-o', f'{HERE}/{name}/{v}.json'], env={**os.environ, **env}, capture_output=True)
else:
    man = {r['path']: r for r in rows}
    for v in EDITS:
        for o, c, e, p in families(v): man[p] = {**e, 'path': p, 'sha256': 'resave:' + p}
    G = ss.build({p: {'path': p} for p in man}, man)
    for name in sys.argv[2:]:
        tot = collections.Counter(); bad = collections.Counter(); fams = collections.Counter()
        for v in EDITS:
            f = f'{HERE}/{name}/{v}.json'
            if not os.path.exists(f): continue
            act = {}
            for g in json.load(open(f))['groups']:
                for x in g['files']:
                    if x['path'] in G: act[x['path']] = x.get('action')
            stay = collections.defaultdict(list)
            for p, a in act.items():
                if a != 'DELETE': stay[G[p]['seed']].append(p)
            lost = set()
            for p, a in act.items():
                if a != 'DELETE': continue
                tot['edit' if G[p]['edited'] else 'original or its copy'] += 1
                if not any(ss.holds(G[k], G[p]) for k in stay[G[p]['seed']]): bad[v] += 1; lost.add(G[p]['seed'])
            fams[v] = len(lost)
        print(f"{name:10s} deleted: {tot['edit']:5d} edits, {tot['original or its copy']:5d} originals or their copies; wrong {sum(bad.values()):4d} in {sum(fams.values())} families  {bad.most_common(12)}")
