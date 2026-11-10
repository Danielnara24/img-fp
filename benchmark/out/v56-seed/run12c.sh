#!/bin/bash
# Compress the cliff reports (32 GB filled the disk), run shard 0 + proxy (run12.sh), then score one at a time.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed; B=$O/../..; P=$B/../vendor/venv/bin/python
cd $O
for f in cliffs/*.json; do gzip -1 $f; done
echo "$(date +%T) cliffs compressed, $(df -h /home | tail -n 1 | awk '{print $4}') free" >> run12.out
./run12.sh
an() { if [ $1 = all2 ]; then $P $B/analyse.py --corpus=/home/daniel/Documents/IMGS-ALL2 $2; else $P $B/analyse.py $2; fi; }
for f in cliffs/*.json.gz; do t=${f%.json.gz}.txt; [ -s $t ] && continue
  an $(basename $f | cut -d- -f1) /dev/stdin < <(zcat $f) > $t.part 2>&1 && mv $t.part $t; done
for b in head ship; do for d in 1024 2048; do an all2 detail/$b-$d.json > detail/$b-$d.txt 2>&1; done; done
$P cliff_table.py > cliff_table.out 2>&1
$P detail_contain.py > detail_contain.out 2>&1
python3 /home/daniel/Documents/DISC21/proxy/cmp.py proxy/head.json proxy/ship.json > proxy/cmp.out 2>&1
python3 disc/score.py > disc/score.out 2>&1
echo "$(date +%T) all scored" >> run12.out
