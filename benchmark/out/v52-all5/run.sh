#!/bin/bash
# IMGS-ALL with all five corpora (first-generation transforms), img-fp 0.74.0 cold, scored.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
O=out/v52-all5; C=/home/daniel/Documents/IMGS-ALL; PY=../vendor/venv/bin/python
echo "=== bench $(date)"
$PY bench.py --only imgfp --corpus $C --out $O
echo "=== score $(date)"
$PY analyse.py --corpus=$C --per-transform $O/imgfp.json > $O/analyse.out 2>&1
$PY score.py --corpus $C --per-transform --csv $O/per_transform.csv $O/imgfp.json > $O/score.out 2>&1
$PY suggest_score.py $O/imgfp.json --manifest $C/derived/manifest.csv > $O/suggest.out 2>&1
echo "=== done $(date)"
