# Replay the claim graph's clean flags off one --dump: standing Clean, the
# shared-part rule (off / on), the bridge test; then which clean links join
# two seeds' families. Usage: replay.py DUMP SEED_A SEED_B [corr] [overlap] [points]
import csv, sys, collections
import networkx as nx
R = '/home/daniel/Documents/IMGS-ALL/'
dump, SA, SB = sys.argv[1:4]
corr = float(sys.argv[4]) if len(sys.argv) > 4 else 0.7
bar = float(sys.argv[5]) if len(sys.argv) > 5 else 0.85
pts = int(sys.argv[6]) if len(sys.argv) > 6 else 10
seed, var = {}, {}
for r in csv.DictReader(open(R + 'derived/manifest.csv')):
    seed[r['path']] = r['seed']; var[r['path']] = r['variant']
ids = {}
def fid(p): return ids.setdefault(p, len(ids))
edges, seen = [], set()
for r in csv.DictReader(open(dump)):
    if r['kind'] == 'propagated': continue
    a, b = fid(r['a']), fid(r['b'])
    k = (min(a, b), max(a, b))
    if k in seen: continue
    seen.add(k)
    ova, ovb, sc = float(r['ov_a']), float(r['ov_b']), float(r['scale'])
    clean = (max(ova, ovb) >= bar and int(r['n_in']) >= pts and r['centred'] == '1'
             and float(r['blk']) >= corr and float(r['blk_min']) >= 0.9)
    edges.append(dict(a=a, b=b, ova=ova, ovb=ovb, clean=clean, r=r))
names = {v: k for k, v in ids.items()}
n = len(ids)
def comps(es):
    g = nx.Graph(); g.add_nodes_from(range(n)); g.add_edges_from((e['a'], e['b']) for e in es)
    cid = {}
    for i, c in enumerate(nx.connected_components(g)):
        for x in c: cid[x] = (i, min(c), len(c))
    return cid
def one_way(e):
    a, b = e['ova'] >= bar, e['ovb'] >= bar
    return (e['a'], e['b']) if a and not b else (e['b'], e['a']) if b and not a else None
def part_rule(clean):
    c = comps([e for e, k in zip(edges, clean) if k and one_way(e) is None])
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
        keep = own if own in cnt else max(cnt.items(), key=lambda t: (t[1][0], -t[1][1]))[0]
        for k, cl in links:
            if cl[0] != keep: out[k] = False
    return out
def bridges(clean):
    g = nx.Graph()
    idx = {}
    for k, e in enumerate(edges):
        if clean[k]: g.add_edge(e['a'], e['b']); idx[(min(e['a'], e['b']), max(e['a'], e['b']))] = k
    out = list(clean)
    br = list(nx.bridges(g))
    h = g.copy(); h.remove_edges_from(br)
    comp = {}
    for i, c in enumerate(nx.connected_components(h)):
        for x in c: comp[x] = i
    size = collections.Counter(comp.values())
    t = nx.Graph(); t.add_nodes_from(size)
    t.add_edges_from((comp[u], comp[v], {'b': (u, v)}) for u, v in br)
    for tree in nx.connected_components(t):
        root = next(iter(tree)); total = sum(size[x] for x in tree)
        sub = {}
        order = list(nx.dfs_postorder_nodes(t, root))
        parent = dict(nx.dfs_predecessors(t, root))
        for x in order: sub[x] = size[x] + sum(sub[y] for y in t[x] if parent.get(y) == x)
        for x, p in parent.items():
            below = sub[x]
            if below > 1 and total - below > 1:
                u, v = t[x][p]['b']
                out[idx[(min(u, v), max(u, v))]] = False
    return out
def fam(i): return seed.get(names[i])
def show(tag, clean):
    c = comps([e for e, k in zip(edges, clean) if k])
    ca = {c[i][0] for i in range(n) if fam(i) == SA}; cb = {c[i][0] for i in range(n) if fam(i) == SB}
    joined = ca & cb
    print(f'{tag}: clean {sum(clean)}; clusters holding {SA} {len(ca)}, {SB} {len(cb)}, shared {len(joined)}')
    cross = [(k, e) for k, e in enumerate(edges) if clean[k] and {fam(e['a']), fam(e['b'])} == {SA, SB}]
    for k, e in cross:
        r = e['r']
        print('   ', var[r['a']], seed[r['a']].split('/')[-1], '|', var[r['b']], seed[r['b']].split('/')[-1],
              'n_in', r['n_in'], 'ov', r['ov_a'], r['ov_b'], 'blk', r['blk'], 'min', r['blk_min'], 'kind', r['kind'])
    return c
base = [e['clean'] for e in edges]
off = bridges(base)
show('off', off)
p = part_rule(base)
print('part rule demoted', sum(a and not b for a, b in zip(base, p)))
on = bridges(p)
show('on', on)
# the off run's bridge demotions that the rule undoes
undone = [k for k in range(len(edges)) if base[k] and not off[k] and on[k]]
print('bridge-demoted off, clean on:', len(undone))
for k in undone:
    r = edges[k]['r']
    if {fam(edges[k]['a']), fam(edges[k]['b'])} & {SA, SB}:
        print('   ', var[r['a']], seed[r['a']].split('/')[-1], '|', var[r['b']], seed[r['b']].split('/')[-1], 'n_in', r['n_in'], 'ov', r['ov_a'], r['ov_b'], 'min', r['blk_min'])
