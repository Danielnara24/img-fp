#!/bin/bash
# G against today's P, cold, ABBA, every file beside the cache removed, idle+3 C wait.
L=/home/daniel/Documents/DISC21/lv; X=/home/daniel/Documents/DISC21; P=$X/commit/img-fp; G=$L/img-fp-J
A=/home/daniel/Documents/IMGS-ALL; A2=/home/daniel/Documents/IMGS-ALL2
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch13.out; }
temp() { sensors | grep -m1 Tctl | grep -o '[0-9]*\.[0-9]' | head -1; }
sleep 240; IDLE=$(temp); say "idle $IDLE"
cool() { sleep 60; local n=0; while (( $(echo "$(temp) > $IDLE + 3" | bc) )) && (( n < 60 )); do sleep 10; n=$((n+1)); done; }
run() { local tag=$1 bin=$2 corpus=$3 cache=$4; shift 4; rm -f $cache $cache.*; cool; say "start $tag $(temp)"
  /usr/bin/time -v $bin $corpus -r -x '*' -t 8 -v --no-last --cache $cache -o $L/$tag.json --log-file $L/$tag.log "$@" 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
run m-a-P1 $P $A $L/m-a-P.bin --work-size 1024
run m-a-G1 $G $A $L/m-a-G.bin --detail 1024
run m-a-G2 $G $A $L/m-a-G.bin --detail 1024
run m-a-P2 $P $A $L/m-a-P.bin --work-size 1024
run m-a2-P1 $P $A2 $L/m-a2-P.bin --work-size 1024
run m-a2-G1 $G $A2 $L/m-a2-G.bin --detail 1024
run m-a2-G2 $G $A2 $L/m-a2-G.bin --detail 1024
run m-a2-P2 $P $A2 $L/m-a2-P.bin --work-size 1024
say "batch13 done"
