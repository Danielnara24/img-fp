#!/bin/bash
# 0.72.0 at --min-pixel-correlation 0.7: a block flat on one side only counts
# at |r| times SSIM's contrast term 2 sa sb / (sa^2 + sb^2) (IMGFP_ONE unset),
# against r = 0 (IMGFP_ONE=zero, v43's rule), plain |r| (raw) and |r| of
# plane-detrended blocks (plane). Baseline 0.71.0 and I-zero at 0.7: v42.
set -u
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
C=$B/out/v40-keep/cache.bin
A=/home/daniel/Documents/IMGS-ALL; I5=/home/daniel/Documents/IMGS5
P=/home/daniel/Documents/IMGS-PROVENANCE; F=/home/daniel/Downloads/archive
X="$O/img-fp-c"
one() { # tag corpus mode name args...
  local tag=$1 corpus=$2 mode=$3 n=$4; shift 4
  local out="$O/$tag-$mode-$n"
  [ -s "$out.txt" ] && return
  rm -f "$C.matched"
  env IMGFP_ONE=$mode "$X" -r -x '*' --cache "$C" --no-last "$corpus" "$@" -o "$out.json" > "$out.log" 2>&1
  $PY "$B/analyse.py" "$out.json" --corpus="$corpus" --per-transform > "$out.txt" 2>&1
  [ "$tag-$mode-$n" = "all-ssim-default" ] && $PY "$B/suggest_score.py" "$out.json" > "$out.suggest.txt" 2>&1
  [ "$n" = default ] || rm -f "$out.json"
  echo "$(date +%T) $tag $mode $n" >> "$O/progress.txt"
}
for m in ssim raw plane; do one i5 $I5 $m default; done
for m in ssim raw; do one all $A $m default; done
# IMGS5's seeds alone, every bar
mkdir -p "$O/seeds"
for c in 0.9 0.85 0.8 0.75 0.7 0.65 0.6 0.55 0.5 0.45 0.4 0.35 0.3 0.25 0.2; do
  rm -f "$O/seeds/cache.bin.matched"
  env IMGFP_ONE=ssim "$X" --cache "$O/seeds/cache.bin" --no-last $I5 --min-pixel-correlation $c -o "$O/seeds/ssim-$c.json" > "$O/seeds/ssim-$c.log" 2>&1
done
echo "$(date +%T) seeds" >> "$O/progress.txt"
for t in dated same-time; do
  rm -f "$C.matched"
  env IMGFP_ONE=ssim "$X" -r --cache "$C" --no-last "$P/$t" -o "$O/prov-$t.json" > "$O/prov-$t.log" 2>&1
  $PY "$B/score_provenance.py" "$O/prov-$t.json" > "$O/provscore-$t.txt" 2>&1
done
echo "$(date +%T) prov" >> "$O/progress.txt"
cp $B/out/v43-flat06/found-cache.bin "$O/found-cache.bin"
env IMGFP_ONE=ssim "$X" -r --cache "$O/found-cache.bin" --no-last $F -o "$O/found-ssim.json" > "$O/found-ssim.log" 2>&1
rm -f "$O/found-cache.bin.matched"
env IMGFP_ONE=zero "$X" -r --cache "$O/found-cache.bin" --no-last $F -o "$O/found-zero.json" > "$O/found-zero.log" 2>&1
echo "$(date +%T) found" >> "$O/progress.txt"
for m in ssim zero; do
  env IMGFP_ONE=$m $PY "$B/pairs.py" run "$X" "$O/pairs-$m" --cache "$C" > "$O/pairs-run-$m.log" 2>&1
  $PY "$B/pairs.py" score "$O/pairs-$m" > "$O/pairs-score-$m.txt" 2>&1
  $PY "$B/pairs.py" compare $B/out/v39-audit/pairs-new "$O/pairs-$m" > "$O/pairs-compare-$m.txt" 2>&1
  echo "$(date +%T) pairs $m" >> "$O/progress.txt"
done
sweep() { # tag corpus
  for p in 8 6 5 4 3; do one $1 $2 ssim points$p --min-aligned-points $p; done
  for o in 0.75 0.65 0.6 0.5 0.4 0.3 0.25; do one $1 $2 ssim overlap$o --min-frame-overlap $o; done
  for c in 0.9 0.8 0.6 0.5 0.4 0.3 0.2; do one $1 $2 ssim corr$c --min-pixel-correlation $c; done
  for k in 100 50 25 10; do one $1 $2 ssim k$k -k $k; done
}
sweep i5 $I5
sweep all $A
echo "$(date +%T) done" >> "$O/progress.txt"
