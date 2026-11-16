#!/bin/bash
# v72: the new defaults (correlation 0.65, points 5), 0.85.0 + the change,
# no bar passed: IMGS-ALL, -ALL2, -ALL3 cached (v71's cache), DISC21 shard 0
# (cache57), the keep rule on all three, and small families.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v72-defaults; B=$O/../..
D=/home/daniel/Documents; X=$D/DISC21; C=$B/out/v71-bars/cache/c.db; PY=$B/../vendor/venv/bin/python
cd $O; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date '+%F %T') $*" >> $O/batch.log; }
for c in ALL ALL2 ALL3; do f=run/$c; p=$D/IMGS-$c
  [ -s $f.an ] && continue; rm -f $C.matched
  /usr/bin/time -v ./img-fp-new $p -r -x '*' -t 8 --no-last --cache $C -o $f.json 2> $f.err
  rm -f $C.matched
  $PY -I $B/analyse.py --corpus=$p --per-transform $f.json > $f.an.part 2>&1 && mv $f.an.part $f.an
  say "run $c $(grep -E 'User time|Elapsed' $f.err | awk '{print $NF}' | tr '\n' ' ')"
done
for c in ALL ALL2 ALL3; do
  [ -s run/$c.keep ] && continue
  $PY $B/out/v67-keep/redundant.py $D/IMGS-$c/derived/manifest.csv run/$c.json > run/$c.keep 2>&1
  IMGFP_EFF=0.7 $PY $B/out/v67-keep/redundant.py $D/IMGS-$c/derived/manifest.csv run/$c.json > run/$c.keep07 2>&1
  say "keep $c"
done
f=disc/new
if [ ! -s $f.score ]; then rm -f $X/cache57/c.db.matched
  /usr/bin/time -v ./img-fp-new $X/queries/images/queries --against $X/refs0/images/references -t 8 --no-last --cache $X/cache57/c.db -o $f.json 2> $f.err
  rm -f $X/cache57/c.db.matched
  $PY -I $B/out/v71-bars/dscore.py $f.json > $f.score 2>&1; rm -f $f.json
  say "disc $(grep -E 'User time|Elapsed' $f.err | awk '{print $NF}' | tr '\n' ' ')"
fi
if [ ! -s fam.score ]; then
  cd $B && $PY families.py run $O/img-fp-new $O/fam --cache $C --corpus all all2 all3 i5 hdev
  $PY families.py score $O/fam -v > $O/fam.score 2>&1
  $PY families.py compare out/v71-bars/fam/base $O/fam > $O/fam.compare 2>&1; cd $O
  say fam
fi
say ALL DONE
