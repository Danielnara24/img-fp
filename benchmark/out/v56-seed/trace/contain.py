import json,sys,collections
sys.path.insert(0,'/home/daniel/Documents/Vscode_repositories/img-fp/benchmark')
sys.argv=['x','--corpus=/home/daniel/Documents/IMGS-ALL2']
import analyse
man,pairs=analyse.load_truth(); known,seed_of,variant_of,positives,skipped,prim=analyse.build(man,pairs)
def true(f):
    found,*_=analyse.load_pairs(f); return found & positives
c512=true('trace/ship-512.json'); old512=true('ship-all2.json'); c1k=true('trace/ship-1024.json'); d1k=true('detail/ship-1024.json')
print('true: cold512',len(c512),'fixtest512',len(old512),'cold1024',len(c1k),'detail/1024',len(d1k))
print('cold512 vs fixtest512: only cold',len(c512-old512),'only fixtest',len(old512-c512))
print('lost cold512 -> 1024 (one cache):',len(c512-c1k),' added',len(c1k-c512))
print('1024 runs identical:',c1k==d1k, len(c1k^d1k))
