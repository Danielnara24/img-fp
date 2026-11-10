#!/bin/bash
# E1: every list at the defaults.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v45-k
O=$PWD
for l in lists/i5-n*.txt lists/i5-f*.txt lists/all-n[0-9]-*.txt lists/all-n10-*.txt lists/all-n30-*.txt lists/all-f*.txt; do
  t=$(basename $l .txt)
  [ -s r/$t.json ] && continue
  case $t in *-f*) C=$O/fcache.bin;; *) C=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v40-keep/cache.bin;; esac
  /usr/bin/time -f "%e s wall %U+%S cpu %M KB" ./img-fp --cache $C --no-last --from-file $l -o r/$t.json > r/$t.log 2>&1
  echo "$(date +%T) $t" >> progress.txt
done
echo done >> progress.txt
