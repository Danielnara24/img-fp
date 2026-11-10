#!/bin/bash
# After run20: the union in one frame with rung tags, IMGS-ALL2 and IMGS-ALL
# at 512 and 1024.
X=/home/daniel/Documents/DISC21; W=$X/ws; B=$W/img-fp-tag
F=$X/fixtest; D=/home/daniel/Documents
echo $$ > $W/run21.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $W/run20.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $W/run5.out; }
corp() { local tag=$1 corpus=$2 cache=$3; shift 3; say "start $tag"
    IMGFP_LADDER=1 /usr/bin/time -v $B $corpus -r -x '*' -t 8 -v --no-last --cache $cache --log-file $W/$tag.log -o $W/$tag.json "$@" 2> $W/$tag.time
    say "end $tag exit $?: $(grep Elapsed $W/$tag.time | awk '{print $NF}')"; }
corp all2-512T $D/IMGS-ALL2 $F/all2.bin --work-size 512
corp all2-1024T $D/IMGS-ALL2 $F/all2.bin --work-size 1024
corp all-512T $D/IMGS-ALL $F/all.bin --work-size 512
corp all-1024T $D/IMGS-ALL $F/all.bin --work-size 1024
say "run21 done"
