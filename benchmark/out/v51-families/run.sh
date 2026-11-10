#!/bin/bash
# families.py on IMGS-ALL (five corpora now) and IMGS-ALL2 (hard), img-fp 0.74.0.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
PY=../vendor/venv/bin/python; O=out/v51-families
echo "=== run $(date)"
$PY families.py run ../target/plain/release/img-fp $O --cache $O/cache.bin --corpus all all2
echo "=== score $(date)"
$PY families.py score $O > $O/score.out 2>&1
echo "=== done $(date)"
