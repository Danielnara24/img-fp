#!/bin/bash
L=/home/daniel/Documents/DISC21/lv; B=$L/img-fp-F; A=/home/daniel/Documents/IMGS-ALL
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $L/batch8.out; }
run() { local tag=$1 cache=$2 envs=$3; shift 3; rm -f $cache.matched; sleep 60; say "start $tag $(sensors | grep -m1 -o '+[0-9.]*°C')"
  env $envs /usr/bin/time -v $B $A -r -x '*' -t 8 -v --no-last --cache $cache -o $L/$tag.json --log-file $L/$tag.log "$@" 2> $L/$tag.time
  say "end $tag exit $? $(grep -E 'User time' $L/$tag.time | awk '{print $NF}') $(grep Elapsed $L/$tag.time | awk '{print $NF}')"; }
run f-k75 $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_VETO=1 IMGFP_FINER_K=75" --detail 1024
run f-k40 $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_VETO=1 IMGFP_FINER_K=40" --detail 1024
run f-k150 $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_VETO=1" --detail 1024
run f-k20 $L/a-x025.bin "IMGFP_FINER_SHARE=0.25 IMGFP_FINER_VETO=1 IMGFP_FINER_K=20" --detail 1024
say "batch8 done"
