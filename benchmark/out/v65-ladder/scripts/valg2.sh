#!/bin/bash
# v65: the 256-based ladder (g2) against head. One run at a time.
O=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65; B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
D=/home/daniel/Documents; X=$D/DISC21
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db; L=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad4/l.db
cd $O; echo 1000 > /proc/self/oom_score_adj; mkdir -p val
say() { echo "$(date '+%F %T') $*" >> $O/val.log; }
temp() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat $h/name)" = k10temp ] && echo $(( $(cat $h/temp1_input) / 1000 )); done; }
tm() { grep -E 'User time|Elapsed|Maximum res' $1 | awk '{print $NF}' | tr '\n' ' '; }
keep() { local m=$1 f=$2
  python3 -I $B/score.py --corpus $m $f.json > $f.score 2>&1
  python3 -I $B/analyse.py --corpus=$m $f.json > $f.an 2>&1; }
cached() { # name binary cache corpus args...
  local n=$1 b=$2 c=$3 m=$4; shift 4; local f=val/$n
  [ -s $f.err ] && grep -q "Exit status" $f.err && return; rm -f $c.matched
  /usr/bin/time -v ./img-fp-$b $m -r -x '*' -t 8 -v --no-last --cache $c "$@" -o $f.json 2> $f.err
  rm -f $c.matched; say "$n $(tm $f.err)"; }

# 1. cost: cold, no cache, evicted, cooled, ABBA
if [ ! -s val/cost.done ]; then
  sleep 60; idle=$(temp); say "cost: idle $idle C"
  for w in found:/home/daniel/Downloads/archive all2:$D/IMGS-ALL2; do n=${w%%:*}; p=${w#*:}; i=0
    for b in head g2 g2 head fast d256 d256 fast; do i=$((i+1)); a=; x=$b; [ $b = fast ] && { a=--fast; x=head; }; [ $b = d256 ] && { a='--detail 256'; x=g2; }
      python3 -I evict.py $p; sleep 60; t=0; while [ $(temp) -gt $((idle+3)) ] && [ $t -lt 900 ]; do sleep 10; t=$((t+10)); done
      t0=$(temp)
      /usr/bin/time -v ./img-fp-$x $p -r -x '*' -t 8 -v --no-cache --no-last $a -o val/cost-$n-$i-$b.json 2> val/cost-$n-$i-$b.err
      say "cost $n $i $b start ${t0}C end $(temp)C $(tm val/cost-$n-$i-$b.err)"
      [ $n = all2 ] && rm -f val/cost-$n-$i-$b.json
    done
  done
  echo ok > val/cost.done
fi
# 2. IMGS-ALL2 cached, ABBA (matching cost), g2 scored
cached all2-1-head head $C $D/IMGS-ALL2; rm -f val/all2-1-head.json
cached all2-2-g2 g2 $L $D/IMGS-ALL2; keep $D/IMGS-ALL2 val/all2-2-g2
cached all2-3-g2 g2 $L $D/IMGS-ALL2; rm -f val/all2-3-g2.json
cached all2-4-head head $C $D/IMGS-ALL2; rm -f val/all2-4-head.json
# 3. --detail 256 against head --fast
cached all2-d256-g2 g2 $L $D/IMGS-ALL2 --detail 256
cached all2-fast-head head $C $D/IMGS-ALL2 --fast
python3 -I - > val/d256.cmp 2>&1 <<'PY'
import json
a=json.load(open('val/all2-d256-g2.json')); b=json.load(open('val/all2-fast-head.json'))
k=lambda d:{(min(p['a'],p['b']),max(p['a'],p['b'])) for p in d['pairs']}
A,B=k(a),k(b); print('d256',len(A),'fast',len(B),'only d256',len(A-B),'only fast',len(B-A))
PY
rm -f val/all2-fast-head.json
# 4. IMGS-ALL: g2 fills its cache, then cached; head cached
cached all-fill-g2 g2 $L $D/IMGS-ALL; rm -f val/all-fill-g2.json
cached all-g2 g2 $L $D/IMGS-ALL; keep $D/IMGS-ALL val/all-g2
cached all-head head $C $D/IMGS-ALL; keep $D/IMGS-ALL val/all-head
python3 -I - > val/all.contain 2>&1 <<'PY'
import json
k=lambda f:{(min(p['a'],p['b']),max(p['a'],p['b'])) for p in json.load(open(f))['pairs']}
A=k('val/all-g2.json'); B=k('val/all-head.json'); print('g2',len(A),'head',len(B),'only g2',len(A-B),'only head',len(B-A))
PY
rm -f val/all-head.json val/all-g2.json
# 5. containment 256 -> 512 on IMGS-ALL2 (true pairs)
python3 -I - > val/all2.contain 2>&1 <<'PY'
import json,csv
k=lambda f:{(min(p['a'],p['b']),max(p['a'],p['b'])) for p in json.load(open(f))['pairs']}
A=k('val/all2-d256-g2.json'); B=k('val/all2-2-g2.json')
print('d256',len(A),'d512',len(B),'d256 pairs d512 lacks',len(A-B))
pos=set()
for r in csv.DictReader(open('/home/daniel/Documents/IMGS-ALL2/derived/pairs.csv')):
    if r['relation'] in ('SAME','CROP'): pos.add((min(r['a'],r['b']),max(r['a'],r['b'])))
import os
rel=lambda p: os.path.relpath(p,'/home/daniel/Documents/IMGS-ALL2')
L={(min(rel(a),rel(b)),max(rel(a),rel(b))) for a,b in A-B}
print('of them true', len(L&pos), 'false', len(L-pos))
PY
# 6. DISC21 shard 0: g2 on a cache of its own (cold fill), 0.5 then 0.7
mkdir -p $X/cache65
for bar in 0.5 0.7; do f=val/disc-g2-$bar
  [ -s $f.json ] && continue; rm -f $X/cache65/c.db.matched
  /usr/bin/time -v ./img-fp-g2 $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation $bar -t 8 -v --no-last --cache $X/cache65/c.db -o $f.json 2> $f.err
  rm -f $X/cache65/c.db.matched; say "disc g2 $bar $(tm $f.err)"
done
f=val/disc-head-0.5; rm -f $X/cache57/c.db.matched
/usr/bin/time -v ./img-fp-head $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $X/cache57/c.db -o $f.json 2> $f.err
rm -f $X/cache57/c.db.matched; say "disc head 0.5 $(tm $f.err)"
python3 -I dscore.py val/disc-g2-0.5.json val/disc-g2-0.7.json val/disc-head-0.5.json > val/disc.score 2>&1
say ALL DONE
