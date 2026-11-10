#!/bin/bash
# After batch1: a clean ABBA pair at 512 on IMGS-ALL2 (batch1's first plain run
# overlapped a build).
L=/home/daniel/Documents/DISC21/lv; D=/home/daniel/Documents/IMGS-ALL2; B=$L/img-fp-g
echo $$ > $L/batch2.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $L/batch1.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $L/batch1.out; }
run() { local tag=$1 ws=$2; shift 2; sleep 60; say "start $tag $(sensors | grep -m1 -o '+[0-9.]*°C')"
  env "$@" /usr/bin/time -v $B $D -r -x '*' -t 8 -v --no-last --no-cache --work-size $ws -o $L/$tag.json --log-file $L/$tag.log 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
LV="IMGFP_LEVELS=1 IMGFP_LOWEST=256 IMGFP_KNOWN=anchor IMGFP_GRIDMAX=1024"
run a2-p512c 512 X=1
run a2-l512c 512 $LV
say "batch2 done"
