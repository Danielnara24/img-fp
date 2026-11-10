import re, sys
from collections import Counter
alone = {l.split()[0]: 'stated=(' in l for l in open('alone.out')}
rows = []
for l in open(sys.argv[1]):
    q, r = l.split()[:2]
    def opt(x): return None if x == 'None' else int(re.sub(r'\D', '', x))
    rq = re.search(r'rank_q (\S+)/', l).group(1); rr = re.search(r'rank_r (\S+)/', l).group(1)
    bow = min([v for v in (opt(rq), opt(rr)) if v is not None], default=10**9)
    m = re.search(r'wgc_q (\S+) (\S+) k150 (\S+) top (\S+) \| wgc_r (\S+) (\S+) k150 (\S+) top (\S+)', l)
    wq, sq, kq, tq, wr, sr, kr, tr = [opt(x) for x in m.groups()]
    w = min([v for v in (wq, wr) if v is not None], default=10**9)
    nin = int(re.search(r'n_in (\d+)', l).group(1))
    rows.append((q, r, bow, w, sq, kq, sr, kr, nin, alone.get(q)))
bands = [10, 50, 150, 300, 500, 1000, 3000, 10**9 + 1]
band = lambda x: next(b for b in bands if x < b)
print('BoW rank   ', sorted(Counter(band(x[2]) for x in rows).items()))
print('WGC rerank ', sorted(Counter(band(x[3]) for x in rows).items()))
lost = [x for x in rows if x[2] >= 150]
print(f'BoW >= 150: {len(lost)}; WGC rank of those:', sorted(Counter(band(x[3]) for x in lost).items()))
gained = [x for x in lost if x[3] < 150]
print('rescued to <150:', len(gained), ' with fixed n_in >= 10:', sum(x[8] >= 10 for x in gained), ' pass alone:', sum(bool(x[9]) for x in gained))
dropped = [x for x in rows if x[2] < 150 and x[3] >= 150]
print('pushed out of 150 by WGC:', len(dropped), ' of them n_in>=10:', sum(x[8] >= 10 for x in dropped))
for x in gained: print('  +', x)
for x in dropped[:15]: print('  -', x)
