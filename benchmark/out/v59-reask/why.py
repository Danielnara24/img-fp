import csv, json, sys, collections
t = {r['path']: (r['seed'], r['side'], r['turn']) for r in csv.DictReader(open('/home/daniel/Documents/MIRFAM-truth.csv'))}
def load(f):
    cross=collections.defaultdict(int); other=collections.Counter(); mir=collections.Counter()
    for p in json.load(open(f))['pairs']:
        a,b=t[p['a']],t[p['b']]
        if a[0]!=b[0]: other[tuple(sorted((a[0],b[0])))]+=1
        elif a[1]!=b[1]: cross[a[0]]+=1
    return cross,other
cc,co=load('mir-cur.json'); rc,ro=load('mir-repfam.json'); ac,ao=load('mir-all.json')
turn={v[0]:v[2] for v in t.values()}
print('cur linked by turn', collections.Counter(turn[s] for s in cc), 'of', collections.Counter(turn.values()))
print('rep missing', [s for s in turn if s not in rc])
print('across-seed pairs new in rep (seed pair: n)')
for k,v in sorted(ro.items(), key=lambda x:-x[1]):
    if v>co.get(k,0): print(' ',k, co.get(k,0),'->',v, 'all', ao.get(k,0))
print('new in all over rep')
for k,v in sorted(ao.items(), key=lambda x:-x[1]):
    if v>ro.get(k,0): print(' ',k, ro.get(k,0),'->',v)
