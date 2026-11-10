import json
rows=[json.loads(l) for l in open("results.jsonl")]
DEF=dict(ap=10,ov=0.85,corr=0.6,k=150)
C=("imgs","imgs2","imgs3","all")
def merged(r): return any(n>=300 for n in r.get("seedpairs",{}).values())
def cell(r):
    if r is None: return "–"
    s=f"{r['f1']:.4f}"
    return f"**{s}†**" if merged(r) else s
def get(c,ws,p,v):
    for r in rows:
        if r["corpus"]!=c or r["ws"]!=ws or "f1" not in r: continue
        if p=="ws":
            if r["key"].startswith("ws_"): 
                if r["ws"]==v: return r
            continue
        if r[p]==v and all(r[q]==DEF[q] for q in DEF if q!=p): return r
def thr(p,vals,label):
    print(f"| {label} | " + " | ".join(f"{w} {c.upper() if c!='all' else 'all 3'}" for w in (384,640) for c in C) + " |")
    print("|---"*9+"|")
    for v in vals:
        b = "**" if v==DEF[p] else ""
        print(f"| {b}{v}{b} | " + " | ".join(cell(get(c,w,p,v)) for w in (384,640) for c in C) + " |")
thr("ap",[3,4,5,6,7,8,9,10,11,12,14,16,20],"`--min-aligned-points`")
print()
thr("ov",[0.3,0.5,0.6,0.7,0.75,0.8,0.85,0.9,0.95],"`--min-frame-overlap`")
print()
thr("corr",[0.2,0.3,0.4,0.5,0.55,0.6,0.65,0.7,0.8],"`--min-pixel-correlation`")
print()
thr("k",[10,25,50,100,150,200,300,500],"`-k`")
print()
def an(ws):
    if ws==0: return "own"
    f=1
    while ws*f*2<=512: f*=2
    return ws*f
print("| `--work-size` | analysed at | IMGS F1 | recall | IMGS2 F1 | IMGS3 F1 | all 3 F1 | IMGS CPU-s |")
print("|---|---|---|---|---|---|---|---|")
for ws in [16,32,48,64,96,128,192,224,240,256,272,288,320,384,448,512,640,768,896,1024,1280,0]:
    r={c:get(c,ws,"ws",ws) for c in C}
    i=r["imgs"]
    b="**" if ws in (384,640) else ""
    print(f"| {b}{ws if ws else '0 (full)'}{b} | {an(ws)} | {cell(i)} | {100*i['recall']:.2f}% | {cell(r['imgs2'])} | {cell(r['imgs3'])} | {cell(r['all'])} | {i['cpu']:.0f} |" if i else f"| {ws} | {an(ws)} | – | – | – | {cell(r['imgs3'])} | – | – |")
