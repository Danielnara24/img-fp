import sys,csv,collections
V,c,m=sys.argv[1:4]
pos=set(); part=set(); var={}
for r in csv.DictReader(open(m+'/derived/pairs.csv')):
    k=tuple(sorted((r['a'],r['b'])))
    if r['relation'] in ('SAME','CROP'): pos.add(k); var[k]=(r['variant_a'],r['variant_b'])
    elif r['relation']=='PARTIAL': part.add(k)
def load(d): return {tuple(sorted((a,b))) for a,b in (l.rstrip('\n').split('\t') for l in open(f'{V}/lad-{c}-{d}.pairs'))}
prev=None
for d in (256,512,1024,2048,4096):
    P=load(d); line=f'{d}: pairs {len(P)} true {len(P&pos)} partial {len(P&part)} false {len(P-pos-part)}'
    if prev is not None:
        lost=prev-P; line+=f' | of the step below: lost {len(lost)} (true {len(lost&pos)}, partial {len(lost&part)}, false {len(lost-pos-part)}); gained true {len((P-prev)&pos)}'
        if lost&pos:
            cnt=collections.Counter(v for k in lost&pos for v in var[k])
            line+='\n    lost true by variant: '+', '.join(f'{a} {n}' for a,n in cnt.most_common(12))
    print(line); prev=P
