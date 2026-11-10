#!/bin/bash
set -u
cd "$(dirname "$0")"
echo 1000 > /proc/self/oom_score_adj
B=./img-fp-head; D=/home/daniel/Documents/IMGS-ALL2; C=$PWD/c.db
for d in 1024 512; do
  rm -f $C.matched
  /usr/bin/time -v $B $D -r -x '*' -t 8 -v --no-last --cache $C --detail $d -o a2-$d.json --dump a2-$d.csv 2> a2-$d.err
  echo "done $d exit $?"
done
