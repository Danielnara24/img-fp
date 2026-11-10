#!/bin/bash
# The one step loop against the base build: reports and dumps, cached.
cd "$(dirname "$0")"
DESK=/home/daniel/Documents/IMGS/derived/Desktop
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db
D=$PWD/desk.db
corpus() { case $1 in all) echo /home/daniel/Documents/IMGS-ALL;; all2) echo /home/daniel/Documents/IMGS-ALL2;; desk) echo "$DESK";; esac; }
go() { # corpus detail build cache
  f=$1-$2-$3
  /usr/bin/time -v ./img-fp-$3 $(corpus $1) -r -x '*' -t 8 --no-last --cache $4 --detail $2 --dump $f.csv -o $f.json 2> $f.err
  echo "$(date '+%F %T') $f exit $? $(grep -E 'Maximum resident|Elapsed' $f.err | awk '{print $NF}' | tr '\n' ' ')"
}
# Desktop first: fill its own cache at 2048 with the base, then both builds.
./img-fp-base "$DESK" -r -x '*' -t 8 --no-last --cache $D --detail 2048 -o /dev/null 2>/dev/null
for d in 512 1024 2048; do for b in base new; do go desk $d $b $D; done; done
for c in all all2; do for d in 512 1024; do for b in base new; do go $c $d $b $C; done; done; done
echo ALL DONE
