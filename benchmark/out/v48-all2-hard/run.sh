#!/bin/bash
# IMGS-ALL2 second pass: regenerate, then img-fp 0.74.0 cold, then scored.
CLEAN=1 /home/daniel/Documents/IMGS-v2/generate.sh || exit 1
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
O=out/v48-all2-hard; C=/home/daniel/Documents/IMGS-ALL2; PY=../vendor/venv/bin/python
echo "=== bench $(date)"
$PY bench.py --only imgfp --corpus $C --out $O
echo "=== score $(date)"
$PY analyse.py --corpus=$C --per-transform $O/imgfp.json > $O/analyse.out 2>&1
$PY score.py --corpus $C --per-transform --csv $O/per_transform.csv $O/imgfp.json > $O/score.out 2>&1
$PY suggest_score.py $O/imgfp.json --manifest $C/derived/manifest.csv > $O/suggest.out 2>&1
echo "=== done $(date)"
