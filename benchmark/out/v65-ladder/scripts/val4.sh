#!/bin/bash
# v65 final: the 256-based ladder (build $G) against head. One run at a time. Resumable.
G=${G:-g8}
SP=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad
O=$SP/v65; B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
D=/home/daniel/Documents; X=$D/DISC21
C=$SP/d/c.db; L=$SP/lad6/l.db; V=val-$G
cd $O; echo 1000 > /proc/self/oom_score_adj; mkdir -p $V
say() { echo "$(date '+%F %T') $*" >> $O/$V.log; }
temp() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat $h/name)" = k10temp ] && echo $(( $(cat $h/temp1_input) / 1000 )); done; }
tm() { grep -E 'User time|Elapsed|Maximum res' $1 | awk '{print $NF}' | tr '\n' ' '; }
pairs() { python3 -I -c "
import json,sys
with open(sys.argv[2],'w') as o:
    for p in json.load(open(sys.argv[1]))['pairs']:
        a,b=sorted((p['a'],p['b'])); o.write(a+'\t'+b+'\n')" $1.json $1.pairs; }
keep() { local m=$1 f=$2
  python3 -I $B/score.py --corpus $m $f.json > $f.score 2>&1
  python3 -I $B/analyse.py --corpus=$m $f.json > $f.an 2>&1; pairs $f; }
cached() { # name binary cache corpus args...
  local n=$1 b=$2 c=$3 m=$4; shift 4; local f=$V/$n
  [ -s $f.err ] && grep -q "Exit status" $f.err && return 1; rm -f $c.matched
  /usr/bin/time -v ./img-fp-$b $m -r -x '*' -t 8 -v --no-last --cache $c "$@" -o $f.json 2> $f.err
  rm -f $c.matched; say "$n $(tm $f.err)"; return 0; }
cold() { # tag corpus order...   (each: head | G | fast | d256)
  local n=$1 p=$2; shift 2; local i=0
  for b in "$@"; do i=$((i+1)); local a= x=$b; [ $b = fast ] && { a=--fast; x=head; }; [ $b = d256 ] && { a='--detail 256'; x=$G; }; [ $b = G ] && x=$G
    local f=$V/cost-$n-$i-$b; [ -s $f.err ] && grep -q "Exit status" $f.err && continue
    python3 -I evict.py $p; sleep ${REST:-60}; local t=0; while [ $(temp) -gt $((idle+3)) ] && [ $t -lt 900 ]; do sleep 10; t=$((t+10)); done
    local t0=$(temp)
    /usr/bin/time -v ./img-fp-$x $p -r -x '*' -t 8 -v --no-cache --no-last $a -o $f.json 2> $f.err
    say "cost $n $i $b start ${t0}C end $(temp)C $(tm $f.err)"
    case $n in found*) pairs $f;; esac; rm -f $f.json
  done; }

