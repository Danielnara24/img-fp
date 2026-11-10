#!/bin/bash
# logo.sh A B : pair A,B (in $S) among n random found photos, 5 draws per n
S=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/fca936e0-45fa-490b-9fb4-7ef700283980/scratchpad
B=/home/daniel/Documents/Vscode_repositories/img-fp/target/release/img-fp
for n in 15 30 120; do miss=0; for seed in 1 2 3 4 5; do d=$S/r; rm -rf $d; mkdir -p $d; cp $S/$1 $S/$2 $d/; shuf -n $n --random-source=<(yes $seed$n) $S/found.txt | while read f; do ln -sf "$f" $d/$(echo $f | md5sum | cut -c1-8).jpg; done; $B --no-cache --no-last --follow-symlinks $d -o $d.json >/dev/null 2>&1; python3 -c "
import json,sys
r=json.load(open('$d.json'))
ok=any(sorted([p['a'].rsplit('/',1)[1],p['b'].rsplit('/',1)[1]])==sorted(['$1','$2']) for p in r['pairs'])
sys.exit(0 if ok else 1)" || miss=$((miss+1)); done; printf "n=%s missed %s/5  " $n $miss; done; echo
