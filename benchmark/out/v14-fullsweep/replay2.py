import pickle, csv, os, sys
from collections import defaultdict
S=os.path.dirname(os.path.abspath(__file__))
def load(rowsfile, corpora):
    rows=pickle.load(open(rowsfile,'rb'))
    pos=[]
    for c in corpora:
        for r in csv.DictReader(open(c+'/derived/pairs.csv')):
            if r['relation'] in ('SAME','CROP'): pos.append((r['a'],r['b']))
    return rows,pos
def bridges_drop(edges):
    adj=defaultdict(list)
    for e,(a,b) in enumerate(edges): adj[a].append((b,e)); adj[b].append((a,e))
    disc,low,size={}, {}, {}
    timer=0; drop=set()
    for s in list(adj):
        if s in disc: continue
        seen=[s]; mark={s}; i=0
        while i<len(seen):
            u=seen[i]; i+=1
            for v,_ in adj[u]:
                if v not in mark: mark.add(v); seen.append(v)
        total=len(seen)
        disc[s]=low[s]=timer; timer+=1; size[s]=1
        stack=[[s,-1,0]]
        while stack:
            fr=stack[-1]; u,pe,k=fr
            if k<len(adj[u]):
                fr[2]+=1; v,e=adj[u][k]
                if e==pe: continue
                if v not in disc:
                    disc[v]=low[v]=timer; timer+=1; size[v]=1; stack.append([v,e,0])
                else: low[u]=min(low[u],disc[v])
            else:
                stack.pop()
                if stack:
                    p=stack[-1][0]; low[p]=min(low[p],low[u]); size[p]+=size[u]
                    if low[u]>disc[p] and min(size[u],total-size[u])>=2: drop.add(pe)
    return drop
def replay(rows,pos,rule,seed_of):
    seen=set(); edges=[]
    for r in rows:
        if not rule(r): continue
        k=(min(r['a'],r['b']),max(r['a'],r['b']))
        if k in seen: continue
        seen.add(k); edges.append(k)
    drop=bridges_drop(edges)
    par={}
    def f(x):
        par.setdefault(x,x)
        while par[x]!=x: par[x]=par[par[x]]; x=par[x]
        return x
    cross_edges=0
    for e,(a,b) in enumerate(edges):
        if e in drop: continue
        par[f(a)]=f(b)
        if seed_of[a]!=seed_of[b]: cross_edges+=1
    # seed merges: union seeds in same component
    comp_seeds=defaultdict(set)
    for x in list(par): comp_seeds[f(x)].add(seed_of[x])
    merges=sum(len(s)-1 for s in comp_seeds.values())
    reach=sum(1 for a,b in pos if a in par and b in par and f(a)==f(b))
    return dict(anchors=len(edges),cross_anchors=sum(1 for a,b in edges if seed_of[a]!=seed_of[b]),cross_kept=cross_edges,merges=merges,reach=reach/len(pos))

def replay_tiered(rows,pos,base,strong,seed_of,key=lambda r:(r['n_in'],r['blk'])):
    """Clusters from strong anchors (with the bridge test); weak anchors may
    attach a file still alone to a cluster, never join two clusters of 2+."""
    seen=set(); S=[]; W=[]
    for r in sorted((r for r in rows if base(r)), key=key, reverse=True):
        k=(min(r['a'],r['b']),max(r['a'],r['b']))
        if k in seen: continue
        seen.add(k); (S if strong(r) else W).append(k)
    drop=bridges_drop(S)
    par={}; size={}
    def f(x):
        if x not in par: par[x]=x; size[x]=1
        while par[x]!=x: par[x]=par[par[x]]; x=par[x]
        return x
    def u(a,b):
        ra,rb=f(a),f(b)
        if ra!=rb: par[ra]=rb; size[rb]+=size[ra]
    for e,(a,b) in enumerate(S):
        if e not in drop: u(a,b)
    refused=0; cross_join=0
    for a,b in W:
        ra,rb=f(a),f(b)
        if ra==rb: continue
        if size[ra]>=2 and size[rb]>=2: refused+=1; continue
        u(a,b)
    comp_seeds=defaultdict(set)
    for x in list(par): comp_seeds[f(x)].add(seed_of[x])
    merges=sum(len(s)-1 for s in comp_seeds.values())
    reach=sum(1 for a,b in pos if a in par and b in par and f(a)==f(b))
    return dict(strong=len(S),weak=len(W),refused=refused,merges=merges,reach=reach/len(pos))

