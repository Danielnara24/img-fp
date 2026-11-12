#!/bin/bash
# go11.sh BUILD: first look at a build on a fresh cache. IMGS-ALL2 and IMGS-ALL
# at the default and at 256 (same cache), containment, DISC21 shard 0 at 0.5.
cd "$(dirname "$0")"; G=$1; V=$G; D=/home/daniel/Documents; X=$D/DISC21; B=$D/Vscode_repositories/img-fp/benchmark
L=${L:-$PWD/../lad8/l.db}; XC=${XC:-$X/cache66/c.db}
say() { echo "$(date '+%F %T') $*" >> $V/log; }
tm() { echo "$(grep 'User time' $1 | awk '{print $NF}') $(grep Elapsed $1 | awk '{print $NF}') $(grep Maximum $1 | awk '{print $NF}')"; }
run() { local n=$1 m=$2; shift 2; local f=$V/$n; rm -f $L.matched
  /usr/bin/time -v ./img-fp-$G $m -r -x '*' -t 8 -v --no-last --cache $L "$@" -o $f.json 2> $f.err; rm -f $L.matched; say "$n $(tm $f.err)"; }
score() { local n=$1 m=$2 f=$V/$1; python3 -I $B/analyse.py --corpus=$m $f.json > $f.an 2>&1
  python3 -I -c "
import json,sys
for p in json.load(open(sys.argv[1]))['pairs']: print(p['a']+'\t'+p['b'])" $f.json | sort > $f.pairs; rm -f $f.json; }
for c in all2 all; do m=$D/IMGS-ALL2; [ $c = all ] && m=$D/IMGS-ALL
  run $c-fill $m; rm -f $V/$c-fill.json
  run $c-512 $m; score $c-512 $m
  run $c-256 $m --detail 256; score $c-256 $m
  for d in 256 512; do ln -sf $c-$d.pairs $V/lad-$c-$d.pairs; done
  python3 -I contain2.py $V $c $m > $V/$c.contain 2>&1; say "contain $c"
done
rm -f $XC.matched
/usr/bin/time -v ./img-fp-$G $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $XC -o $V/disc-0.5.json 2> $V/disc-0.5.err
rm -f $XC.matched; say "disc fill $(tm $V/disc-0.5.err)"
python3 -I dscore.py $V/disc-0.5.json > $V/disc.score 2>&1
say DONE
