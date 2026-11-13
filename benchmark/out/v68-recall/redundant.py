#!/usr/bin/env python3
"""redundant.py MANIFEST REPORT...: the keep rule as a classifier of redundant files.

A file is redundant when it can go with nothing lost: some file that stays holds it (`suggest_score.holds`).
Which files can all go at once is a choice of what stays, so the ideal is a smallest set of files that between
them hold every other (greedy, per seed: the file holding most of what is not yet held, until all is; the
true smallest set is no larger, so recall here is at most a little high). Then, of a report's actions:
  TP  deleted, and a file that stays holds it        FP  deleted, and none does (content lost)
  FN  not deleted, though the ideal deletes that many more     TN  the rest
`strict`: the same with composites and mirrored or turned copies holding nothing but their own bytes."""
import json, csv, sys, os, collections
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))
import suggest_score as ss
man = {r['path']: r for r in csv.DictReader(open(sys.argv[1]))}
G = ss.build({p: {'path': p} for p in man}, man)
seeds = collections.defaultdict(list)
for p, g in G.items(): seeds[g['seed']].append(p)
kind = lambda g: 'trap' if g['trap'] else 'composite' if g['host'] else 'edit' if g['edited'] else 'unedited'
def H(k, d, strict):
    if k == d: return False
    if not ss.holds(G[k], G[d]): return False
    return not (strict and G[k]['sha'] != G[d]['sha'] and (G[k]['host'] or G[k]['var'] in ss.GEOM))
def ideal(strict):
    """{file: True if the smallest keep set deletes it}."""
    gone = {}
    for s, ps in seeds.items():
        holds = {k: {d for d in ps if H(k, d, strict)} for k in ps}
        left, kept = set(ps), []
        while left:
            k = max(left, key=lambda k: (len(holds[k] & left), -ps.index(k)))
            kept.append(k); left -= holds[k]; left.discard(k)
        for p in ps: gone[p] = p not in kept
    return gone
for strict in ((False, True) if "--strict" in sys.argv else (False,)):
    gone = ideal(strict)
    print(f"\n{'STRICT: ' if strict else ''}files {len(G)}, redundant (the ideal deletes) {sum(gone.values())}, must stay {len(G) - sum(gone.values())}")
    for rep in [a for a in sys.argv[2:] if a != "--strict"]:
        act = {}
        for g in json.load(open(rep))['groups']:
            for f in g['files']:
                if f['path'] in G: act[f['path']] = f.get('action')
        stay = collections.defaultdict(list)
        for p in G:
            if act.get(p) != 'DELETE': stay[G[p]['seed']].append(p)
        tp = collections.Counter(); fp = collections.Counter(); red = collections.Counter()
        for p in G:
            if gone[p]: red[kind(G[p])] += 1
            if act.get(p) == 'DELETE':
                (tp if any(H(k, p, strict) for k in stay[G[p]['seed']]) else fp)[kind(G[p])] += 1
        TP, FP, R = sum(tp.values()), sum(fp.values()), sum(red.values()); FN = R - TP; TN = len(G) - TP - FP - FN
        print(f"  {rep.split('/')[-2]:5s} TP {TP:6d}  FP {FP:4d}  FN {FN:6d}  TN {TN:6d}   precision {TP/(TP+FP):.4%}  recall {TP/R:.2%}")
        print('        recall by kind: ' + ', '.join(f"{k} {tp[k]}/{red[k]} ({tp[k]/max(red[k],1):.1%})" for k in ['unedited', 'edit', 'composite', 'trap']) + (f"   FP by kind {dict(fp)}" if FP else ''))
