#!/bin/bash
# found.sh BIN NAME [ENV=V...]: found corpus cached on the v57 cache, CPU and pairs
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v58-reach
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/c.db
bin=$1 n=$2; shift 2
rm -f $C.matched
env "$@" /usr/bin/time -f "%U user %S sys %e wall" $O/$bin /home/daniel/Downloads/archive -r -t 8 -v --no-last --cache $C -o $O/found-$n.json 2> $O/found-$n.err
rm -f $C.matched
echo "found-$n $(tail -1 $O/found-$n.err) pairs $(python3 -c "import json;print(len(json.load(open('$O/found-$n.json'))['pairs']))")"
