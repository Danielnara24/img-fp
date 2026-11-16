#!/bin/bash
# v71: the three bars a user sets, each swept alone about the defaults
# (correlation 0.7, overlap 0.85, points 10), on IMGS-ALL, IMGS-ALL2,
# IMGS-ALL3 (cached, real runs: nothing replayed from a dump) and DISC21
# shard 0 (cached, cache57). img-fp 0.85.0 plain build. One run at a time;
# a step whose output exists is not run again.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v71-bars; B=$O/../..
D=/home/daniel/Documents; X=$D/DISC21; C=$O/cache/c.db; PY=$B/../vendor/venv/bin/python
cd $O; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date '+%F %T') $*" >> $O/batch.log; }
CONFIGS=(
  "c0.7:"
  "c0.6:--min-pixel-correlation 0.6" "c0.65:--min-pixel-correlation 0.65"
  "c0.75:--min-pixel-correlation 0.75" "c0.8:--min-pixel-correlation 0.8"
  "o0.75:--min-frame-overlap 0.75" "o0.8:--min-frame-overlap 0.8" "o0.9:--min-frame-overlap 0.9"
  "p3:--min-aligned-points 3" "p4:--min-aligned-points 4" "p5:--min-aligned-points 5"
  "p6:--min-aligned-points 6" "p7:--min-aligned-points 7" "p8:--min-aligned-points 8"
  "p9:--min-aligned-points 9" "p11:--min-aligned-points 11" "p12:--min-aligned-points 12"
  "p13:--min-aligned-points 13" "p14:--min-aligned-points 14"
)
for cfg in "${CONFIGS[@]}"; do t=${cfg%%:*}; a=${cfg#*:}
  for c in all:IMGS-ALL all2:IMGS-ALL2 all3:IMGS-ALL3; do n=${c%%:*}; p=$D/${c#*:}; f=sweep/$n-$t
    [ -s $f.an ] && continue; rm -f $C.matched
    /usr/bin/time -v ./img-fp-head $p -r -x '*' -t 8 --no-last --cache $C $a -o $f.json 2> $f.err
    rm -f $C.matched
    $PY -I $B/analyse.py --corpus=$p $f.json > $f.an.part 2>&1 && mv $f.an.part $f.an
    rm -f $f.json; say "sweep $n $t $(grep -E 'User time|Elapsed' $f.err | awk '{print $NF}' | tr '\n' ' ')"
  done
done
for cfg in "${CONFIGS[@]}"; do t=${cfg%%:*}; a=${cfg#*:}; f=disc/$t
  [ -s $f.score ] && continue; rm -f $X/cache57/c.db.matched
  /usr/bin/time -v ./img-fp-head $X/queries/images/queries --against $X/refs0/images/references --follow-symlinks $a -t 8 --no-last --cache $X/cache57/c.db -o $f.json 2> $f.err
  rm -f $X/cache57/c.db.matched
  $PY -I dscore.py $f.json > $f.score.part 2>&1 && mv $f.score.part $f.score
  rm -f $f.json; say "disc $t $(grep -E 'User time|Elapsed' $f.err | awk '{print $NF}' | tr '\n' ' ')"
done
say ALL DONE
