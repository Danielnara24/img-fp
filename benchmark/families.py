#!/usr/bin/env python3
"""Precision where siblings can meet: small families, and seeds among unrelated photos.

A generated corpus gives every seed ~90 copies, and a run over all of them hides most
false pairs between near-miss sibling photographs: the vocabulary is sized from the
corpus, so ~90 copies of each seed make it finest exactly around that seed's
descriptors, a sibling's matches fall into other words, and the pair is never met or
falls under the points bar (out/v45-k/RESULTS.md: IMGS-ALL joins 0 sibling photographs
in the full corpus and 5-11 at 1-2 copies a photo). A real library is the small-family
regime. So precision is scored here, on subsets of the generated corpora:

  n0          every seed's original alone
  n1-d1..d3   every original + 1 random variant of it (three draws)
  n2-d1..d3   every original + 2 random variants
  f           every original among the 9,285 found-corpus photos (unlabelled)

The full corpora stay what recall is read from (`analyse.py`).

Columns: P / R, pair precision and recall against the generated truth restricted to
the subset (SAME/CROP positive, PARTIAL left out); xpairs, false pairs between two
seeds' files; joined, seed pairs with any such pair (two different photographs
reported as one); orig, of them those whose two originals are paired directly;
found, pairs between a seed's file and a found photo (unlabelled, nearly all false);
keepX, DELETEs whose kept copy is another photograph's file.

Each run reads the analysis from `--cache` (the paths are the corpora's own, so a
cache an earlier run made serves; the first run makes it).

Usage:
  families.py run BINARY OUTDIR --cache CACHE [--corpus all i5] [--only n0 n1 ...] [-- img-fp args]
  families.py score OUTDIR [-v]          a row per subset and a summary per corpus
  families.py compare OUTDIR OUTDIR ...  the summaries side by side
"""
import argparse, collections, csv, glob, json, os, random, subprocess, sys

CORPORA = {'all': os.path.expanduser('~/Documents/IMGS-ALL'), 'i5': os.path.expanduser('~/Documents/IMGS5'),
           # the hard generation (IMGS-v2): same 364 seeds as IMGS-ALL, other transforms
           'all2': os.path.expanduser('~/Documents/IMGS-ALL2'),
           # Fresh seeds (session v56): INRIA Holidays, 1,491 photos in 500 scene groups, split
           # by group hash into a dev half and a held-out test half; 12 transforms a seed.
           'hdev': os.path.expanduser('~/Documents/HOLI-DEV'), 'htest': os.path.expanduser('~/Documents/HOLI-TEST')}
# Default corpora for `run`: the held-out half only on request.
DEFAULT = ['all', 'i5', 'all2', 'hdev']


def group_of(seed):
    """The sibling set a seed belongs to: photographs of one scene. Holidays names a
    photo by its group's four digits then its own two; the generated corpora's near-miss
    sets share a name less its trailing number (Segovia1, Segovia2)."""
    base = os.path.splitext(os.path.basename(seed))[0]
    if base.isdigit() and len(base) == 6:
        return seed.split('/')[0] + '/' + base[:4] if '/' in seed else base[:4]
    stem = base.rstrip('0123456789')
    return (seed.rsplit('/', 1)[0] + '/' if '/' in seed else '') + (stem or base)
FOUND = os.path.expanduser('~/Downloads/archive')
SUBSETS = ['n0', 'n1-d1', 'n1-d2', 'n1-d3', 'n2-d1', 'n2-d2', 'n2-d3', 'f']
# The whole corpus, on request only (`--only full`): ~90 copies a seed, the
# regime that hides siblings; what recall is read from.
EXTRA = ['full']
POSITIVE = ('SAME', 'CROP')


def families(corpus):
    """{seed: (original, [variants])} and {path: (seed, variant)}."""
    fam, man = {}, {}
    for r in csv.DictReader(open(os.path.join(corpus, 'derived', 'manifest.csv'))):
        man[r['path']] = (r['seed'], r['variant'])
        o, vs = fam.setdefault(r['seed'], [None, []])
        if r['variant'] == 'ORIGINAL':
            fam[r['seed']][0] = r['path']
        else:
            vs.append(r['path'])
    return fam, man


