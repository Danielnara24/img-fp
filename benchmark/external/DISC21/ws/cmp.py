"""ws/cmp.py RUN...: zip 0 uAP / recall per bar for each ws/RUN/report.json
(or a path), and true copies lost against the first run."""
import csv,json,os,sys
ident=lambda p: os.path.splitext(os.path.basename(p))[0]
refs={l.strip() for l in open("refs0.ids")}
gt={(r[0],r[1]) for r in csv.reader(open("gt/dev_ground_truth.csv")) if len(r)>1 and r[1] in refs}
def load(f):
    best={}
    for p in json.load(open(f))["pairs"]:
        a,b=ident(p["a"]),ident(p["b"])
        if {a[0],b[0]}!={"Q","R"}: continue
        k=(a,b) if a[0]=="Q" else (b,a); v=(p["aligned_points"],p["pixel_correlation"])
        if k not in best or v>best[k]: best[k]=v
    return best
runs=[(r, load(r if r.endswith(".json") else f"ws/{r}/report.json")) for r in sys.argv[1:]]
print(f"zip 0: {len(gt)} true copies")
base=runs[0][1]
for lo in (0.5,0.6,0.7):
    bt={k for k,v in base.items() if k in gt and v[1]>=lo}
    for name,best in runs:
        sel=sorted(((v[0]+v[1]/2,k in gt) for k,v in best.items() if v[1]>=lo),reverse=True)
        tp=ap=r90=0
        for i,(_,t) in enumerate(sel,1):
            if t: tp+=1; ap+=tp/i
            if tp/i>=0.9: r90=max(r90,tp/len(gt))
        found={k for k,v in best.items() if k in gt and v[1]>=lo}
        print(f"bar {lo} {name:18s} true {tp:4d} false {len(sel)-tp:5d} uAP {ap/len(gt)*100:5.1f} R@P90 {r90*100:5.1f}  lost vs first {len(bt-found):3d} new {len(found-bt):3d}")
