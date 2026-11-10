#!/bin/bash
# 0.72.0 keep rule (A: larger lossy file; B: lossless-only pixels do not choose),
# n1 = with the SOFT_TIE filter, n2 = without (IMGFP_NO_SOFT_TIE). Base: v39-audit (0.71.0).
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
V=$B/out/v39-audit
P=/home/daniel/Documents/IMGS-PROVENANCE
cp "$V/cache-new.bin" "$O/cache.bin"; cp "$V/cache-new.bin.heads" "$O/cache.bin.heads"
run() { # name env...
  local n=$1; shift
  env "$@" "$O/img-fp-new" -r -x '*' --cache "$O/cache.bin" --no-last /home/daniel/Documents/IMGS-ALL -o "$O/all-$n.json" > "$O/all-$n.log" 2>&1
  echo "all $n exit $?" >> "$O/progress.txt"
  $PY "$B/suggest_score.py" "$O/all-$n.json" > "$O/score-$n.txt" 2>&1
  echo "score $n exit $?" >> "$O/progress.txt"
}
run n1 X=1
run n2 IMGFP_NO_SOFT_TIE=1
for t in dated same-time; do
  for v in base n1 n2; do
    bin="$O/img-fp-new"; [ $v = base ] && bin="$V/img-fp-new"
    e=X=1; [ $v = n2 ] && e=IMGFP_NO_SOFT_TIE=1
    env $e "$bin" -r --cache "$O/cache.bin" --no-last "$P/$t" -o "$O/prov-$t-$v.json" > "$O/prov-$t-$v.log" 2>&1
    $PY "$B/score_provenance.py" "$O/prov-$t-$v.json" > "$O/provscore-$t-$v.txt" 2>&1
  done
done
echo "prov done" >> "$O/progress.txt"
rm -f "$O/cache.bin.matched"
"$O/img-fp-new" -t 3 -r -x '*' --cache "$O/cache.bin" --no-last /home/daniel/Documents/IMGS-ALL -o "$O/all-t3.json" > "$O/all-t3.log" 2>&1
echo "t3 exit $?" >> "$O/progress.txt"
for n in n1 n2; do
  e=X=1; [ $n = n2 ] && e=IMGFP_NO_SOFT_TIE=1
  env $e $PY "$B/pairs.py" run "$O/img-fp-new" "$O/pairs-$n" --cache "$O/cache.bin" > "$O/pairs-run-$n.log" 2>&1
  $PY "$B/pairs.py" score "$O/pairs-$n" > "$O/pairs-score-$n.txt" 2>&1
  $PY "$B/pairs.py" compare "$V/pairs-new" "$O/pairs-$n" > "$O/pairs-compare-$n.txt" 2>&1
  echo "pairs $n done" >> "$O/progress.txt"
done
echo "done" >> "$O/progress.txt"
