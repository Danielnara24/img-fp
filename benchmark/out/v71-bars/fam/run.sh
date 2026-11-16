#!/bin/bash
# v71: small-family precision (families.py) at the defaults and at the three
# candidate bars the sweep favoured: points 5, points 3, correlation 0.65.
# IMGS-ALL, IMGS-ALL2, IMGS-ALL3, IMGS5, HOLI-DEV; one cache (v71's, which
# holds the three corpora; the rest is analysed by the first run).
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
PY=../vendor/venv/bin/python; O=out/v71-bars/fam; BIN=out/v71-bars/img-fp-head; C=out/v71-bars/cache/c.db
echo 1000 > /proc/self/oom_score_adj
for cfg in "base:" "p5:--min-aligned-points 5" "p3:--min-aligned-points 3" "c0.65:--min-pixel-correlation 0.65"; do
  t=${cfg%%:*}; a=${cfg#*:}
  [ -s $O/$t.score ] && continue
  echo "=== $t $(date)"
  $PY families.py run $BIN $O/$t --cache $C --corpus all all2 all3 i5 hdev -- $a
  $PY families.py score $O/$t -v > $O/$t.score 2>&1
done
$PY families.py compare $O/base $O/p5 $O/p3 $O/c0.65 > $O/compare.out 2>&1
echo "=== done $(date)"
