#!/bin/bash
L=/home/daniel/Documents/DISC21/lv; B=$L/img-fp-E; A=/home/daniel/Documents/IMGS-ALL
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch7.out; }
run() { local tag=$1 cache=$2 envs=$3; shift 3; rm -f $cache.matched; sleep 60; say "start $tag $(sensors | grep -m1 -o '+[0-9.]*°C')"
  env $envs /usr/bin/time -v $B $A -r -x '*' -t 8 -v --no-last --cache $cache -o $L/$tag.json --log-file $L/$tag.log "$@" 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
run e-all $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_ALL=1" --detail 1024
run e-fresh $L/a-x025.bin "IMGFP_FINER_SHARE=0.25" --detail 1024
run e-fresh-n $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_NOSECOND=1" --detail 1024
run e-fresh-v $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_VETO=1" --detail 1024
run e-all2 $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_ALL=1" --detail 1024
run e-fresh2 $L/a-x025.bin "IMGFP_FINER_SHARE=0.25" --detail 1024
say "batch7 done"
