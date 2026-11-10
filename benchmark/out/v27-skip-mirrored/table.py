import json
rows=[json.loads(l) for l in open('results.jsonl')]
C=['imgs','imgs2','imgs3','imgs4','all','found']
def get(c,mode,skip): return [r for r in rows if r['corpus']==c and r['mode']==mode and r['skip']==skip]
print(f"{'corpus':6} {'set':4} | {'F1':6} {'P%':6} {'R%':6} perf cross merges300 groups | pairs   keep  del  rev | cold cpu      cold wall   | cached cpu    cached wall | peak")
for c in C:
    for skip in (False,True):
        a=get(c,'cold',skip)[0]; co=get(c,'cold',skip); ca=get(c,'cached',skip)
        consistent=len({r['pairs_hash'] for r in co+ca})==1
        m300=sum(1 for v in a.get('seedpairs',{}).values() if v>=300)
        acc=f"{a.get('f1',0):.4f} {100*a.get('precision',0):6.2f} {100*a.get('recall',0):6.2f} {a.get('perfect','-'):>4} {a.get('cross','-'):>5} {m300:>9}" if c!='found' else ' '*43
        print(f"{c:6} {'skip' if skip else 'full':4} | {acc} {a['groups']:6} | {a['pairs']:7,} {a['keep']:5} {a['delete']:5} {a['review']:4} | {' / '.join(str(r['cpu']) for r in co):13} {' / '.join(str(r['wall']) for r in co):11} | {' / '.join(str(r['cpu']) for r in ca):13} {' / '.join(str(r['wall']) for r in ca):11} | {max(r['peak_pss_swap'] for r in co+ca)} {'' if consistent else 'INCONSISTENT'}")
print()
for c in C:
    f=lambda s,m: sum(r[m] for r in get(c,s[0],s[1]))/2
    print(f"{c:6} cold cpu {100*(f(('cold',True),'cpu')/f(('cold',False),'cpu')-1):+5.1f}%  wall {100*(f(('cold',True),'wall')/f(('cold',False),'wall')-1):+5.1f}%   cached cpu {100*(f(('cached',True),'cpu')/f(('cached',False),'cpu')-1):+5.1f}%  wall {100*(f(('cached',True),'wall')/f(('cached',False),'wall')-1):+5.1f}%")
print()
for c in C:
    p={}
    for s in ('full','skip'):
        d=json.load(open(f'out/{c}_{s}.json'))
        p[s]={tuple(sorted((x['a'],x['b']))) for x in d['pairs']}
    print(c,'skip not in full:',len(p['skip']-p['full']),' full not in skip:',len(p['full']-p['skip']))
for c in C[:5]:
    a=get(c,'cold',True)[0]; b=get(c,'cold',False)[0]
    print(c,'cross seedpairs full',dict(list(b.get('seedpairs',{}).items())[:4]),'| skip',dict(list(a.get('seedpairs',{}).items())[:5]),'halves',b['halves'],a['halves'])
