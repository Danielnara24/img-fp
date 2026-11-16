#!/bin/bash
# imgs.sh BINARY TAG: IMGS-ALL 1-3 at the defaults on v71's cache, analyse.py;
# then families.py into fam-TAG. Base is v72-defaults (same base binary).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v74-reach; B=$O/../..
D=/home/daniel/Documents; C=$B/out/v71-bars/cache/c.db; PY=$B/../vendor/venv/bin/python
bin=$1; tag=$2; mkdir -p $O/run-$tag; cd $O
for c in ALL ALL2 ALL3; do f=run-$tag/$c; p=$D/IMGS-$c
  [ -s $f.an ] && continue; rm -f $C.matched
  /usr/bin/time -v $O/$bin $p -r -x '*' -t 8 --no-last --cache $C -v -o $f.json 2> $f.err
  rm -f $C.matched
  $PY -I $B/analyse.py --corpus=$p --per-transform $f.json > $f.an.part 2>&1 && mv $f.an.part $f.an
done
if [ ! -s fam-$tag.score ]; then
  cd $B && $PY families.py run $O/$bin $O/fam-$tag --cache $C --corpus all all2 all3 i5 hdev
  $PY families.py score $O/fam-$tag -v > $O/fam-$tag.score 2>&1
  $PY families.py compare out/v72-defaults/fam $O/fam-$tag > $O/fam-$tag.compare 2>&1
fi
echo done > $O/imgs-$tag.done
