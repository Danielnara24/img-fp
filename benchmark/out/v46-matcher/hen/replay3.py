# Replay the claim graph's clean flags off one --dump: standing Clean, the
# shared-part rule (off / on), the bridge test; then which clean links join
# two seeds' families. Usage: replay.py DUMP SEED_A SEED_B [corr] [overlap] [points]
import csv, sys, collections
import networkx as nx
import os
R = os.environ.get('CORPUS', '/home/daniel/Documents/IMGS-ALL') + '/'
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
    ova0, ovb0 = float(r['ov_a']), float(r['ov_b'])
    anchor = (max(ova0, ovb0) >= bar and 1/8 <= float(r['scale']) <= 8 and int(r['n_in']) >= pts
              and r['centred'] == '1' and float(r['blk']) >= corr)
    if not anchor or k in seen: continue
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
def part_rule(clean, resolve=False):
    c = comps([e for e, k in zip(edges, clean) if k and one_way(e) is None])
    if resolve:
        # a file with no two-way clean link takes the cluster most of its
        # one-way clean links reach (ties: the lowest file); else stays alone
        votes = collections.defaultdict(collections.Counter)
        for e, k in zip(edges, clean):
            if k and one_way(e):
                for x, y in ((e['a'], e['b']), (e['b'], e['a'])):
                    if c[x][2] == 1 and c[y][2] > 1: votes[x][c[y]] += 1
        c = dict(c)
        for x, v in votes.items():
            c[x] = max(v.items(), key=lambda t: (t[1], -t[0][1]))[0]
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

class C:
    def __init__(s, n): s.id=list(range(n)); s.m=[[i] for i in range(n)]; s.cyc=[False]*n
    def join(s, a, b):
        ra, rb = s.id[a], s.id[b]
        if ra == rb: s.cyc[ra] = True; return ra
        if len(s.m[ra]) < len(s.m[rb]): ra, rb = rb, ra
        for x in s.m[rb]: s.id[x] = ra
        s.m[ra] += s.m[rb]; s.m[rb] = []; s.cyc[ra] = s.cyc[ra] or s.cyc[rb]
        return ra
def admit(clean, tag):
    c = C(n); weak = []
    for e, k in zip(edges, clean):
        if k: c.join(e['a'], e['b'])
        else: weak.append(e)
    weak.sort(key=lambda e: (-int(e['r']['n_in']), -float(e['r']['blk']), min(e['a'],e['b']), max(e['a'],e['b'])))
    near = collections.defaultdict(list)
    for e in weak: near[e['a']].append(e['b']); near[e['b']].append(e['a'])
    def has(cl, s): return any(fam(x) == s for x in c.m[cl])
    refused = set(); rnd = 0
    while True:
        joined = False; rnd += 1
        for e in weak:
            ra, rb = c.id[e['a']], c.id[e['b']]
            if ra == rb: continue
            small, big = (ra, rb) if len(c.m[ra]) <= len(c.m[rb]) else (rb, ra)
            if (small, big) in refused: continue
            covered = all(any(c.id[y] == big for y in near[x]) for x in c.m[small])
            ok = covered and (not (c.cyc[ra] and c.cyc[rb]) or any(sum(1 for z in near[y] if c.id[z] == small) == len(c.m[small]) for y in c.m[big]))
            if ok:
                if (has(small, SA) and has(big, SB)) or (has(small, SB) and has(big, SA)) or                    (has(small, SA) and has(small, SB)) is False and False: pass
                hit = (has(small, SA) or has(big, SA)) and (has(small, SB) or has(big, SB)) and not ((has(small,SA) and has(small,SB)) or (has(big,SA) and has(big,SB)))
                if hit:
                    r = e['r']
                    print(f'  {tag} round {rnd}: join small {len(c.m[small])} (cycle {c.cyc[small]}) into big {len(c.m[big])} (cycle {c.cyc[big]}) by',
                          var[r['a']], seed[r['a']].split('/')[-1], '|', var[r['b']], seed[r['b']].split('/')[-1], 'n_in', r['n_in'], 'ov', r['ov_a'], r['ov_b'], 'blk', r['blk'], 'min', r['blk_min'])
                    print('     small:', collections.Counter(seed[names[x]].split('/')[-1]+':'+var[names[x]] for x in c.m[small]).most_common(8))
                    print('     big seeds:', collections.Counter(seed[names[x]].split('/')[-1] for x in c.m[big]).most_common(4))
                c.join(small, big); refused.clear(); joined = True
            else: refused.add((small, big))
        if not joined: break
    a = {c.id[i] for i in range(n) if fam(i) == SA}; b = {c.id[i] for i in range(n) if fam(i) == SB}
    print(tag, 'after admit: clusters sharing both', len(a & b))

def merged(clean, tag):
    c = C(n); weak = []
    for e, k in zip(edges, clean):
        if k: c.join(e['a'], e['b'])
        else: weak.append(e)
    weak.sort(key=lambda e: (-int(e['r']['n_in']), -float(e['r']['blk']), min(e['a'],e['b']), max(e['a'],e['b'])))
    near = collections.defaultdict(list)
    for e in weak: near[e['a']].append(e['b']); near[e['b']].append(e['a'])
    refused = set()
    while True:
        joined = False
        for e in weak:
            ra, rb = c.id[e['a']], c.id[e['b']]
            if ra == rb: continue
            small, big = (ra, rb) if len(c.m[ra]) <= len(c.m[rb]) else (rb, ra)
            if (small, big) in refused: continue
            covered = all(any(c.id[y] == big for y in near[x]) for x in c.m[small])
            ok = covered and (not (c.cyc[ra] and c.cyc[rb]) or any(sum(1 for z in near[y] if c.id[z] == small) == len(c.m[small]) for y in c.m[big]))
            if ok: c.join(small, big); refused.clear(); joined = True
            else: refused.add((small, big))
        if not joined: break
    out = []
    for cl in set(c.id):
        cnt = collections.Counter(fam(x) for x in c.m[cl])
        big = sorted([(v, k) for k, v in cnt.items() if k and v >= 5], reverse=True)
        for i in range(len(big)):
            for j in range(i + 1, len(big)):
                out.append(f"{big[i][1].split('/')[-1]}({big[i][0]})+{big[j][1].split('/')[-1]}({big[j][0]})")
    print(f'  {tag}: clean {sum(clean)}, seed pairs sharing a cluster (>=5 files each): {sorted(out)}')
base = [e['clean'] for e in edges]
merged(bridges(base), 'off')
merged(bridges(part_rule(base)), 'v1 ')
merged(bridges(part_rule(base, True)), 'v2 ')
