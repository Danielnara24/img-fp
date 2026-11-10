"""cmp.py N... : uAP of each run's report over zips N..., GT restricted to them."""
import csv,json,os,sys
ident=lambda p: os.path.splitext(os.path.basename(p))[0]
ns=sys.argv[1:]; refs=set()
for n in ns: refs|={l.strip() for l in open(f"refs{n}.ids")}
gt={(r[0],r[1]) for r in csv.reader(open("gt/dev_ground_truth.csv")) if len(r)>1 and r[1] in refs}
def load(paths):
    best={}
    for f in paths:
        if not os.path.exists(f): return None
        for p in json.load(open(f))["pairs"]:
            a,b=ident(p["a"]),ident(p["b"])
            if {a[0],b[0]}!={"Q","R"}: continue
            k=(a,b) if a[0]=="Q" else (b,a); v=(p["aligned_points"],p["pixel_correlation"])
            if k not in best or v>best[k]: best[k]=v
    return best
runs={"512":[f"shard{n}/report.json" for n in ns],"1024 old":[f"w1024_{n}/report.json" for n in ns],"1024 both fixes":[f"w1024b_{n}/report.json" for n in ns]}
print(f"zips {ns}: {len(gt)} true copies")
for lo in (0.5,0.6,0.7):
    for name,paths in runs.items():
        best=load(paths)
        if best is None: continue
        sel=sorted(((v[0]+v[1]/2,k in gt) for k,v in best.items() if v[1]>=lo),reverse=True)
        tp=ap=r90=0
        for i,(_,t) in enumerate(sel,1):
            if t: tp+=1; ap+=tp/i
            if tp/i>=0.9: r90=max(r90,tp/len(gt))
        print(f"bar {lo} {name:16s} preds {len(sel):5d} true {tp:4d} false {len(sel)-tp:5d} uAP {ap/len(gt)*100:.1f} recall {tp/len(gt)*100:.1f} R@P90 {r90*100:.1f}")
