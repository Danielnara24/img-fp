#!/bin/bash
# battery.sh BINARY NAME ENV...: both corpora, the small families and the two-file families for one setting.
cd /home/daniel/Documents/Vscode_repositories/img-fp
V=benchmark/out/v68-recall; bin=$1; n=$2; shift 2; mkdir -p $V/$n
{
for c in ALL2 ALL; do
  env "$@" X=1 $bin /home/daniel/Documents/IMGS-$c -r -x '*' -t 8 -v --no-last --cache benchmark/out/v67-keep/cache/c.db -o $V/$n/$c.json > $V/$n/$c.err 2>&1
  echo "== $c $n  ($(grep 'pace: grouping' $V/$n/$c.err | grep -o 'in [0-9.]*s'))"
  vendor/venv/bin/python $V/redundant.py /home/daniel/Documents/IMGS-$c/derived/manifest.csv $V/$n/$c.json | grep -v '^$' | tail -2 | cut -c1-250
done
vendor/venv/bin/python $V/small.py $bin $n/small "$@"
if [ -z "$NOPAIRS" ]; then
  env "$@" X=1 vendor/venv/bin/python benchmark/pairs.py run $bin $V/$n/pairs --cache benchmark/out/v67-keep/cache/c.db > $V/$n/pairs.log 2>&1
  vendor/venv/bin/python benchmark/pairs.py score $V/$n/pairs | tail -22
fi
echo "done $n"
} > $V/$n.out 2>&1
