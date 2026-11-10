#!/bin/bash
# Keep rule after 0.69.0 (Ctx::carries_more): base 0.69.0 vs new, cold IMGS-ALL, then pairs.py.
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
for v in base new; do
  rm -f "$O/cache-$v.bin"
  /usr/bin/time -v "$O/img-fp-$v" -r -x '*' --cache "$O/cache-$v.bin" --no-last /home/daniel/Documents/IMGS-ALL -o "$O/all-$v.json" > "$O/all-$v.log" 2>&1
  echo "all $v exit $?" >> "$O/progress.txt"
done
for v in base new; do
  $PY "$B/suggest_score.py" "$O/all-$v.json" > "$O/score-$v.txt" 2>&1
  echo "score $v exit $?" >> "$O/progress.txt"
done
for v in base new; do
  $PY "$B/pairs.py" run "$O/img-fp-$v" "$O/pairs-$v" --cache "$O/cache-$v.bin" > "$O/pairs-run-$v.log" 2>&1
  echo "pairs $v exit $?" >> "$O/progress.txt"
done
$PY "$B/pairs.py" score "$O/pairs-base" > "$O/pairs-score-base.txt" 2>&1
$PY "$B/pairs.py" score "$O/pairs-new" > "$O/pairs-score-new.txt" 2>&1
$PY "$B/pairs.py" compare "$O/pairs-base" "$O/pairs-new" > "$O/pairs-compare.txt" 2>&1
echo done >> "$O/progress.txt"
