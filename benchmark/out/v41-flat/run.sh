#!/bin/bash
# I: pixel-check blocks flat on one side only count as r = 0 (IMGFP_FLAT_BOTH),
# against the same binary without it. Cache shared with v40-keep.
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
C=$B/out/v40-keep/cache.bin
run() { # name corpus env... -- extra args
  local n=$1 corpus=$2; shift 2
  local envs=(); while [ "$1" != "--" ]; do envs+=("$1"); shift; done; shift
  rm -f "$C.matched"
  env "${envs[@]}" "$O/img-fp-i" -r -x '*' --cache "$C" --no-last "$corpus" "$@" -o "$O/$n.json" > "$O/$n.log" 2>&1
  $PY "$B/analyse.py" "$O/$n.json" --corpus="$corpus" > "$O/$n.analyse.txt" 2>&1
  echo "$n done" >> "$O/progress.txt"
}
A=/home/daniel/Documents/IMGS-ALL
run all-base $A X=1 --
run all-flat $A IMGFP_FLAT_BOTH=1 --
$PY "$B/suggest_score.py" "$O/all-flat.json" > "$O/all-flat.suggest.txt" 2>&1
run i5-base /home/daniel/Documents/IMGS5 X=1 --
run i5-flat /home/daniel/Documents/IMGS5 IMGFP_FLAT_BOTH=1 --
run all-base-ov05 $A X=1 -- --min-frame-overlap 0.5
run all-flat-ov05 $A IMGFP_FLAT_BOTH=1 -- --min-frame-overlap 0.5
run all-base-p3 $A X=1 -- --min-aligned-points 3
run all-flat-p3 $A IMGFP_FLAT_BOTH=1 -- --min-aligned-points 3
echo "done" >> "$O/progress.txt"
