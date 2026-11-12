import json, sys
B=sys.argv[1]
def load(p):
    d=json.load(open(p)); return {(min(x["a"],x["b"]),max(x["a"],x["b"])) for x in d["pairs"]}
D=load(B+"/all2-head.json"); F=load(B+"/all2-head-fast.json")
def fams(P):
    par={}
    def f(x):
        while par.setdefault(x,x)!=x:
            par[x]=par[par[x]]; x=par[x]
        return x
    for a,b in P: par[f(a)]=f(b)
    return f,par
def veto(base, extra):
    f,par=fams(base)
    out=set(base)
    for a,b in extra:
        if (a,b) in base: continue
        if a not in par or b not in par or f(a)==f(b): out.add((a,b))
    return out
def w(name,P):
    json.dump({"tool":name,"config":{},"groups":[],"pairs":[{"a":a,"b":b} for a,b in P]},open(name+".json","w"))
    print(name,len(P))
print("D",len(D),"F",len(F),"F-D",len(F-D),"D-F",len(D-F))
w("u0-union",D|F); w("u1-default-rules",veto(D,F)); w("u2-fast-rules",veto(F,D))