def subset(tag, fam, name):
    """The files of one subset: deterministic, by name."""
    if name == 'full':
        return [p for s in sorted(fam) for p in [fam[s][0]] + fam[s][1] if os.path.exists(p)]
    if name == 'f':
        found = sorted(glob.glob(os.path.join(FOUND, '**', '*.*'), recursive=True))
        return [fam[s][0] for s in sorted(fam)] + found
    n, d = (int(name[1]), name[4:]) if '-' in name else (0, '')
    rng = random.Random(f'{tag}-{n}-{d}')
    out = []
    for s in sorted(fam):
        o, vs = fam[s]
        out.append(o)
        out += rng.sample(sorted(v for v in vs if os.path.exists(v)), min(n, len(vs)))
    return out


def run(a):
    os.makedirs(a.out, exist_ok=True)
    for tag in a.corpus:
        fam, _ = families(CORPORA[tag])
        for name in a.only or SUBSETS:
            files = subset(tag, fam, name)
            lst = os.path.join(a.out, f'{tag}-{name}.list')
            open(lst, 'w').write('\n'.join(files) + '\n')
            rep = os.path.join(a.out, f'{tag}-{name}.json')
            r = subprocess.run([a.binary, '--from-file', lst, '-x', '*', '--cache', a.cache, '--no-last',
                                *a.extra, '-o', rep], capture_output=True)
            open(os.path.join(a.out, f'{tag}-{name}.log'), 'wb').write(r.stderr)
            if r.returncode not in (0, 2):
                print(tag, name, 'exit', r.returncode, r.stderr.decode()[-300:], file=sys.stderr)
            else:
                print(tag, name, len(files), 'files', file=sys.stderr)


def key(a, b):
    return (a, b) if a <= b else (b, a)


def truth(corpus, files):
    """{pair: relation} for the pairs.csv rows whose two files are both in `files`."""
    rel = {}
    with open(os.path.join(corpus, 'derived', 'pairs.csv')) as f:
        for r in csv.DictReader(f):
            if r['a'] in files and r['b'] in files:
                rel[key(r['a'], r['b'])] = r['relation']
    return rel


def score_one(rep, files, man, rel):
    d = json.load(open(rep))
    npos = sum(1 for v in rel.values() if v in POSITIVE)
    tp = fp = found = 0
    xpairs = 0
    joined, orig = collections.Counter(), set()
    for p in d['pairs']:
        a, b = p['a'], p['b']
        ka, kb = a in man, b in man
        if not (ka and kb):
            found += ka != kb  # a seed's file and a found photo; two found photos are unlabelled
            continue
        r = rel.get(key(a, b))
        if r == 'PARTIAL':
            continue
        if r in POSITIVE:
            tp += 1
            continue
        fp += 1
        (sa, va), (sb, vb) = man[a], man[b]
        if sa != sb:
            xpairs += 1
            joined[key(sa, sb)] += 1
            if va == vb == 'ORIGINAL':
                orig.add(key(sa, sb))
    keep_x = 0
    for g in d['groups']:
        for m in g['files']:
            k = m.get('kept_copy')
            if m.get('action') == 'DELETE' and k and m['path'] in man and k in man and man[k][0] != man[m['path']][0]:
                keep_x += 1
    # n0 and f hold no positive pair: precision and recall say nothing there.
    P = tp / (tp + fp) if npos and tp + fp else None
    R = tp / npos if npos else None
    return dict(files=len(files), P=P, R=R, tp=tp, fp=fp, xpairs=xpairs, joined=len(joined), orig=len(orig),
                big=sum(1 for v in joined.values() if v >= 300), found=found, keepX=keep_x,
                sib=sum(1 for a, b in joined if group_of(a) == group_of(b)),
                unrel=sum(1 for a, b in joined if group_of(a) != group_of(b)),
                which=sorted(f'{a}|{b}{"*" if (a, b) in orig else ""}' for a, b in joined))


def scores(out):
    """{corpus tag: {subset: row}} for the reports in `out`."""
    res = {}
    for tag, corpus in CORPORA.items():
        names = [n for n in SUBSETS + EXTRA if os.path.exists(os.path.join(out, f'{tag}-{n}.json'))]
        if not names:
            continue
        fam, man = families(corpus)
        lists = {n: [l for l in open(os.path.join(out, f'{tag}-{n}.list')).read().split('\n') if l] for n in names}
        rel = truth(corpus, set().union(*(set(l) for l in lists.values())))
        res[tag] = {}
        for n in names:
            fs = set(lists[n])
            sub = {k: v for k, v in rel.items() if k[0] in fs and k[1] in fs}
            res[tag][n] = score_one(os.path.join(out, f'{tag}-{n}.json'), fs, man, sub)
    return res


