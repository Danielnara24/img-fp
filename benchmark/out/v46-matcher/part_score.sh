#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
P=../vendor/venv/bin/python; O=out/v46-matcher
$P families.py compare $O/p-off $O/p-on
for r in off on; do
  echo "== p-$r default"
  $P analyse.py $O/p-$r/all-full.json 2>&1 | grep -E "^   F1|FP:|perfect|[0-9,]{3,} pairs"
  $P analyse.py --corpus=/home/daniel/Documents/IMGS5 $O/p-$r/i5-full.json 2>&1 | grep -E "^   F1|FP:|[0-9,]{3,} pairs"
  $P suggest_score.py $O/p-$r/all-full.json 2>&1 | grep -E "^actions|content lost|original or exact|faithful full|only in a composite" | cut -c1-110
done
for f in $O/cliff-off/*.json; do
  b=$(basename $f .json); t=${b%%-*}
  for r in off on; do
    g=$O/cliff-$r/$b.json
    if [ $t = i5 ]; then c=--corpus=/home/daniel/Documents/IMGS5; else c=; fi
    echo "$r $b $($P analyse.py $c $g 2>&1 | grep -E '^   F1|FP:' | tr -s ' ' | tr '\n' ' ') | $($P analyse.py $c $g 2>&1 | grep -E 'cross .*: [0-9,]{3,} pairs' | grep -vE ': [12][0-9]{2} pairs' | sed 's/.*cross //; s/.jpg//g; s/ pairs//' | tr '\n' ';')"
  done
done
