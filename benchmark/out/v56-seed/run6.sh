#!/bin/bash
# Fewer tables (14 x 2, 14 x 3) against words: cost on IMGS-ALL2 (alternating), accuracy on both.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo 1000 > /proc/self/oom_score_adj
temp() { awk '{print $1/1000}' /sys/class/hwmon/hwmon3/temp1_input; }
idle=61.75
one() { local tag=$1 seed=$2 corpus=$3 cache=$4
    sleep 60; while [ "$(awk -v t=$(temp) -v i=$idle 'BEGIN{print (t>i+3)}')" = 1 ]; do sleep 10; done
    rm -f $cache.matched
    if [ $seed = off ]; then unset IMGFP_SEED; else export IMGFP_SEED=$seed; fi
    /usr/bin/time -v $O/img-fp-seedE $corpus -r -x '*' -t 8 -v --no-last --cache $cache --log-file $O/$tag.log -o $O/$tag.json 2> $O/$tag.time
    echo "$(date +%T) end $tag $(grep -E 'User time|System time|Elapsed' $O/$tag.time | awk '{print $NF}' | tr '\n' ' ') verify $(grep 'pace: verifying' $O/$tag.log | grep -o 'in [0-9.]*s')" >> $O/run6.out
    rm -f $cache.matched; unset IMGFP_SEED
}
one w-14,3 14,3 $D/IMGS-ALL2 $F/all2.bin
one w-off off $D/IMGS-ALL2 $F/all2.bin
one w-14,2 14,2 $D/IMGS-ALL2 $F/all2.bin
one w-14,5 14,5 $D/IMGS-ALL2 $F/all2.bin
one wa-14,3 14,3 $D/IMGS-ALL $F/all.bin
one wa-14,2 14,2 $D/IMGS-ALL $F/all.bin
echo "$(date +%T) done" >> $O/run6.out
