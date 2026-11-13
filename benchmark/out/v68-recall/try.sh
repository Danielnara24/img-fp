#!/bin/bash
# try.sh NAME [env...]: the quick build over both corpora from the cache, scored as a classifier of redundant files.
cd /home/daniel/Documents/Vscode_repositories/img-fp
V=benchmark/out/v68-recall; n=$1; shift; mkdir -p $V/$n
for c in ALL2 ALL; do
  env "$@" X=1 target/iter/release/img-fp /home/daniel/Documents/IMGS-$c -r -x '*' -t 8 -v --no-last --cache benchmark/out/v67-keep/cache/c.db -o $V/$n/$c.json > $V/$n/$c.err 2>&1
  echo "== $c $n  ($(grep 'pace: grouping' $V/$n/$c.err | grep -o 'in [0-9.]*s'))"
  vendor/venv/bin/python $V/redundant.py /home/daniel/Documents/IMGS-$c/derived/manifest.csv $V/$n/$c.json | grep -v '^$' | tail -2 | cut -c1-250
done
