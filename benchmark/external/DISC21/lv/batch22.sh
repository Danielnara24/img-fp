#!/bin/bash
# --detail 2048 against 4096 (one build, W), cold, ABBA; then DISC21's proxy at 2048 and 4096.
L=/home/daniel/Documents/DISC21/lv; X=/home/daniel/Documents/DISC21; G=$L/img-fp-W
A=/home/daniel/Documents/IMGS-ALL; A2=/home/daniel/Documents/IMGS-ALL2; PX=$X/proxy
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch22.out; }
temp() { sensors | grep -m1 Tctl | grep -o '[0-9]*\.[0-9]' | head -1; }
sleep 240; IDLE=$(temp); say "idle $IDLE"
cool() { sleep 60; local n=0; while (( $(echo "$(temp) > $IDLE + 3" | bc) )) && (( n < 60 )); do sleep 10; n=$((n+1)); done; }
run() { local tag=$1 corpus=$2 cache=$3; shift 3; rm -f $cache $cache.*; cool; say "start $tag $(temp)"
  /usr/bin/time -v $G $corpus -r -x '*' -t 8 -v --no-last --cache $cache -o $L/$tag.json --log-file $L/$tag.log "$@" 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}') maxrss $(grep Maximum $L/$tag.time | awk '{print $NF}')"; }
px() { local tag=$1; shift; cool; say "start $tag $(temp)"
  /usr/bin/time -v $G $PX/q --against $PX/r --follow-symlinks --min-pixel-correlation 0.5 -t 8 -v --no-last --no-cache -o $L/$tag.json --log-file $L/$tag.log "$@" 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}') maxrss $(grep Maximum $L/$tag.time | awk '{print $NF}')"; }
run x-a2-2k1 $A2 $L/x-a2-2k.bin --detail 2048
run x-a2-4k1 $A2 $L/x-a2-4k.bin --detail 4096
run x-a2-4k2 $A2 $L/x-a2-4k.bin --detail 4096
run x-a2-2k2 $A2 $L/x-a2-2k.bin --detail 2048
run x-a-2k1 $A $L/x-a-2k.bin --detail 2048
run x-a-4k1 $A $L/x-a-4k.bin --detail 4096
run x-a-4k2 $A $L/x-a-4k.bin --detail 4096
run x-a-2k2 $A $L/x-a-2k.bin --detail 2048
px x-px-2k1 --detail 2048
px x-px-4k1 --detail 4096
px x-px-4k2 --detail 4096
px x-px-2k2 --detail 2048
say "batch22 done"
