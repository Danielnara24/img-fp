import json, sys
rows=[]
for f in ("v14-fullsweep/results.jsonl", sys.argv[1]):
    rows += [json.loads(l) for l in open(f)]
DEF=dict(ap=10,ov=0.85,corr=0.6,k=150)
C=("imgs","imgs2","imgs3","all")
def merged(r): return any(n>=300 for n in r.get("seedpairs",{}).values())
def cell(r):
    if r is None: return "–"
    s=f"{r['f1']:.4f}"; return f"**{s}†**" if merged(r) else s
def get(c,ws,p,v):
    best=None
    for r in rows:
        if r["corpus"]!=c or r["ws"]!=ws or "f1" not in r: continue
        k=r["key"]
        if ws==512 and not (k.startswith("thr_") or k.startswith("lim_")): continue
        if not (k.startswith("thr_") or k.startswith("ws_") or k.startswith("lim_")): continue
        if r[p]==v and all(r[q]==DEF[q] for q in DEF if q!=p): best=r
    return best
def thr(p,vals,label):
    print(f"| {label} | " + " | ".join(f"{w} {c.upper() if c!='all' else 'all 3'}" for w in (512,640) for c in C) + " |")
    print("|---"*9+"|")
    for v in vals:
        b="**" if v==DEF[p] else ""
        print(f"| {b}{v}{b} | " + " | ".join(cell(get(c,w,p,v)) for w in (512,640) for c in C) + " |")
    print()
thr("ap",[3,5,6,7,8,9,10,11,12,14,16,20],"`--min-aligned-points`")
thr("ov",[0.3,0.4,0.5,0.6,0.7,0.8,0.85,0.9,0.95],"`--min-frame-overlap`")
thr("corr",[0.2,0.4,0.5,0.55,0.6,0.65,0.7,0.8],"`--min-pixel-correlation`")
thr("k",[10,25,50,100,150,300,500],"`-k`")
for r in rows:
    if r["key"].startswith("thr_") and r["ws"]==512 and merged(r):
        print(r["key"], [(k,n) for k,n in r["seedpairs"].items() if n>=300])
