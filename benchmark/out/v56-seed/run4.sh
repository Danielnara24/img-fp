#!/bin/bash
# Timing screen, cached IMGS-ALL2, nothing else running: words against hashed sizes (bucket matcher).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo 1000 > /proc/self/oom_score_adj
temp() { awk '{print $1/1000}' /sys/class/hwmon/hwmon3/temp1_input; }
idle=61.75; echo "$(date +%T) idle $idle" >> $O/run4.out
one() { local tag=$1 seed=$2
    sleep 60; while [ "$(awk -v t=$(temp) -v i=$idle 'BEGIN{print (t>i+3)}')" = 1 ]; do sleep 10; done
    rm -f $F/all2.bin.matched
    if [ $seed = off ]; then unset IMGFP_SEED; else export IMGFP_SEED=$seed; fi
    /usr/bin/time -v $O/img-fp-seedB $D/IMGS-ALL2 -r -x '*' -t 8 -v --no-last --cache $F/all2.bin --log-file $O/$tag.log -o $O/$tag.json 2> $O/$tag.time
    echo "$(date +%T) end $tag $(grep -E 'User time|System time|Elapsed' $O/$tag.time | awk '{print $NF}' | tr '\n' ' ') verify $(grep 'pace: verifying' $O/$tag.log | grep -o 'in [0-9.]*s')" >> $O/run4.out
    rm -f $F/all2.bin.matched; unset IMGFP_SEED
}
one u-11,4 11,4; one u-off off; one u-10,3 10,3; one u-12,4 12,4; one u-off2 off; one u-8,3 8,3
echo "$(date +%T) done" >> $O/run4.out
