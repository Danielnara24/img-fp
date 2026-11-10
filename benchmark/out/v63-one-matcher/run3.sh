#!/bin/bash
# IMGS-ALL2 (cached at 512 and 1024) and Desktop: base against the first-wins build.
cd "$(dirname "$0")"
DESK=/home/daniel/Documents/IMGS/derived/Desktop
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db
D=$PWD/desk.db
corpus() { case $1 in all2) echo /home/daniel/Documents/IMGS-ALL2;; desk) echo "$DESK";; esac; }
go() { f=$1-$2-$3
  /usr/bin/time -v ./img-fp-$3 $(corpus $1) -r -x '*' -t 8 --no-last --cache $4 --detail $2 --dump $f.csv -o $f.json 2> $f.err
  echo "$(date '+%F %T') $f exit $? $(grep -E 'Maximum resident|Elapsed' $f.err | awk '{print $NF}' | tr '\n' ' ')"; }
for d in 512 1024 2048; do go desk $d new2 $D; done
for d in 1024 512; do for b in base new2; do go all2 $d $b $C; done; done
echo ALL DONE
