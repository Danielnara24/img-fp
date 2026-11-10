#!/bin/bash
# base against new, each run: report and dump byte for byte, else what differs.
cd "$(dirname "$0")"
for f in *-base.json; do
  n=${f%-base.json}
  [ -f $n-new.json ] || continue
  r=$(cmp -s $n-base.json $n-new.json && echo same || echo DIFF)
  d=$(cmp -s $n-base.csv $n-new.csv && echo same || echo DIFF)
  echo "$n report $r dump $d  $(grep -c . $n-base.csv) dump rows"
  if [ $r = DIFF ]; then
    /home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python - $n-base.json $n-new.json <<'P'
import json,sys
a,b=(json.load(open(p)) for p in sys.argv[1:])
for k in sorted(set(a)|set(b)):
    if a.get(k)!=b.get(k): print("   key differs:",k, (len(a.get(k)) if isinstance(a.get(k),list) else a.get(k)), (len(b.get(k)) if isinstance(b.get(k),list) else b.get(k)))
P
  fi
done
