#!/bin/bash
# Final build G against today's (P): identity, --fast, cost ABBA at 1024, 2048.
L=/home/daniel/Documents/DISC21/lv; X=/home/daniel/Documents/DISC21; P=$X/commit/img-fp; G=$L/img-fp-G
A=/home/daniel/Documents/IMGS-ALL; A2=/home/daniel/Documents/IMGS-ALL2; I=/home/daniel/Documents/IMGS
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch9.out; }
run() { local tag=$1 bin=$2 corpus=$3; shift 3; sleep 60; say "start $tag $(sensors | grep -m1 -o '+[0-9.]*°C')"
  /usr/bin/time -v $bin $corpus -r -x '*' -t 8 -v --no-last -o $L/$tag.json --log-file $L/$tag.log "$@" 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
rm -f $X/fixtest/all.bin.matched $X/fixtest/all2.bin.matched
run h-all-def $G $A --cache $X/fixtest/all.bin
run h-all2-def $G $A2 --cache $X/fixtest/all2.bin
run h-imgs-P256 $P $I --no-cache --work-size 256
run h-imgs-fast $G $I --no-cache --fast
rm -f $L/h-a-*.bin $L/h-a2-*.bin
run h-a-P1024a $P $A --cache $L/h-a-P.bin --work-size 1024
run h-a-G1024a $G $A --cache $L/h-a-G.bin --detail 1024
rm -f $L/h-a-P.bin $L/h-a-G.bin
run h-a-G1024b $G $A --cache $L/h-a-G.bin --detail 1024
run h-a-P1024b $P $A --cache $L/h-a-P.bin --work-size 1024
rm -f $L/h-a-P.bin
run h-a2-P1024a $P $A2 --cache $L/h-a2-P.bin --work-size 1024
run h-a2-G1024a $G $A2 --cache $L/h-a2-G.bin --detail 1024
rm -f $L/h-a2-P.bin $L/h-a2-G.bin
run h-a2-G1024b $G $A2 --cache $L/h-a2-G.bin --detail 1024
run h-a2-P1024b $P $A2 --cache $L/h-a2-P.bin --work-size 1024
rm -f $L/h-a2-P.bin
run h-a-G2048 $G $A --cache $L/h-a-G2048.bin --detail 2048
run h-a2-G2048 $G $A2 --cache $L/h-a2-G2048.bin --detail 2048
say "batch9 done"
