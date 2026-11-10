import json,sys
sys.path.insert(0,'/home/daniel/Documents/Vscode_repositories/img-fp/benchmark')
sys.argv=['x','--corpus=/home/daniel/Documents/IMGS-ALL2']
import analyse
man,pairs=analyse.load_truth(); known,seed_of,variant_of,positives,skipped,prim=analyse.build(man,pairs)
def true(f):
    found,*_=analyse.load_pairs(f); return found & positives
for b in ('head','ship'):
    s512=true('o2o-all2-off.json' if b=='head' else 'ship-all2.json'); s1k=true(f'detail/{b}-1024.json'); s2k=true(f'detail/{b}-2048.json')
    print(b,'true pairs 512 / 1024 / 2048:',len(s512),len(s1k),len(s2k),' lost 512->1024',len(s512-s1k),' 1024->2048',len(s1k-s2k),' added',len(s1k-s512),len(s2k-s1k))
