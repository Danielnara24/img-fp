#!/bin/bash
# v64 part 1: does the default state every true pair --fast states? Cached
# (--fast's analyses are added to the same caches), one run at a time.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v64-ladder
D=/home/daniel/Documents; X=$D/DISC21
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db
cd $O; mkdir -p fast; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date '+%F %T') $*" >> $O/fast.log; }
t() { grep -E 'User time|Elapsed|Maximum res' $1 | awk '{print $NF}' | tr '\n' ' '; }
for c in all2 all; do m=$D/IMGS-ALL2; [ $c = all ] && m=$D/IMGS-ALL
  for mode in fast default; do f=fast/$c-$mode
    [ -s $f.json ] && continue; rm -f $C.matched
    fl=; [ $mode = fast ] && fl=--fast
    /usr/bin/time -v ./img-fp-head $m -r -x '*' -t 8 --no-last --cache $C $fl -o $f.json 2> $f.err
    say "$c $mode $(t $f.err)"
  done
done
for r in fast:0.5 default:0.5 fast:0.7; do mode=${r%%:*}; bar=${r#*:}; f=fast/disc-$mode-$bar
  [ -s $f.json ] && continue; rm -f $X/cache57/c.db.matched
  fl=; [ $mode = fast ] && fl=--fast
  /usr/bin/time -v ./img-fp-head $X/queries/images/queries --against $X/refs0/images/references $fl --min-pixel-correlation $bar -t 8 --no-last --cache $X/cache57/c.db -o $f.json 2> $f.err
  rm -f $X/cache57/c.db.matched
  say "disc $mode $bar $(t $f.err)"
done
say ALL DONE
