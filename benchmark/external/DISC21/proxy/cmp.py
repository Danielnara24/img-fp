"""proxy/cmp.py RUN.json...: uAP on the proxy (zip 0's true copies in reach)."""
import csv,json,os,sys
X="/home/daniel/Documents/DISC21"
ident=lambda p: os.path.splitext(os.path.basename(p))[0]
refs={l.strip() for l in open(X+"/proxy/refs.ids")}
qs={os.path.splitext(f)[0] for f in os.listdir(X+"/proxy/q")}
gt={(r[0],r[1]) for r in csv.reader(open(X+"/gt/dev_ground_truth.csv")) if len(r)>1 and r[1] in refs and r[0] in qs}
def load(f):
    best={}
    for p in json.load(open(f))["pairs"]:
        a,b=ident(p["a"]),ident(p["b"])
        if {a[0],b[0]}!={"Q","R"}: continue
        k=(a,b) if a[0]=="Q" else (b,a); v=(p["aligned_points"] or 0,p["pixel_correlation"])
        if k not in best or v>best[k]: best[k]=v
    return best
for f in sys.argv[1:]:
    b=load(f)
    for lo in (0.5,0.7):
        sel=sorted(((v[0]+v[1]/2,k in gt) for k,v in b.items() if v[1]>=lo),reverse=True)
        tp=ap=r90=0
        for i,(_,t) in enumerate(sel,1):
            if t: tp+=1; ap+=tp/i
            if tp/i>=0.9: r90=max(r90,tp/len(gt))
        print(f"{os.path.basename(os.path.dirname(f)) or f} bar {lo}: true {tp} false {len(sel)-tp} uAP {ap/len(gt)*100:.1f} R@P90 {r90*100:.1f}")
