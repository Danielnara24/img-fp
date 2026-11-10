#!/bin/bash
# Option C: today's build (P, --work-size) against --detail (N), cold.
L=/home/daniel/Documents/DISC21/lv; X=/home/daniel/Documents/DISC21; P=$X/commit/img-fp; N=$L/img-fp-C
echo $$ > $L/batch5.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch5.out; }
run() { local tag=$1 bin=$2 corpus=$3; shift 3; sleep 60; say "start $tag $(sensors | grep -m1 -o '+[0-9.]*°C')"
  /usr/bin/time -v $bin $corpus -r -x '*' -t 8 -v --no-last --no-cache -o $L/$tag.json --log-file $L/$tag.log "$@" 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
A2=/home/daniel/Documents/IMGS-ALL2; A=/home/daniel/Documents/IMGS-ALL
run c-a2-P512 $P $A2
run c-a2-P1024 $P $A2 --work-size 1024
run c-a2-N1024 $N $A2 --detail 1024
run c-a2-P2048 $P $A2 --work-size 2048
run c-a2-N2048 $N $A2 --detail 2048
run c-a-P512 $P $A
run c-a-P1024 $P $A --work-size 1024
run c-a-N1024 $N $A --detail 1024
run c-a-P2048 $P $A --work-size 2048
run c-a-N2048 $N $A --detail 2048
mkdir -p $L/z0-N1024; sleep 60; say "start z0-N1024"
/usr/bin/time -v $N $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $L/disc-N.bin --detail 1024 --log-file $L/z0-N1024/run.log -o $L/z0-N1024/report.json 2> $L/z0-N1024/time.txt
say "end z0-N1024 exit $? $(grep -E 'User time' $L/z0-N1024/time.txt | awk '{print $NF}')"
say "batch5 done"
