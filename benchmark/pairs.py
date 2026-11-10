#!/usr/bin/env python3
"""The keep rule on two-file families: every seed's original and one variant of it.

IMGS-ALL's families are an original and ninety variants, so the keep rule always
has a majority to choose by (the grey middle, the softness median), and on IMGS-ALL
no deletion loses content. Real libraries are mostly pairs: a photograph and a
re-encode, an edit, a crop. This rebuilds that regime from IMGS-ALL itself: for each
transform, a run over every seed's original and its one variant of that transform,
so every family is two files, scored with `suggest_score.holds` against the generated
ground truth. 0.49.0 lost content in 2,241 of 27,355 pairs.

Each run reads the analysis from a cache made by a run over IMGS-ALL (the paths are
IMGS-ALL's own, so its records serve), ~7 s a transform, ~11 min for all of them.
The near-miss corpora's sibling seeds still match each other within a run, which
is a few cross-seed pairs, scored like any other.

Usage:
  pairs.py run BINARY OUTDIR --cache CACHE [--only T ...]     one report per transform
  pairs.py score OUTDIR [-v]                                 a row per transform with
                                                             something deleted (-v: all)
  pairs.py compare OUTDIR OUTDIR ...                         side by side, rows that differ

Columns: pairs; origDEL, originals deleted; varDEL, variants deleted; loss, deletions
no surviving file of the pair holds (an edit standing in for its original, a
composite for the photograph); lossX, the same with regions read exactly rather
than with `suggest_score`'s 2% slack, which hid a crop missing up to 2% standing
in for its original; REVIEW, files left for a look.
"""
import argparse, collections, csv, json, os, subprocess, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import suggest_score as ss

MAN = os.path.expanduser('~/Documents/IMGS-ALL/derived/manifest.csv')


def load(manifest):
    rows = list(csv.DictReader(open(manifest)))
    man = {r['path']: r for r in rows}
    orig = {r['seed']: r['path'] for r in rows if r['variant'] == 'ORIGINAL'}
    by_var = collections.defaultdict(dict)
    for r in rows:
        if r['variant'] != 'ORIGINAL' and r['seed'] not in by_var[r['variant']] and os.path.exists(r['path']):
            by_var[r['variant']][r['seed']] = r['path']
    return man, orig, by_var


def run(a, man, orig, by_var):
    os.makedirs(a.out, exist_ok=True)
    for v in sorted(a.transforms or by_var):
        lst = os.path.join(a.out, v + '.list')
        with open(lst, 'w') as f:
            for seed, p in by_var[v].items():
                f.write(orig[seed] + '\n' + p + '\n')
        r = subprocess.run([a.binary, '--from-file', lst, '-x', '*', '--cache', a.cache, '--no-last',
                            '-o', os.path.join(a.out, v + '.json')], capture_output=True)
        if r.returncode not in (0, 2):
            print(v, 'exit', r.returncode, r.stderr.decode()[-300:], file=sys.stderr)


def table(out, man, orig, by_var):
    """{transform: Counter} for the reports in `out`."""
    res = {}
    for v in sorted(by_var):
        p = os.path.join(out, v + '.json')
        if not os.path.exists(p):
            continue
        act = {f['path']: f.get('action') for g in json.load(open(p))['groups'] for f in g['files']}
        G = ss.build({q: {'path': q} for q in list(act) + list(orig.values())}, man)
        c = collections.Counter()
        for seed, vp in by_var[v].items():
            o = orig[seed]
            c['pairs'] += 1
            ao, av = act.get(o), act.get(vp)
            c['origDEL'] += ao == 'DELETE'
            c['varDEL'] += av == 'DELETE'
            c['REVIEW'] += (ao == 'REVIEW') + (av == 'REVIEW')
            for x, y, ax in ((o, vp, ao), (vp, o, av)):
                if ax == 'DELETE' and not ss.holds(G.get(y), G.get(x)):
                    c['loss'] += 1
                if ax == 'DELETE' and not ss.holds(G.get(y), G.get(x), ss.EXACT):
                    c['lossX'] += 1
        res[v] = c
    return res


COLS = ['origDEL', 'varDEL', 'loss', 'lossX', 'REVIEW']


def score(a, man, orig, by_var):
    t = table(a.out, man, orig, by_var)
    tot = collections.Counter()
    print(f"{'transform':22s} {'pairs':>5s} " + ' '.join(f'{c:>7s}' for c in COLS))
    for v, c in t.items():
        tot.update(c)
        if a.v or c['origDEL'] or c['loss'] or c['lossX']:
            print(f"{v:22s} {c['pairs']:5d} " + ' '.join(f'{c[k]:7d}' for k in COLS))
    print(f"{'TOTAL':22s} {tot['pairs']:5d} " + ' '.join(f'{tot[k]:7d}' for k in COLS))


def compare(a, man, orig, by_var):
    ts = [table(o, man, orig, by_var) for o in a.outs]
    names = [os.path.basename(o.rstrip('/')) for o in a.outs]
    print(f"{'transform':22s} " + '  '.join(f'{n:>29s}' for n in names))
    print(f"{'':22s} " + '  '.join(f"{'oDEL  vDEL  loss lossX   REV':>29s}" for _ in names))
    tots = [collections.Counter() for _ in ts]
    for v in sorted(by_var):
        rows = [t.get(v, collections.Counter()) for t, tot in zip(ts, tots)]
        for r, tot in zip(rows, tots):
            tot.update(r)
        if len({tuple(r[k] for k in COLS) for r in rows}) > 1:
            print(f'{v:22s} ' + '  '.join(' '.join(f'{r[k]:5d}' for k in COLS) for r in rows))
    print(f"{'TOTAL':22s} " + '  '.join(' '.join(f'{r[k]:5d}' for k in COLS) for r in tots))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--manifest', default=MAN)
    sub = ap.add_subparsers(dest='cmd', required=True)
    r = sub.add_parser('run')
    r.add_argument('binary')
    r.add_argument('out')
    r.add_argument('--cache', required=True)
    r.add_argument('--only', nargs='+', dest='transforms', metavar='TRANSFORM', help='these transforms alone')
    s = sub.add_parser('score')
    s.add_argument('out')
    s.add_argument('-v', action='store_true')
    c = sub.add_parser('compare')
    c.add_argument('outs', nargs='+')
    a = ap.parse_args()
    man, orig, by_var = load(a.manifest)
    {'run': run, 'score': score, 'compare': compare}[a.cmd](a, man, orig, by_var)


if __name__ == '__main__':
    main()
