#!/bin/bash
# E2: -k and the three bars on small families.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v45-k
O=$PWD; V=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v40-keep/cache.bin
one() { # list tag-suffix args...
  local l=$1 s=$2; shift 2; local t=$l@$s
  [ -s r/$t.json ] && return
  case $l in *-f*) C=$O/fcache.bin;; *) C=$V;; esac
  /usr/bin/time -f "%e s wall %U+%S cpu %M KB" ./img-fp --cache $C --no-last --from-file lists/$l.txt "$@" -o r/$t.json > r/$t.log 2>&1
  echo "$(date +%T) $t" >> progress2.txt
}
L="i5-n0-d0 i5-n1-d1 i5-n2-d1 i5-n2-d2 i5-n2-d3 i5-n10-d1 i5-n30-d1 all-n0-d0 all-n1-d1 all-n2-d1 all-n2-d2 all-n2-d3 all-n10-d1 all-n30-d1"
for l in $L; do for k in 10 25 50 100 250 400; do one $l k$k -k $k; done; done
for l in $L; do
  for c in 0.9 0.8 0.75 0.65 0.6 0.5; do one $l c$c --min-pixel-correlation $c; done
  for o in 0.75 0.6 0.5 0.4; do one $l o$o --min-frame-overlap $o; done
  for p in 8 6 5 4 3; do one $l p$p --min-aligned-points $p; done
done
echo done >> progress2.txt
