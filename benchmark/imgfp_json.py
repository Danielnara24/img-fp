"""An img-fp JSON report's pairs, every one, as a reader of the report counts them.

Since 2026-11-16 a set of identical files is stated once, in the report's
`identical` list, and `pairs` names only its first file: every other file of
the set has the same pairs with its own name in place, and every two files of
the set are an `identical` pair. `pairs(doc)` gives them all back, each a dict
as `pairs` writes one; a report from before (no `identical`) is read as it is.
"""

def pairs(doc):
    members = {}
    for s in doc.get('identical') or []:
        names = [f['name'] for f in s]
        for n in names:
            members[n] = names
        for i, a in enumerate(names):
            for b in names[i + 1:]:
                yield {'a': a, 'b': b, 'aligned_points': 0, 'frame_overlap': 1.0, 'pixel_correlation': 1.0, 'scale': 1.0, 'identical': True}
    for p in doc.get('pairs') or []:
        for a in members.get(p['a'], (p['a'],)):
            for b in members.get(p['b'], (p['b'],)):
                if a == p['a'] and b == p['b']:
                    yield p
                else:
                    yield dict(p, a=a, b=b)
