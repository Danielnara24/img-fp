"""replay3.py: depth set per file in blocks of -k: read the next block while the last one held a
candidate beating the file's null (its first window's highest peak); verify those that beat it."""
import sys
exec(open('replay2.py').read().split("rules = {")[0].replace("for l in open(sys.argv[1] if len(sys.argv) > 1 else 'sweep.log'):", "for l in open('sweep.log'):"))
def adaptive(cap, beat, extra_blocks=0):
    scored = ver = 0; g5 = set(); g7 = set(); fa = 0; deep = []
    for n, mx, sec, p90, c in askers:
        by = {}
        for x in c: by.setdefault(x[0] // K, []).append(x)
        b = 0; slack = extra_blocks
        while True:
            lo = K + b * K
            if lo >= min(n, cap): break
            scored += min(K, min(n, cap) - lo)
            hit = [x for x in by.get(b, []) if beat(x[1], mx, sec, p90)]
            ver += len(hit)
            for r, peak, st, blk, k in hit:
                if st >= 2 and {k[0][0], k[1][0]} == {'Q', 'R'}:
                    if k in gt:
                        if k not in base[0.5]: g5.add(k)
                        if blk >= 0.7 and k not in base[0.7]: g7.add(k)
                    else: fa += 1
            b += 1
            if not hit:
                if slack == 0: break
                slack -= 1
        deep.append(K + b * K)
    deep.sort()
    cpu = scored * SCORE + ver * VERIFY + QUERY * (sum(deep) / len(deep)) / 3000
    return ver, len(g5), len(g7), fa, cpu, deep[len(deep) // 2], deep[int(len(deep) * .99)], scored
for name, beat in (('>max', lambda p, mx, s, p9: p > mx), ('>=max', lambda p, mx, s, p9: p >= mx), ('>2nd', lambda p, mx, s, p9: p > s)):
    for slack in (0, 1, 2):
        v, a, b, fa, cpu, med, p99, sc = adaptive(3000, beat, slack)
        print(f'{name:6s} blocks past an empty one {slack} | scored {sc:9d} verified {v:7d} | gain {a:3d} {b:3d} | false {fa:3d} | depth median {med} p99 {p99} | est +{100*cpu/2623:.1f}%')
