#!/bin/bash
# go13.sh: every detail step cold (no cache, evicted, rested) on IMGS-ALL2, h3
# against head, with each run's own report scored; then the found corpus.
cd "$(dirname "$0")"; V=steps; D=/home/daniel/Documents; B=$D/Vscode_repositories/img-fp/benchmark; m=$D/IMGS-ALL2
say() { echo "$(date '+%F %T') $*" >> $V/log; }
temp() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat $h/name)" = k10temp ] && echo $(( $(cat $h/temp1_input) / 1000 )); done; }
tm() { grep -E 'User time|Elapsed|Maximum res' $1 | awk '{print $NF}' | tr '\n' ' '; }
sleep 120; idle=$(temp); say "idle $idle C"
one() { local b=$1 d=$2 p=$3 n=$4; local a="--detail $d"; [ $b = head ] && [ $d = 256 ] && a=--fast
  local f=$V/$n-$b-$d; [ -s $f.err ] && grep -q "Exit status" $f.err && return
  python3 -I evict.py $p; sleep ${REST:-180}; local t=0; while [ $(temp) -gt $((idle+3)) ] && [ $t -lt 900 ]; do sleep 10; t=$((t+10)); done
  local t0=$(temp)
  /usr/bin/time -v ./img-fp-$b $p -r -x '*' -t 8 -v --no-cache --no-last $a -o $f.json 2> $f.err
  say "$n $b $d start ${t0}C end $(temp)C $(tm $f.err)"
  if [ $n = all2 ]; then python3 -I $B/analyse.py --corpus=$p $f.json > $f.an 2>&1; fi
  python3 -I -c "
import json,sys
for p in json.load(open(sys.argv[1]))['pairs']: print('\t'.join(sorted((p['a'],p['b']))))" $f.json | sort > $f.pairs; rm -f $f.json; }
one head 512 $m all2; one h3 512 $m all2
one h3 256 $m all2; one head 256 $m all2
one head 1024 $m all2; one h3 1024 $m all2
one h3 2048 $m all2; one head 2048 $m all2
one head 4096 $m all2; one h3 4096 $m all2
REST=60
for b in head h3 h3 head; do i=$((i+1)); one $b 512 $D/../Downloads/archive found$i; done
say DONE
