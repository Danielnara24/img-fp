#!/bin/bash
# img-fp 0.74.0 baseline on IMGS-ALL2 (cold), then scored.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
O=out/v47-all2; C=/home/daniel/Documents/IMGS-ALL2; PY=../vendor/venv/bin/python
echo "=== bench $(date)"
$PY bench.py --only imgfp --corpus $C --out $O
echo "=== analyse $(date)"
R=$(ls $O/*imgfp*.json | grep -v metrics | head -1); echo "report: $R"
$PY analyse.py --corpus=$C --per-transform "$R" > $O/analyse.out 2>&1
echo "=== suggest $(date)"
$PY suggest_score.py "$R" --manifest $C/derived/manifest.csv > $O/suggest.out 2>&1
echo "=== done $(date)"
