#!/bin/bash
# The ladder (IMGFP_LADDER): IMGS-ALL and IMGS-ALL2 at 512 and 1024, then
# DISC21 zip 0 at 512 and 1024.
X=/home/daniel/Documents/DISC21; W=$X/ws; B=$W/img-fp-union
F=$X/fixtest; D=/home/daniel/Documents
echo $$ > $W/run19.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $W/run5.out; }
corp() { local tag=$1 corpus=$2 cache=$3; shift 3; say "start $tag"
    IMGFP_LADDER=1 /usr/bin/time -v $B $corpus -r -x '*' -t 8 -v --no-last --cache $cache --log-file $W/$tag.log -o $W/$tag.json "$@" 2> $W/$tag.time
    say "end $tag exit $?: $(grep Elapsed $W/$tag.time | awk '{print $NF}')"; }
disc() { local tag=$1; shift; mkdir -p $W/$tag; say "start $tag"
    IMGFP_LADDER=1 /usr/bin/time -v $B $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
        -t 8 -v --no-last --cache $W/ladder.bin --log-file $W/$tag/run.log -o $W/$tag/report.json "$@" 2> $W/$tag/time.txt
    say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"; }
corp all-512U $D/IMGS-ALL $F/all.bin --work-size 512
corp all2-512U $D/IMGS-ALL2 $F/all2.bin --work-size 512
corp all-1024U $D/IMGS-ALL $F/all.bin --work-size 1024
corp all2-1024U $D/IMGS-ALL2 $F/all2.bin --work-size 1024
disc z0-512U --work-size 512
disc z0-1024U --work-size 1024
say "run19 done"
