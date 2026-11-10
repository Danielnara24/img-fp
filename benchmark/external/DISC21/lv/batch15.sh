#!/bin/bash
# Cached matching cost: today's 1024 (P) against --detail 1024 at finer window / second look settings.
L=/home/daniel/Documents/DISC21/lv; X=/home/daniel/Documents/DISC21; P=$X/commit/img-fp; S=$L/img-fp-S
A=/home/daniel/Documents/IMGS-ALL
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch15.out; }
temp() { sensors | grep -m1 Tctl | grep -o '[0-9]*\.[0-9]' | head -1; }
sleep 180; IDLE=$(temp); say "idle $IDLE"
cool() { sleep 60; local n=0; while (( $(echo "$(temp) > $IDLE + 3" | bc) )) && (( n < 60 )); do sleep 10; n=$((n+1)); done; }
run() { local tag=$1 bin=$2 cache=$3; shift 3; rm -f $cache.matched; cool; say "start $tag $(temp)"
  /usr/bin/time -v env "$@" -r -x '*' -t 8 -v --no-last --cache $cache -o $L/$tag.json --log-file $L/$tag.log 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
CP=$L/n-a-P.bin; CG=$L/n-a-G.bin
for r in 1 2; do
  if [ $r = 1 ]; then order="P a b c d"; else order="d c b a P"; fi
  for o in $order; do case $o in
    P) run c-P$r $P $CP $P $A --work-size 1024;;
    a) run c-a$r $S $CG IMGFP_FK=75 $S $A --detail 1024;;
    b) run c-b$r $S $CG IMGFP_FK=40 $S $A --detail 1024;;
    c) run c-c$r $S $CG IMGFP_FK=75 IMGFP_FLOOK=0 $S $A --detail 1024;;
    d) run c-d$r $S $CG IMGFP_FK=40 IMGFP_FLOOK=0 $S $A --detail 1024;;
  esac; done
done
say "batch15 done"
