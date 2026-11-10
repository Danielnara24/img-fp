#!/bin/bash
# abba.sh TAG ORDER "ENV_A" "ENV_B" BIN_A BIN_B ARGS... : cooled alternating runs on the v57 cache.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v58-reach
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/c.db
tag=$1 order=$2 ea=$3 eb=$4 ba=$5 bb=$6; shift 6
temp() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat $h/name)" = k10temp ] && echo $(( $(cat $h/temp1_input) / 1000 )); done; }
sleep 60; idle=$(temp); echo "idle $idle C"
i=0
for c in $(echo $order | fold -w1); do
  i=$((i+1)); sleep 60; w=0; while [ $(temp) -gt $((idle+3)) ] && [ $w -lt 600 ]; do sleep 10; w=$((w+10)); done
  if [ $c = A ]; then e=$ea; b=$ba; else e=$eb; b=$bb; fi
  rm -f $C.matched; t0=$(temp)
  env $e /usr/bin/time -f "%U %S %e" $O/$b "$@" --cache $C --no-last -o $O/abba-$tag-$c$i.json 2> $O/abba-$tag-$c$i.err
  echo "$c$i start ${t0}C end $(temp)C user/sys/wall $(tail -1 $O/abba-$tag-$c$i.err)"
done
rm -f $C.matched
