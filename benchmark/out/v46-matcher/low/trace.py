import sys, collections
sys.argv = ['x', 'low/on2.dump.csv', 'IMGS/low-light.avif', 'IMGS/low-light2.jpg', '0.7']
src = open('hen/replay3.py').read(); src = src[:src.index("base = [e['clean']")]
exec(src)
base = [e['clean'] for e in edges]
c = comps([e for e, k in zip(edges, base) if k and one_way(e) is None])
after = part_rule(base, True)
LL, LL2 = 'IMGS/low-light.avif', 'IMGS/low-light2.jpg'
def tag(i): return f"{fam(i).split('/')[-1]}:{var[names[i]]}"
def cl(i):  # describe a two-way cluster
    cnt = collections.Counter(fam(x).split('/')[-1] for x in range(n) if c[x][0] == c[i][0]) if c[i][2] > 1 else {'alone': 1}
    return f"cluster#{c[i][1]} size {c[i][2]} {dict(cnt)}"
big = {}
for s in (LL, LL2):
    dist = collections.Counter((c[i][1], c[i][2]) for i in range(n) if fam(i) == s)
    print(s, 'files in dump', sum(dist.values()), 'two-way clusters (lowest file, size): count', dist.most_common(8))
lost = ['qr_sticker', 'crop_off_centre', 'crop_half', 'slide_deck', 'wall_poster', 'magazine_spread']
for v in lost:
    xs = [i for i in range(n) if fam(i) == LL and var[names[i]] == v]
    for x in xs:
        print(f'\n== {tag(x)}  two-way: {cl(x)}')
        for k, e in enumerate(edges):
            if x not in (e['a'], e['b']) or not base[k]: continue
            y = e['b'] if e['a'] == x else e['a']
            ow = one_way(e)
            role = 'two-way' if ow is None else ('part of' if ow[0] == x else 'holds')
            r = e['r']
            print(f"   {role:8} {tag(y):40} ov {r['ov_a']}/{r['ov_b']} n_in {r['n_in']:>4} blk_min {r['blk_min']}  y in {cl(y)[:60]:60} {'DEMOTED' if not after[k] else ''}")
