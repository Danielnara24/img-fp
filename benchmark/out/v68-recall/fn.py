#!/usr/bin/env python3
"""fn.py BINARY CORPUS OUT [-v REASON]: run BINARY (built with the v68 trace) over CORPUS from the session cache, and say
for every file that stays though a file that stays holds it (the truth's `holds`) why the best-placed holder refused."""
import subprocess, sys, os, csv, json, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
binary, corpus, out = sys.argv[1:4]
cache = 'benchmark/out/v67-keep/cache/c.db'
if not os.path.exists(out + '.json') or '--rerun' in sys.argv:
    with open(out + '.ref', 'w') as err:
        subprocess.run([binary, corpus, '-r', '-x', '*', '-t', '8', '--no-last', '--cache', cache, '-o', out + '.json'], env={**os.environ, 'IMGFP_REF': '1'}, stderr=err)
man = {r['path']: r for r in csv.DictReader(open(os.path.join(corpus, 'derived', 'manifest.csv')))}
G = ss.build({p: {'path': p} for p in man}, man)
act = {}
for g in json.load(open(out + '.json'))['groups']:
    for f in g['files']: act[f['path']] = f.get('action')
idx = {}; ref = collections.defaultdict(dict)
for l in open(out + '.ref', errors='replace'):
    if l.startswith('IDX '): i, p = l[4:].rstrip('\n').split(' ', 1); idx[int(i)] = p
for l in open(out + '.ref', errors='replace'):
    if l.startswith('REF '):
        y, x, rest = l.rstrip('\n').split(' ', 3)[1:]
        ref[idx[int(y)]][idx[int(x)]] = rest
ORDER = ['MAY', 'undecided', 'held', 'softer', 'carries', 'frames', 'whole:one-frame', 'whole:strict-in', 'corners-one-frame,pixels-not', 'in-but-not-past', 'composite-rule',
         'smaller-and-extra-detail', 'not-in:limbo', 'not-in:past', 'shape-other', 'unusable:weak', 'unusable:other', 'unusable:mirrored', 'no side', 'no?']
def label(rest):
    w = rest.split()[0]
    return w[5:] if w.startswith('made:') else w
seeds = collections.defaultdict(list)
for p in G: seeds[G[p]['seed']].append(p)
kind = lambda g: 'trap' if g['trap'] else 'composite' if g['host'] else 'edit' if g['edited'] else 'unedited'
why = collections.Counter(); byk = collections.defaultdict(collections.Counter); ex = collections.defaultdict(list)
ndel = sum(a == 'DELETE' for p, a in act.items() if p in G)
for s, ps in seeds.items():
    stay = [p for p in ps if act.get(p) != 'DELETE']
    for p in stay:
        hs = [k for k in stay if k != p and ss.holds(G[k], G[p]) and not (ss.holds(G[p], G[k]) and p < k)]
        if not hs: continue
        ls = [(label(ref[p][k]), k) for k in hs if k in ref[p]]
        if ls: w, k = min(ls, key=lambda t: ORDER.index(t[0]) if t[0] in ORDER else 99)
        else:
            w, k = ('never grouped' if p not in act else 'no pair with a holder that stays'), None
        why[w] += 1; byk[w][('edit:' if G[p]['edited'] else '') + G[p]['var']] += 1
        ex[w].append((p, k))
print(f"{corpus}: {ndel} deleted; {sum(why.values())} more stay though a file that stays holds them:")
for w, c in why.most_common(): print(f"{c:6d} {w:45s} {byk[w].most_common(7)}")
if '-v' in sys.argv:
    w = sys.argv[sys.argv.index('-v') + 1]
    for p, k in ex[w][:int(os.environ.get('N', 25))]:
        print(f"  {man[p]['variant']:18s} {man[p]['width']}x{man[p]['height']} <- {man[k]['variant'] if k else None} {man[k]['width'] + 'x' + man[k]['height'] if k else ''}  {ref[p].get(k, '')}   {os.path.basename(p)} | {man[p]['seed']}")
