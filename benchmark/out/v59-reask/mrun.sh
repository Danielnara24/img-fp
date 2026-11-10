#!/bin/bash
# mrun.sh NAME [ENV=V...]: MIRFAM with the scaffolding build, own cache
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask; C=$O/mir.db
n=$1; shift; rm -f $C.matched
env "$@" /usr/bin/time -f "%U user %e wall" $O/img-fp-re /home/daniel/Documents/MIRFAM -r -t 8 -v --no-last --cache $C -o $O/mir-$n.json 2> $O/mir-$n.err
rm -f $C.matched; echo "mir-$n $(tail -1 $O/mir-$n.err) $(grep 'variants:' $O/mir-$n.err | sed 's/.*variants: //')"
