#!/bin/bash
# crun.sh CORPUS NAME [ENV=V...]: all | all2 | found, cached on the v57 scratch cache
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/c.db
c=$1 n=$2; shift 2
case $c in all) p=(/home/daniel/Documents/IMGS-ALL -x '*');; all2) p=(/home/daniel/Documents/IMGS-ALL2 -x '*');; found) p=(/home/daniel/Downloads/archive);; esac
rm -f $C.matched
env "$@" /usr/bin/time -f "%U user %e wall" $O/img-fp-re "${p[@]}" -r -t 8 -v --no-last --cache $C -o $O/$c-$n.json 2> $O/$c-$n.err
rm -f $C.matched; echo "$c-$n $(tail -1 $O/$c-$n.err) $(grep 'variants:' $O/$c-$n.err | sed 's/.*variants: //')"
