#!/bin/bash
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
run() { local n=$1; shift
  env "$@" "$O/img-fp-var" -r -x '*' --cache "$O/cache.bin" --no-last /home/daniel/Documents/IMGS-ALL -o "$O/all-$n.json" > "$O/all-$n.log" 2>&1
  $PY "$B/suggest_score.py" "$O/all-$n.json" > "$O/score-$n.txt" 2>&1
  echo "$n done" >> "$O/progress.txt"
}
run v1 IMGFP_B_NARROW=1 IMGFP_A_SMALLER=1
run v2 IMGFP_B_NARROW=1
