#!/bin/bash
# abba.sh TAG ORDER ARGS... : runs base (A) and new (B) in ORDER (e.g. ABBAAB), cooled.
S=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57
cd $S
tag=$1; order=$2; shift 2
temp() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat $h/name)" = k10temp ] && echo $(( $(cat $h/temp1_input) / 1000 )); done; }
sleep 60; idle=$(temp); echo "idle $idle C"
i=0
for c in $(echo $order | fold -w1); do
  i=$((i+1)); sleep 60; w=0; while [ $(temp) -gt $((idle+3)) ] && [ $w -lt 600 ]; do sleep 10; w=$((w+10)); done
  bin=./img-fp-a; [ $c = B ] && bin=./img-fp-new
  rm -f c.db.matched
  t0=$(temp)
  /usr/bin/time -f "%U %S %e" $bin "$@" --cache c.db --no-last -o $tag-$c$i.json 2> $tag-$c$i.err
  echo "$c$i start ${t0}C end $(temp)C user/sys/wall $(tail -1 $tag-$c$i.err)"
done
rm -f c.db.matched
