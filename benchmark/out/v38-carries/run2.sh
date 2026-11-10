#!/bin/bash
# The same, after carries_more was limited to pixels proven the same (Link::Exact vs Twin).
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
"$O/img-fp-new2" -r -x '*' --cache "$O/cache-new.bin" --no-last /home/daniel/Documents/IMGS-ALL -o "$O/all-new2.json" > "$O/all-new2.log" 2>&1
echo "all new2 exit $?" >> "$O/progress.txt"
for v in base new new2; do $PY "$B/suggest_score.py" "$O/all-$v.json" > "$O/score-$v.txt" 2>&1; done
echo "scores exit $?" >> "$O/progress.txt"
$PY "$B/pairs.py" run "$O/img-fp-new2" "$O/pairs-new2" --cache "$O/cache-new.bin" > "$O/pairs-run-new2.log" 2>&1
$PY "$B/pairs.py" score "$O/pairs-new2" > "$O/pairs-score-new2.txt" 2>&1
$PY "$B/pairs.py" compare "$O/pairs-base" "$O/pairs-new" "$O/pairs-new2" > "$O/pairs-compare.txt" 2>&1
echo "done2" >> "$O/progress.txt"
