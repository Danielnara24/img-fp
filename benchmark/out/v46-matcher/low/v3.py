import sys, collections, os
dump, corpus, corr = sys.argv[1], sys.argv[2], sys.argv[3]
os.environ['CORPUS'] = corpus
sys.argv = ['x', dump, 'none', 'none', corr]
src = open('hen/replay3.py').read(); src = src[:src.index("base = [e['clean']")]
exec(src)
# v3: the part keeps the cluster with most links; own cluster only on a tie
def part_rule3(clean):
    c = comps([e for e, k in zip(edges, clean) if k and one_way(e) is None])
    votes = collections.defaultdict(collections.Counter)
    for e, k in zip(edges, clean):
        if k and one_way(e):
            for x, y in ((e['a'], e['b']), (e['b'], e['a'])):
                if c[x][2] == 1 and c[y][2] > 1: votes[x][c[y]] += 1
    c = dict(c)
    for x, v in votes.items(): c[x] = max(v.items(), key=lambda t: (t[1], -t[0][1]))[0]
    by = collections.defaultdict(list)
    for k, e in enumerate(edges):
        if clean[k] and (ow := one_way(e)): by[ow[0]].append((k, c[ow[1]]))
    out = list(clean)
    for part, links in by.items():
        cnt = {}
        for _, cl in links:
            x = cnt.setdefault(cl[0], [0, cl[1]]); x[0] += 1
        if len(cnt) < 2: continue
        own = c[part][0]
        keep = max(cnt.items(), key=lambda t: (t[1][0], t[0] == own, -t[1][1]))[0]
        for k, cl in links:
            if cl[0] != keep: out[k] = False
    return out
base = [e['clean'] for e in edges]
for tag, rule in (('v2', lambda b: part_rule(b, True)), ('v3', part_rule3)):
    p = rule(base)
    dem = [k for k in range(len(edges)) if base[k] and not p[k]]
    same = sum(1 for k in dem if fam(edges[k]['a']) == fam(edges[k]['b']))
    print(f'{tag}: demoted {len(dem):,}  same-family {same:,}  cross-family {len(dem) - same:,}')
    merged(bridges(p), tag)
print('off:'); merged(bridges(base), 'off')
if 'IMGS-ALL' in corpus:
    for tag, rule in (('v2', lambda b: part_rule(b, True)), ('v3', part_rule3)):
        fin = bridges(rule(base)); c2 = comps([e for e, k in zip(edges, fin) if k])
        ll = collections.Counter(c2[i][2] for i in range(n) if fam(i) == 'IMGS/low-light.avif')
        sizes = collections.Counter(c2[i][2] for i in range(n) if fam(i) == 'IMGS/low-light.avif')
        print(tag, 'low-light files by clean-cluster size after the rule:', dict(sorted(sizes.items(), reverse=True)))
        dem = collections.Counter(fam(edges[k]['a']).split('/')[-1] for k in range(len(edges)) if base[k] and not rule(base)[k]) if False else None
    for tag, rule in (('v2', lambda b: part_rule(b, True)), ('v3', part_rule3)):
        p = rule(base)
        by = collections.Counter(fam(edges[k]['a']).split('/')[-1] for k in range(len(edges)) if base[k] and not p[k])
        print(tag, 'demotions by family, top:', by.most_common(8))
