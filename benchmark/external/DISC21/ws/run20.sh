#!/bin/bash
# The union in one frame (plane at 512): IMGS-ALL2 and IMGS-ALL at 512 and
# 1024, then DISC21 zip 0 at 512 and 1024.
X=/home/daniel/Documents/DISC21; W=$X/ws; B=$W/img-fp-frame
F=$X/fixtest; D=/home/daniel/Documents
echo $$ > $W/run20.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $W/run5.out; }
corp() { local tag=$1 corpus=$2 cache=$3; shift 3; say "start $tag"
    IMGFP_LADDER=1 /usr/bin/time -v $B $corpus -r -x '*' -t 8 -v --no-last --cache $cache --log-file $W/$tag.log -o $W/$tag.json "$@" 2> $W/$tag.time
    say "end $tag exit $?: $(grep Elapsed $W/$tag.time | awk '{print $NF}')"; }
disc() { local tag=$1; shift; mkdir -p $W/$tag; say "start $tag"
    IMGFP_LADDER=1 /usr/bin/time -v $B $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
        -t 8 -v --no-last --cache $W/ladder.bin --log-file $W/$tag/run.log -o $W/$tag/report.json "$@" 2> $W/$tag/time.txt
    say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"; }
corp all2-512F $D/IMGS-ALL2 $F/all2.bin --work-size 512
corp all2-1024F $D/IMGS-ALL2 $F/all2.bin --work-size 1024
corp all-512F $D/IMGS-ALL $F/all.bin --work-size 512
corp all-1024F $D/IMGS-ALL $F/all.bin --work-size 1024
disc z0-512F --work-size 512
disc z0-1024F --work-size 1024
say "run20 done"
