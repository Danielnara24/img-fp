#!/bin/bash
# v65 part 2: containment at every step 256 -> 4096, IMGS-ALL2 and IMGS-ALL (g2). After val.sh.
O=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65; B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
D=/home/daniel/Documents; L=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad4/l.db
cd $O; echo 1000 > /proc/self/oom_score_adj; mkdir -p val
until grep -q "ALL DONE" val.log; do sleep 60; done
say() { echo "$(date '+%F %T') $*" >> $O/val.log; }
tm() { grep -E 'User time|Elapsed|Maximum res' $1 | awk '{print $NF}' | tr '\n' ' '; }
for c in all2 all; do m=$D/IMGS-ALL2; [ $c = all ] && m=$D/IMGS-ALL
  for d in 4096 2048 1024 512 256; do f=val/lad-$c-$d
    [ -s $f.pairs ] && continue; rm -f $L.matched
    /usr/bin/time -v ./img-fp-g2 $m -r -x '*' -t 8 -v --no-last --cache $L --detail $d -o $f.json 2> $f.err
    rm -f $L.matched; say "lad $c $d $(tm $f.err)"
    python3 -I $B/analyse.py --corpus=$m $f.json > $f.an 2>&1
    python3 -I -c "
import json,sys
with open('$f.pairs','w') as o:
    for p in json.load(open('$f.json'))['pairs']:
        a,b=sorted((p['a'],p['b'])); o.write(a+'\t'+b+'\n')"
    rm -f $f.json
  done
  python3 -I - $c $m > val/lad-$c.contain 2>&1 <<'PY'
import sys,csv,os
c,m=sys.argv[1],sys.argv[2]
pos=set()
for r in csv.DictReader(open(m+'/derived/pairs.csv')):
    if r['relation'] in ('SAME','CROP'): pos.add(tuple(sorted((r['a'],r['b']))))
rel=lambda p: os.path.relpath(p,m)
def load(d): return {tuple(sorted((rel(a),rel(b)))) for a,b in (l.rstrip('\n').split('\t') for l in open(f'val/lad-{c}-{d}.pairs'))}
prev=None
for d in (256,512,1024,2048,4096):
    P=load(d); line=f'{d}: pairs {len(P)} true {len(P&pos)} false-or-excluded {len(P-pos)}'
    if prev is not None:
        lost=prev-P; line+=f' | of the step below: lost {len(lost)} (true {len(lost&pos)})'
    print(line); prev=P
PY
  say "contain $c done"
done
say ALL DONE 2
