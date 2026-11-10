#!/bin/bash
# IMGS-ALL2 cold: plain vs levels (anchors to 1024, lowest step 256, known=anchor).
L=/home/daniel/Documents/DISC21/lv; D=/home/daniel/Documents/IMGS-ALL2; B=$L/img-fp-g
echo $$ > $L/batch1.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch1.out; }
cool() { sleep 60; }
run() { local tag=$1 ws=$2; shift 2; cool; say "start $tag $(sensors | grep -m1 -o '+[0-9.]*°C')"
  env "$@" /usr/bin/time -v $B $D -r -x '*' -t 8 -v --no-last --no-cache --work-size $ws -o $L/$tag.json --log-file $L/$tag.log 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
LV="IMGFP_LEVELS=1 IMGFP_LOWEST=256 IMGFP_KNOWN=anchor IMGFP_GRIDMAX=1024"
run a2-p512a 512 X=1
run a2-l512a 512 $LV
run a2-l512b 512 $LV
run a2-p512b 512 X=1
run a2-l256 256 $LV
run a2-p256 256 X=1
run a2-l1024 1024 $LV
run a2-p1024 1024 X=1
say "batch1 done"
