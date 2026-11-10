#!/bin/bash
# Scores run11: cliffs (analyse.py per step, then cliff_table.py), --detail containment, proxy, cold cost.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed; B=$O/../..
P=$B/../vendor/venv/bin/python
cd $O
an() { if [ $1 = all2 ]; then $P $B/analyse.py --corpus=/home/daniel/Documents/IMGS-ALL2 $2; else $P $B/analyse.py $2; fi; }
for f in cliffs/*.json; do [ -s ${f%.json}.txt ] || an $(basename $f | cut -d- -f1) $f > ${f%.json}.txt 2>&1 & 
  while [ $(jobs -r | wc -l) -ge 3 ]; do sleep 2; done; done; wait
for b in head ship; do for d in 1024 2048; do an all2 detail/$b-$d.json > detail/$b-$d.txt 2>&1; done; done
$P cliff_table.py > cliff_table.out
$P detail_contain.py > detail_contain.out
python3 /home/daniel/Documents/DISC21/proxy/cmp.py proxy/head.json proxy/ship.json > proxy/cmp.out
echo scored
