#!/bin/bash
# v64 part 2: the 256-based ladder (img-fp-lad2) on IMGS-ALL2 against head.
# After fast.sh. Its own cache (L), filled by the first run (cold).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v64-ladder
D=/home/daniel/Documents
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db
L=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad/l.db
mkdir -p $(dirname $L); cd $O; echo 1000 > /proc/self/oom_score_adj
until grep -q "ALL DONE" fast.log; do sleep 30; done
say() { echo "$(date '+%F %T') $*" >> $O/lad.log; }
t() { grep -E 'User time|System time|Elapsed|Maximum res' $1 | awk '{print $NF}' | tr '\n' ' '; }
run() { # name binary cache env... -- args
  local n=$1 b=$2 c=$3; shift 3; local f=lad/all2-$n
  [ -s $f.json ] && return; rm -f $c.matched
  env "$@" /usr/bin/time -v ./img-fp-$b $D/IMGS-ALL2 -r -x '*' -t 8 --no-last --cache $c $ARGS -o $f.json 2> $f.err
  rm -f $c.matched; say "$n $(t $f.err)"; }
ARGS= run cold-w15 lad2 $L IMGFP_W512=15
ARGS= run w15 lad2 $L IMGFP_W512=15
ARGS= run w1 lad2 $L IMGFP_W512=1
ARGS= run w1-sl lad2 $L IMGFP_W512=1 IMGFP_SL512=1
ARGS= run w1-sl-noveto lad2 $L IMGFP_W512=1 IMGFP_SL512=1 IMGFP_VETO512=0
ARGS= run w3 lad2 $L IMGFP_W512=3
ARGS="--detail 256" run d256 lad2 $L X=1
ARGS= run head head $C X=1
ARGS="--fast" run head-fast head $C X=1
say ALL DONE