def replay_cover(rows,pos,base,strong,seed_of,cover=1.0,key=lambda r:(r['n_in'],r['blk'])):
    """Clean clusters (bridge test); then two clusters join on weak anchors
    only when every member of the smaller has a weak anchor into the larger."""
    seen=set(); S=[]; W=[]
    for r in sorted((r for r in rows if base(r)), key=key, reverse=True):
        k=(min(r['a'],r['b']),max(r['a'],r['b']))
        if k in seen: continue
        seen.add(k); (S if strong(r) else W).append(k)
    drop=bridges_drop(S)
    par={}; mem={}
    nodes=set(x for e in S+W for x in e)
    for x in nodes: par[x]=x; mem[x]={x}
    def f(x):
        while par[x]!=x: par[x]=par[par[x]]; x=par[x]
        return x
    def u(a,b):
        ra,rb=f(a),f(b)
        if ra==rb: return
        if len(mem[ra])>len(mem[rb]): ra,rb=rb,ra
        par[ra]=rb; mem[rb]|=mem.pop(ra)
    for e,(a,b) in enumerate(S):
        if e not in drop: u(a,b)
    adj=defaultdict(list)
    for a,b in W: adj[a].append(b); adj[b].append(a)
    changed=True; joins=0
    while changed:
        changed=False
        for a,b in W:
            ra,rb=f(a),f(b)
            if ra==rb: continue
            small,big=(ra,rb) if len(mem[ra])<=len(mem[rb]) else (rb,ra)
            touching=sum(1 for x in mem[small] if any(f(y)==big for y in adj[x]))
            if touching>=cover*len(mem[small]):
                u(ra,rb); changed=True; joins+=1
    comp_seeds=defaultdict(set)
    for x in nodes: comp_seeds[f(x)].add(seed_of[x])
    merges=sum(len(s)-1 for s in comp_seeds.values())
    reach=sum(1 for a,b in pos if a in par and b in par and f(a)==f(b))
    return dict(strong=len(S),weak=len(W),joins=joins,merges=merges,reach=reach/len(pos))

def cover_components(rows,base,strong,key=lambda r:(r['n_in'],r['blk'])):
    seen=set(); S=[]; W=[]
    for r in sorted((r for r in rows if base(r)), key=key, reverse=True):
        k=(min(r['a'],r['b']),max(r['a'],r['b']))
        if k in seen: continue
        seen.add(k); (S if strong(r) else W).append(k)
    drop=bridges_drop(S)
    par={}; mem={}
    nodes=set(x for e in S+W for x in e)
    for x in nodes: par[x]=x; mem[x]={x}
    def f(x):
        while par[x]!=x: par[x]=par[par[x]]; x=par[x]
        return x
    def u(a,b):
        ra,rb=f(a),f(b)
        if ra==rb: return
        if len(mem[ra])>len(mem[rb]): ra,rb=rb,ra
        par[ra]=rb; mem[rb]|=mem.pop(ra)
    for e,(a,b) in enumerate(S):
        if e not in drop: u(a,b)
    adj=defaultdict(list)
    for a,b in W: adj[a].append(b); adj[b].append(a)
    changed=True
    while changed:
        changed=False
        for a,b in W:
            ra,rb=f(a),f(b)
            if ra==rb: continue
            small,big=(ra,rb) if len(mem[ra])<=len(mem[rb]) else (rb,ra)
            if all(any(f(y)==big for y in adj[x]) for x in mem[small]):
                u(ra,rb); changed=True
    return {x:f(x) for x in nodes}
