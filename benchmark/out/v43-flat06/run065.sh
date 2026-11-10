#!/bin/bash
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
C=$B/out/v40-keep/cache.bin
until grep -q "^done" "$O/progress.txt"; do sleep 30; done
for c in 0.7 0.65; do
  "$O/img-fp-new" -r --cache "$O/found-cache.bin" --no-last /home/daniel/Downloads/archive --min-pixel-correlation $c -o "$O/found-new-$c.json" > /dev/null 2>&1
done
for spec in "i5 /home/daniel/Documents/IMGS5" "all /home/daniel/Documents/IMGS-ALL"; do
  set -- $spec
  "$O/img-fp-new" -r -x '*' --cache "$C" --no-last "$2" --min-pixel-correlation 0.65 -o "$O/c-$1-corr0.65.json" > /dev/null 2>&1
  $PY "$B/analyse.py" "$O/c-$1-corr0.65.json" --corpus="$2" --per-transform > "$O/c-$1-corr0.65.txt" 2>&1
  [ $1 = all ] && $PY "$B/suggest_score.py" "$O/c-all-corr0.65.json" > "$O/c-all-corr0.65.suggest.txt" 2>&1
  rm -f "$O/c-$1-corr0.65.json"
done
echo "065 done" >> "$O/progress.txt"
