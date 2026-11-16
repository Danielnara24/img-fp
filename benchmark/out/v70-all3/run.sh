#!/bin/bash
# The field on IMGS-ALL3 (the third catalogue: 60 transformations neither
# earlier corpus holds, 30 chains of two), img-fp 0.85.0 (plain build) and
# five of the field, every tool cold, alone, evicted, cooled. As
# v54-field/run.sh: the tools that may run the machine out of memory first.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
PY=../vendor/venv/bin/python; O=out/v70-all3; D=/home/daniel/Documents/IMGS-ALL3
echo "=== idle $(date)"; free -m
sleep 300
echo "=== bench $(date)"
$PY bench.py --corpus $D --out $O/all3 --timeout 6 \
  --only imagededup_cnn,pdq,sscd,czkawka_tuned,imagededup_phash,imgfp
echo "=== score $(date)"
( cd $O/all3 && ls *.json | grep -v metrics | grep -v run_meta > tools.txt )
$PY score.py --corpus $D --per-transform --csv $O/all3/per_transform.csv $(sed "s|^|$O/all3/|" $O/all3/tools.txt) > $O/all3/score.txt 2>&1
$PY analyse.py --corpus=$D --per-transform $O/all3/imgfp.json > $O/all3/analyse.txt 2>&1
echo "=== done $(date)"
