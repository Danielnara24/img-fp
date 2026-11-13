#!/bin/bash
# The final measurements, HEAD (base) against the new rule (new), both plain builds, one cache.
cd /home/daniel/Documents/Vscode_repositories/img-fp
O=benchmark/out/v67-keep; D=/home/daniel/Documents
for c in ALL2 ALL; do
  $O/new/img-fp $D/IMGS-$c -r -x '*' -t 8 -v --no-last --cache $O/cache/c.db -o $O/new/$c.json > $O/new/$c.err 2>&1
done
# The suggestion's cost: base new new base, cached, IMGS-ALL2.
for w in base new new base; do
  $O/$w/img-fp $D/IMGS-ALL2 -r -x '*' -t 8 -v --no-last --cache $O/cache/c.db -o $O/cost.json 2>&1 | grep -E 'pace: grouping|suggested' | sed "s/^/$w /" >> $O/cost.txt
done
for w in base new; do
  for t in dated same-time; do
    $O/$w/img-fp $D/IMGS-PROVENANCE/$t -r -x '*' -t 8 --no-last --cache $O/cache/c.db -o $O/$w/prov-$t.json > $O/$w/prov-$t.err 2>&1
  done
  $O/$w/img-fp $D/IMGS-AGAINST/scan --against $D/IMGS-AGAINST/library --follow-symlinks -r -x '*' -t 8 --no-last --cache $O/cache/c.db -o $O/$w/against.json > $O/$w/against.err 2>&1
  $O/$w/img-fp /home/daniel/Downloads/archive -r -t 8 --no-last --cache $O/cache/c.db -o $O/$w/found.json > $O/$w/found.err 2>&1
done
vendor/venv/bin/python benchmark/pairs.py run $O/new/img-fp $O/new/pairs --cache $O/cache/c.db > $O/new/pairs.log 2>&1
echo done > $O/final.done
