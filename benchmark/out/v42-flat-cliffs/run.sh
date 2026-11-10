#!/bin/bash
# I (IMGFP_FLAT_BOTH: a block flat on one side only counts as r = 0) against
# the same binary without it, at every step of every bar, IMGS-ALL and IMGS5.
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
C=$B/out/v40-keep/cache.bin
one() { # corpus-tag corpus variant name args...
  local tag=$1 corpus=$2 v=$3 n=$4; shift 4
  local out="$O/$tag-$v-$n"
  [ -s "$out.txt" ] && return
  rm -f "$C.matched"
  local e=X=1; [ "$v" = flat ] && e=IMGFP_FLAT_BOTH=1
  env $e "$O/img-fp-i" -r -x '*' --cache "$C" --no-last "$corpus" "$@" -o "$out.json" > "$out.log" 2>&1
  $PY "$B/analyse.py" "$out.json" --corpus="$corpus" --per-transform > "$out.txt" 2>&1
  rm -f "$out.json"
  echo "$tag $v $n" >> "$O/progress.txt"
}
sweep() { # tag corpus
  for v in base flat; do
    one $1 $2 $v default
    for p in 8 6 5 4 3; do one $1 $2 $v points$p --min-aligned-points $p; done
    for o in 0.75 0.65 0.6 0.5 0.4 0.3 0.25; do one $1 $2 $v overlap$o --min-frame-overlap $o; done
    for c in 0.9 0.8 0.6 0.5 0.4 0.3 0.2; do one $1 $2 $v corr$c --min-pixel-correlation $c; done
    for k in 100 50 25 10; do one $1 $2 $v k$k -k $k; done
  done
}
sweep i5 /home/daniel/Documents/IMGS5
sweep all /home/daniel/Documents/IMGS-ALL
echo done >> "$O/progress.txt"
