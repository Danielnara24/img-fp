import json,csv,os,subprocess,sys,collections,concurrent.futures as cf
D='/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v57-fixed/battery'
S=os.path.dirname(os.path.abspath(__file__))  # alone/ (a report a pair and binary) and alone.json are written beside this script
ident=lambda p: os.path.splitext(os.path.basename(p))[0]
X='/home/daniel/Documents/DISC21'
refs={l.strip() for l in open(f'{X}/refs0.ids')}
gt={(r[0],r[1]) for r in csv.reader(open(f'{X}/gt/dev_ground_truth.csv')) if len(r)>1 and r[1] in refs}
def qr(a,b):
    a,b=ident(a),ident(b)
    if {a[0],b[0]}!={'Q','R'}: return None
    return (a,b) if a[0]=='Q' else (b,a)
R={};P={}
for b in ('head','ship-r3'):
    R[b]={}
    for p in json.load(open(f'{D}/disc/{b}/report.json'))['pairs']:
        k=qr(p['a'],p['b'])
        if k: R[b][k]=p; P[k]=(p['a'],p['b'])
h,s=R['head'],R['ship-r3']
sets={'only_ship':[k for k in s if k not in gt and k not in h],'only_head':[k for k in h if k not in gt and k not in s],'both':[k for k in h if k not in gt and k in s]}
def run(args):
    binary,k=args
    a,b=P[k]; out=f'{S}/alone/{binary}-{k[0]}-{k[1]}.json'
    if not os.path.exists(out):
        subprocess.run([f'{D}/img-fp-{binary}','--no-cache','--no-last','-t','1','--min-pixel-correlation','0.5',a,b,'-o',out],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try: ps=json.load(open(out))['pairs']
    except Exception: return (binary,k,None)
    return (binary,k,(ps[0]['aligned_points'],ps[0]['pixel_correlation']) if ps else ())
os.makedirs(f'{S}/alone',exist_ok=True)
jobs=[(b,k) for ks in sets.values() for k in ks for b in ('head','ship')]
res={}
with cf.ThreadPoolExecutor(6) as ex:
    for b,k,v in ex.map(run,jobs): res[b,k]=v
for name,ks in sets.items():
    direct=[k for k in ks if (R['ship-r3'].get(k) or R['head'][k])['aligned_points']]
    for b in ('head','ship'):
        st=sum(1 for k in ks if res[b,k]); err=sum(1 for k in ks if res[b,k] is None)
        print(f'{name:10s} n={len(ks):4d}  {b} states alone: {st:4d}  (errors {err})')
json.dump({f'{b}|{k[0]}|{k[1]}':v for (b,k),v in res.items()},open(f'{S}/alone.json','w'))