# 1. IMGS-ALL2 cached on a cache $G fills: matching cost ABBA, scores
cached all2-fill-G $G $L $D/IMGS-ALL2 && rm -f $V/all2-fill-G.json
cached all2-d256fill-G $G $L $D/IMGS-ALL2 --detail 256 && rm -f $V/all2-d256fill-G.json
cached all2-1-head head $C $D/IMGS-ALL2 && { keep $D/IMGS-ALL2 $V/all2-1-head; rm -f $V/all2-1-head.json; }
cached all2-2-G $G $L $D/IMGS-ALL2 && { keep $D/IMGS-ALL2 $V/all2-2-G; rm -f $V/all2-2-G.json; }
cached all2-3-G $G $L $D/IMGS-ALL2 && rm -f $V/all2-3-G.json
cached all2-4-head head $C $D/IMGS-ALL2 && rm -f $V/all2-4-head.json
# 2. --detail 256 against head --fast (cached; ABBA)
cached all2-d256-1 $G $L $D/IMGS-ALL2 --detail 256 && { keep $D/IMGS-ALL2 $V/all2-d256-1; rm -f $V/all2-d256-1.json; }
cached all2-fast-2 head $C $D/IMGS-ALL2 --fast && { keep $D/IMGS-ALL2 $V/all2-fast-2; rm -f $V/all2-fast-2.json; }
cached all2-fast-3 head $C $D/IMGS-ALL2 --fast && rm -f $V/all2-fast-3.json
cached all2-d256-4 $G $L $D/IMGS-ALL2 --detail 256 && rm -f $V/all2-d256-4.json
cmp $V/all2-d256-1.pairs $V/all2-fast-2.pairs > $V/d256.cmp 2>&1 && echo "identical pair lists" >> $V/d256.cmp
# 3. IMGS-ALL: fill, then cached; head cached
cached all-fill-G $G $L $D/IMGS-ALL && rm -f $V/all-fill-G.json
cached all-G $G $L $D/IMGS-ALL && { keep $D/IMGS-ALL $V/all-G; rm -f $V/all-G.json; }
cached all-head head $C $D/IMGS-ALL && { keep $D/IMGS-ALL $V/all-head; rm -f $V/all-head.json; }
cached all-d256fill $G $L $D/IMGS-ALL --detail 256 && rm -f $V/all-d256fill.json
say "PART 1-3 DONE"
# 4. cost: cold, no cache, evicted, cooled, ABBA
sleep 90; idle=$(temp); say "cost: idle $idle C"
cold found /home/daniel/Downloads/archive head G G head fast d256 d256 fast
REST=300; cold all2 $D/IMGS-ALL2 head G G head G head head G fast d256 d256 fast
cold all $D/IMGS-ALL head G G head
REST=60
cold r10k $X/r10k head G G head
cold desk $D/IMGS/derived/Desktop head G G head G head head G
say "PART 4 DONE"
# 5. containment at every step 256 -> 4096
for c in all2 all; do m=$D/IMGS-ALL2; [ $c = all ] && m=$D/IMGS-ALL
  for d in 4096 2048 1024 512 256; do f=$V/lad-$c-$d
    [ -s $f.pairs ] && continue; rm -f $L.matched
    /usr/bin/time -v ./img-fp-$G $m -r -x '*' -t 8 -v --no-last --cache $L --detail $d -o $f.json 2> $f.err
    rm -f $L.matched; say "lad $c $d $(tm $f.err)"
    python3 -I $B/analyse.py --corpus=$m $f.json > $f.an 2>&1; pairs $f; rm -f $f.json
  done
  python3 -I - $V $c $m > $V/lad-$c.contain 2>&1 <<'PY'
import sys,csv,os
V,c,m=sys.argv[1:4]
pos=set()
for r in csv.DictReader(open(m+'/derived/pairs.csv')):
    if r['relation'] in ('SAME','CROP'): pos.add(tuple(sorted((r['a'],r['b']))))
rel=lambda p: os.path.relpath(p,m)
def load(d): return {tuple(sorted((rel(a),rel(b)))) for a,b in (l.rstrip('\n').split('\t') for l in open(f'{V}/lad-{c}-{d}.pairs'))}
prev=None
for d in (256,512,1024,2048,4096):
    P=load(d); line=f'{d}: pairs {len(P)} true {len(P&pos)} false-or-excluded {len(P-pos)}'
    if prev is not None:
        lost=prev-P; line+=f' | of the step below: lost {len(lost)} (true {len(lost&pos)})'
    print(line); prev=P
PY
  say "contain $c done"
done
say "PART 5 DONE"
# 6. small families
for w in head:$C $G:$L; do b=${w%%:*}; c=${w#*:}
  [ -s $V/fam-$b.score ] && continue
  python3 -I $B/families.py run $O/img-fp-$b $V/fam-$b --cache $c --corpus all all2 > $V/fam-$b.out 2>&1
  python3 -I $B/families.py score $V/fam-$b > $V/fam-$b.score 2>&1; say "families $b"
done
python3 -I $B/families.py compare $V/fam-head $V/fam-$G > $V/fam.compare 2>&1
say "PART 6 DONE"
# 7. DISC21 shard 0: $G on a cache of its own (cold fill), 0.5 then 0.7; head 0.5 and 0.7 cached
mkdir -p $X/cache65
disc() { local n=$1 b=$2 c=$3 bar=$4 f=$V/disc-$1
  [ -s $f.err ] && grep -q "Exit status" $f.err && return; rm -f $c.matched
  /usr/bin/time -v ./img-fp-$b $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation $bar -t 8 -v --no-last --cache $c -o $f.json 2> $f.err
  rm -f $c.matched; say "disc $n $(tm $f.err)"; }
disc G-0.5-fill $G $X/cache65/c.db 0.5
disc head-0.5 head $X/cache57/c.db 0.5
disc G-0.5 $G $X/cache65/c.db 0.5
disc G-0.7 $G $X/cache65/c.db 0.7
disc head-0.7 head $X/cache57/c.db 0.7
python3 -I dscore.py $V/disc-G-0.5.json $V/disc-G-0.7.json $V/disc-head-0.5.json $V/disc-head-0.7.json > $V/disc.score 2>&1
say ALL DONE
