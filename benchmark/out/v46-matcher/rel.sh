#!/bin/bash
# 0.73.0 (scaffolding removed, the stack hard-wired) against the measured stack runs (p-on3, env on).
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
for t in all i5; do
  rm -f cache.bin.matched
  ./img-fp-0.73.0 -v --from-file g320/$t-full.list -x '*' --cache cache.bin --no-last -o rel/$t-full.json 2> rel/$t-full.log
  python3 -c "
import json,sys
a=json.load(open('rel/$t-full.json')); b=json.load(open('p-on3/$t-full.json'))
a.pop('runtime_seconds',None); b.pop('runtime_seconds',None)
print('$t', 'IDENTICAL' if a==b else 'DIFFERENT', len(a.get('pairs',[])), len(b.get('pairs',[])))"
done
echo rel done
