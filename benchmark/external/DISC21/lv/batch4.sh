#!/bin/bash
L=/home/daniel/Documents/DISC21/lv; D=/home/daniel/Documents/IMGS-ALL2; B=$L/img-fp-o
echo $$ > $L/batch4.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch4.out; }
run() { local tag=$1 ws=$2; shift 2; rm -f $L/a2o.bin.matched; sleep 30; say "start $tag"
  env "$@" /usr/bin/time -v $B $D -r -x '*' -t 8 -v --no-last --cache $L/a2o.bin --work-size $ws -o $L/$tag.json --log-file $L/$tag.log 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
LV="IMGFP_LEVELS=1 IMGFP_LOWEST=256 IMGFP_GRIDMAX=1024 IMGFP_CAPX=1.4 IMGFP_KNOWN=anchor"
run o-l512 512 $LV
run o-l512v 512 $LV IMGFP_UNION=veto
run o-l256 256 $LV
run o-l1024 1024 $LV
run o-l1024v 1024 $LV IMGFP_UNION=veto
say "batch4 done"
