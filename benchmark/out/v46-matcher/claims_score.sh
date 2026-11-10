#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
P=../vendor/venv/bin/python
O=out/v46-matcher
$P families.py compare $O/f143 $O/c-none $O/c-bridge $O/c-cover $O/c-cycle $O/c-clean
for d in f143 c-none c-bridge c-cover c-cycle c-clean; do
  echo "== $d"
  $P analyse.py $O/$d/all-full.json 2>&1 | grep -E "F1|FP:|perfect|cross .*: [0-9,]{3,} pairs"
  $P analyse.py --corpus=/home/daniel/Documents/IMGS5 $O/$d/i5-full.json 2>&1 | grep -E "F1|FP:|cross .*: [0-9,]{3,} pairs"
  $P suggest_score.py $O/$d/all-full.json 2>&1 | grep -E "content lost by|original or exact|faithful full" | cut -c1-90
done
