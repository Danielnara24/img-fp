#!/bin/bash
# Regenerate the second-generation corpus and merge it; safe to re-run (resumes:
# only after an interruption: a changed format moves later names, so after
# changing the catalogue run it as `CLEAN=1 generate.sh`). Parts run in parallel.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/corpus || exit 1
PY=../../vendor/venv/bin/python; V=/home/daniel/Documents/IMGS-v2
echo "=== generate $(date)"
for c in IMGS IMGS2 IMGS3 IMGS4 IMGS5; do
  $PY make_variants.py --corpus $V/$c ${CLEAN:+--clean} > $V/gen-$c.log 2>&1 &
done
wait
grep -H -A12 "wrote\|failures" $V/gen-*.log | grep -v "^\S*-  [^ ]* [0-9]*x[0-9]*$"
echo "=== combine $(date)"
$PY make_combined.py --src $V \
  --parts IMGS,IMGS2,IMGS3,IMGS4,IMGS5 --out /home/daniel/Documents/IMGS-ALL2 --clean
echo "=== done $(date)"
