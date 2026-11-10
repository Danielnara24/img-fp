#!/usr/bin/env python3
"""Each watched sibling pair, per run: where it is lost. watch/why.py RUN... (e.g. all-n0-320)"""
import json, sys, re, collections
sib = [tuple(l.rstrip('\n').split('\t')) for l in open('watch/sib.tsv')]
def key(a, b): return (a, b) if a <= b else (b, a)
def load(run):
    rank, ver = collections.defaultdict(list), collections.defaultdict(list)
    for l in open(f'watch/{run}.log', errors='replace'):
        if not l.startswith('WATCH\t/'): continue
        f = l.rstrip('\n').split('\t')
        k = key(f[1], f[2])
        if f[3] == 'rank': rank[k].append(f[4:])
        else: ver[k].append(f[3:])
    rep = json.load(open(f'watch/{run}.json'))
    stated = {key(p['a'], p['b']) for p in rep['pairs']}
    files = set(open(f"g320/{run.split('-')[0]}-{run.split('-')[1]}.list").read().split('\n'))
    return rank, ver, stated, files
def why(k, rank, ver, stated):
    same = [v for v in ver[k] if 'mirror: false, invert: false' in v[0] or v[0] == 'SAME']
    if k in stated: return 'stated', same
    if not same:
        rs = rank[k]
        if not rs: return 'retrieval: never scored', same
        if all(r[1] == 'untouched' for r in rs): return 'retrieval: no shared word', same
        return 'retrieval: ranked out', same
    best = max(same, key=lambda v: int(re.search(r'n_in (\d+)', '\t'.join(v)).group(1)) if 'n_in' in '\t'.join(v) else -1)
    t = '\t'.join(best)
    if 'n_match' not in t: return 'start: <3 shared', same
    vocab = int(re.search(r'vocab (\d+)', t).group(1))
    if vocab < 3: return 'start: <3 agreeing', same
    if t.endswith('Rejected'): return 'verdict: rejected', same
    return 'claim graph: stands, not stated', same
for run in sys.argv[1:]:
    rank, ver, stated, files = load(run)
    ks = [key(a, b) for a, b in sib if a in files and b in files]
    c = collections.Counter(why(k, rank, ver, stated)[0] for k in ks)
    print(run, len(ks), dict(sorted(c.items())))
