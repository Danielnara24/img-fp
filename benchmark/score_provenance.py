#!/usr/bin/env python3
"""Score an img-fp JSON report of one IMGS-PROVENANCE tree (`corpus/make_provenance.py`).

Per family, of an original and its lossy or edited copies:
  original kept      the original is not DELETE
  faithful kept      the original or a copy of its whole frame at its size that is not an edit
                     (stripped, reencode_*) is not DELETE
  content lost       a DELETE whose content no surviving file holds: a full-size full-frame file
                     holds everything, any full-frame file holds `half`
and, for families whose original was deleted, which variants were kept instead.

Usage: score_provenance.py REPORT.json [--manifest IMGS-PROVENANCE/manifest.csv]
"""
import argparse, collections, csv, json, os

FULL = {'original', 'stripped', 'reencode_q85', 'reencode_q90', 'reencode_q92', 'reencode_q97', 'noise', 'sharpen'}
FAITHFUL = {'original', 'stripped', 'reencode_q85', 'reencode_q90', 'reencode_q92', 'reencode_q97'}


def holds(k, d):
    if k in FULL:
        return True
    return k == 'crop90' and d == 'crop90' or k == 'half' and d == 'half'


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('report')
    ap.add_argument('--manifest', default=os.path.expanduser('~/Documents/IMGS-PROVENANCE/manifest.csv'))
    a = ap.parse_args()
    man = {os.path.realpath(r['path']): r for r in csv.DictReader(open(a.manifest))}
    rep = json.load(open(a.report))
    action = {}
    for g in rep['groups']:
        for f in g['files']:
            p = os.path.realpath(bytes(f['path_bytes']).decode() if f.get('path_bytes') else f['path'])
            action[p] = f.get('action')
    trees = {man[p]['tree'] for p in action if p in man}
    if len(trees) != 1:
        raise SystemExit(f'the report should cover one tree of the manifest, not {sorted(trees)}')
    fams = collections.defaultdict(dict)
    for p, r in man.items():
        if r['tree'] in trees:
            fams[(r['tree'], r['family'])][r['variant']] = action.get(p)
    n = len(fams)
    orig = faithful = lost = deletes = 0
    kept_instead = collections.Counter()
    lost_ex = []
    for fam, v in sorted(fams.items()):
        live = {k for k, x in v.items() if x != 'DELETE'}
        deletes += sum(1 for x in v.values() if x == 'DELETE')
        orig += 'original' in live
        faithful += bool(live & FAITHFUL)
        if 'original' not in live:
            kept_instead[','.join(sorted(k for k in live if v[k] == 'KEEP')) or '(none KEEP)'] += 1
        for d, x in v.items():
            if x == 'DELETE' and not any(holds(k, d) for k in live):
                lost += 1
                lost_ex.append(f'{fam[1]}/{d}')
    print(f'families: {n}   DELETE: {deletes}')
    print(f'original kept:   {orig} / {n}')
    print(f'faithful kept:   {faithful} / {n}')
    print(f'content lost:    {lost} deletions' + (f'  e.g. {lost_ex[:5]}' if lost_ex else ''))
    for k, c in kept_instead.most_common():
        print(f'  original deleted, kept instead: {k}: {c}')


if __name__ == '__main__':
    main()
