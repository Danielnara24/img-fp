import sys
from collections import Counter
from clus import load, cluster, seed_of
for dump in ("base.dump.csv", "area.dump.csv"):
    best = load(dump)
    for demote in (False, True):
        for order in ("n_in", "blk_min"):
            f, mem, joins = cluster(best, demote, order)
            strays = Counter(); clusters = 0; frag = 0
            for ms in mem.values():
                c = Counter(seed_of[x] for x in ms)
                if len(ms) > 1: clusters += 1
                top = c.most_common()
                for s, n in top[1:]: strays[(top[0][0], s)] += n
            # clusters per seed: fragmentation of families
            per = Counter(Counter(seed_of[x] for x in ms).most_common(1)[0][0] for ms in mem.values() if len(ms) > 1)
            print(f"{dump[:4]} demote={demote!s:5} order={order:7} clusters {clusters} multi-cluster seeds {sum(1 for v in per.values() if v>1)} strays {sum(strays.values())}: "
                  + ", ".join(f"{a.split('/')[-1]}<-{b.split('/')[-1]} {n}" for (a, b), n in strays.most_common()))
