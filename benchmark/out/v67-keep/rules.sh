#!/bin/bash
# The two group rules on both corpora, HEAD's build and the new one's (the groups' heads read the content rule).
cd /home/daniel/Documents/Vscode_repositories/img-fp
O=benchmark/out/v67-keep
for c in ALL2 ALL; do
  for r in correlation representative; do
    $O/new/img-fp-final /home/daniel/Documents/IMGS-$c -r -x '*' -t 8 --no-last --suggest $r --cache $O/cache/c.db -o $O/new/$c-$r.json > /dev/null 2>&1
  done
  mkdir -p $O/base-correlation $O/base-representative $O/new-correlation $O/new-representative
  for r in correlation representative; do
    $O/base/img-fp /home/daniel/Documents/IMGS-$c -r -x '*' -t 8 --no-last --suggest $r --cache $O/cache/c.db -o $O/base-$r/$c.json > /dev/null 2>&1
    mv $O/new/$c-$r.json $O/new-$r/$c.json
  done
done
echo done > $O/rules.done