def summary(rows):
    """Per regime: mean over draws of joined / orig / P / R / keepX, and the total `joined`
    (n0 + mean n1 + mean n2 + f), the one figure to drive down."""
    out = {}
    for reg in ('n0', 'n1', 'n2', 'f', 'full'):
        rs = [r for n, r in rows.items() if n.split('-')[0] == reg]
        if rs:
            out[reg] = {k: (None if rs[0][k] is None else sum(r[k] for r in rs) / len(rs))
                        for k in ('joined', 'orig', 'P', 'R', 'keepX', 'found', 'xpairs', 'sib', 'unrel')}
    out['total'] = sum(v['joined'] for k, v in out.items() if k != 'full')
    out['total_sib'] = sum(v['sib'] for k, v in out.items() if k not in ('full', 'total'))
    return out


HEAD = f"{'subset':<8} {'files':>6} {'P%':>6} {'R%':>6} {'xpairs':>6} {'joined':>6} {'sib':>4} {'unrel':>5} {'orig':>4} {'>=300':>5} {'found':>5} {'keepX':>5}"


def pct(x, w=6):
    return f"{'-':>{w}}" if x is None else f'{100 * x:>{w}.2f}'


def score(a):
    res = scores(a.out)
    for tag, rows in res.items():
        print(f'== {tag}')
        print(HEAD)
        for n, r in rows.items():
            print(f"{n:<8} {r['files']:>6} {pct(r['P'])} {pct(r['R'])} {r['xpairs']:>6} {r['joined']:>6} {r['sib']:>4} {r['unrel']:>5} "
                  f"{r['orig']:>4} {r['big']:>5} {r['found']:>5} {r['keepX']:>5}")
            if a.v and r['which']:
                print('         ' + ', '.join(w.split('/')[-1].replace('.jpg', '') if '|' not in w else
                                          '|'.join(x.split('/')[-1] for x in w.split('|')) for w in r['which']))
        s = summary(rows)
        print('mean ' + '  '.join(f"{k}: joined {v['joined']:.1f} orig {v['orig']:.1f} P {pct(v['P'], 0)} R {pct(v['R'], 0)}"
                                  for k, v in s.items() if not k.startswith('total')) + f"  | total joined {s['total']:.1f} (siblings {s['total_sib']:.1f})")


def compare(a):
    res = [scores(o) for o in a.outs]
    for tag in CORPORA:
        if not all(tag in r for r in res):
            continue
        print(f'== {tag}')
        ss = [summary(r[tag]) for r in res]
        print(f"{'':<6}" + ''.join(f'{os.path.basename(os.path.normpath(o)):>34}' for o in a.outs))
        for reg in ('n0', 'n1', 'n2', 'f', 'full'):
            if all(reg in s for s in ss):
                print(f'{reg:<6}' + ''.join(f"{s[reg]['joined']:>6.1f} j {s[reg]['orig']:>4.1f} o "
                                            f"{pct(s[reg]['P'])} P {pct(s[reg]['R'])} R" for s in ss))
        print(f"{'total':<6}" + ''.join(f"{s['total']:>8.1f} joined{'':>20}" for s in ss))
        print(f"{'sibs':<6}" + ''.join(f"{s['total_sib']:>8.1f} joined{'':>20}" for s in ss))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest='cmd', required=True)
    r = sp.add_parser('run')
    r.add_argument('binary')
    r.add_argument('out')
    r.add_argument('--cache', required=True)
    r.add_argument('--corpus', nargs='+', default=DEFAULT, choices=list(CORPORA))
    r.add_argument('--only', nargs='+', choices=SUBSETS + EXTRA)
    s = sp.add_parser('score')
    s.add_argument('out')
    s.add_argument('-v', action='store_true')
    c = sp.add_parser('compare')
    c.add_argument('outs', nargs='+')
    argv = sys.argv[1:]
    extra = argv[argv.index('--') + 1:] if '--' in argv else []
    a = ap.parse_args(argv[:len(argv) - len(extra) - (1 if '--' in argv else 0)])
    a.extra = extra
    if a.cmd == 'run':
        run(a)
    elif a.cmd == 'score':
        score(a)
    else:
        compare(a)


if __name__ == '__main__':
    main()
