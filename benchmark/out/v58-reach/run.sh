#!/bin/bash
# run.sh BIN CORPUS(all|all2) NAME [ENV=V ...]: cached run on the v57 cache, scored
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v58-reach
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/c.db
bin=$1 corp=$2 n=$3; shift 3
D=/home/daniel/Documents/IMGS-ALL; [ $corp = all2 ] && D=/home/daniel/Documents/IMGS-ALL2
rm -f $C.matched
env "$@" /usr/bin/time -f "%U user %S sys %e wall" $O/$bin $D -r -x "*" -t 8 -v --no-last --cache $C -o $O/$corp-$n.json 2> $O/$corp-$n.err
rm -f $C.matched
echo "$corp-$n $(tail -1 $O/$corp-$n.err)"
(cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark && ../vendor/venv/bin/python analyse.py $O/$corp-$n.json --corpus=$D --per-transform > $O/sc-$corp-$n.txt 2>&1)
sed -n 3,6p $O/sc-$corp-$n.txt
