#!/bin/bash
# 0.70.0 audit fixes B-G against v38-carries' img-fp-new2 (= 0.70.0): IMGS-ALL cold, keep rule, pairs.py.
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
V=$B/out/v38-carries
"$O/img-fp-new" -r -x '*' --cache "$O/cache-new.bin" --no-last /home/daniel/Documents/IMGS-ALL -o "$O/all-new.json" > "$O/all-new.log" 2>&1
echo "all new exit $?" >> "$O/progress.txt"
$PY "$B/suggest_score.py" "$O/all-new.json" > "$O/score-new.txt" 2>&1
echo "score exit $?" >> "$O/progress.txt"
$PY "$B/pairs.py" run "$O/img-fp-new" "$O/pairs-new" --cache "$O/cache-new.bin" > "$O/pairs-run-new.log" 2>&1
$PY "$B/pairs.py" score "$O/pairs-new" > "$O/pairs-score-new.txt" 2>&1
$PY "$B/pairs.py" compare "$V/pairs-new2" "$O/pairs-new" > "$O/pairs-compare.txt" 2>&1
echo "done" >> "$O/progress.txt"
