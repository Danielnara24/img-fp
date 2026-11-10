#!/bin/bash
# 0.72.0 as shipped: a block flat on one side only counts as r = 0, and
# --min-pixel-correlation 0.6. Against 0.71.0 (v39-audit).
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
V=$B/out/v39-audit
C=$B/out/v40-keep/cache.bin
A=/home/daniel/Documents/IMGS-ALL
I5=/home/daniel/Documents/IMGS5
P=/home/daniel/Documents/IMGS-PROVENANCE
F=/home/daniel/Downloads/archive
"$O/img-fp-new" -r -x '*' --cache "$C" --no-last $A -o "$O/all.json" > "$O/all.log" 2>&1
$PY "$B/analyse.py" "$O/all.json" --per-transform > "$O/all.analyse.txt" 2>&1
$PY "$B/suggest_score.py" "$O/all.json" > "$O/all.suggest.txt" 2>&1
echo "all done" >> "$O/progress.txt"
"$O/img-fp-new" -r -x '*' --cache "$C" --no-last $I5 -o "$O/i5.json" > "$O/i5.log" 2>&1
$PY "$B/analyse.py" "$O/i5.json" --corpus=$I5 > "$O/i5.analyse.txt" 2>&1
echo "i5 done" >> "$O/progress.txt"
for t in dated same-time; do
  "$O/img-fp-new" -r --cache "$C" --no-last "$P/$t" -o "$O/prov-$t.json" > "$O/prov-$t.log" 2>&1
  $PY "$B/score_provenance.py" "$O/prov-$t.json" > "$O/provscore-$t.txt" 2>&1
done
echo "prov done" >> "$O/progress.txt"
for v in new base; do
  bin="$O/img-fp-new"; [ $v = base ] && bin="$V/img-fp-new"
  "$bin" -r --cache "$O/found-cache.bin" --no-last $F -o "$O/found-$v.json" > "$O/found-$v.log" 2>&1
done
echo "found done" >> "$O/progress.txt"
$PY "$B/pairs.py" run "$O/img-fp-new" "$O/pairs-new" --cache "$C" > "$O/pairs-run.log" 2>&1
$PY "$B/pairs.py" score "$O/pairs-new" > "$O/pairs-score.txt" 2>&1
$PY "$B/pairs.py" compare "$V/pairs-new" "$O/pairs-new" > "$O/pairs-compare.txt" 2>&1
echo "pairs done" >> "$O/progress.txt"
one() { # tag corpus name args...
  local tag=$1 corpus=$2 n=$3; shift 3
  "$O/img-fp-new" -r -x '*' --cache "$C" --no-last "$corpus" "$@" -o "$O/c-$tag-$n.json" > "$O/c-$tag-$n.log" 2>&1
  $PY "$B/analyse.py" "$O/c-$tag-$n.json" --corpus="$corpus" --per-transform > "$O/c-$tag-$n.txt" 2>&1
  rm -f "$O/c-$tag-$n.json"
  echo "cliff $tag $n" >> "$O/progress.txt"
}
for spec in "i5 $I5" "all $A"; do
  set -- $spec; tag=$1; corpus=$2
  for p in 8 6 5 4 3; do one $tag $corpus points$p --min-aligned-points $p; done
  for o in 0.75 0.65 0.6 0.5 0.4 0.3 0.25; do one $tag $corpus overlap$o --min-frame-overlap $o; done
  for k in 100 50 25 10; do one $tag $corpus k$k -k $k; done
done
echo done >> "$O/progress.txt"
