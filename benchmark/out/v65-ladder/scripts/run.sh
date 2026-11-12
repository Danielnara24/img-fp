#!/bin/bash
# run NAME BINARY CACHE [ENV=..]... -- [args]   (IMGS-ALL2, -v, report to out/NAME.json)
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65; echo 1000 > /proc/self/oom_score_adj
n=$1; b=$2; c=$3; shift 3; e=(); while [ "$1" != "--" ]; do e+=("$1"); shift; done; shift
rm -f $c.matched
env "${e[@]}" X=1 /usr/bin/time -v ./$b ${CORPUS:-/home/daniel/Documents/IMGS-ALL2} -r -x '*' -t 8 -v --no-last --cache $c "$@" -o out/$n.json 2> out/$n.err
rm -f $c.matched
echo "$(date '+%T') $n $(grep -E 'User time|Elapsed|Maximum res' out/$n.err | awk '{print $NF}' | tr '\n' ' ')" >> run.log
