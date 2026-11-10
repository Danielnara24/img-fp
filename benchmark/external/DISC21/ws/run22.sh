#!/bin/bash
# The ladder (union, one frame, rung tags) at 64, 256, 2048 on IMGS-ALL2 and
# IMGS-ALL, then DISC21 zip 0 at 64, 256, 512, 1024, 2048.
X=/home/daniel/Documents/DISC21; W=$X/ws; B=$W/img-fp-tag
F=$X/fixtest; D=/home/daniel/Documents
echo $$ > $W/run22.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $W/run5.out; }
corp() { local tag=$1 corpus=$2 cache=$3; shift 3; say "start $tag"
    IMGFP_LADDER=1 /usr/bin/time -v $B $corpus -r -x '*' -t 8 -v --no-last --cache $cache --log-file $W/$tag.log -o $W/$tag.json "$@" 2> $W/$tag.time
    say "end $tag exit $?: $(grep Elapsed $W/$tag.time | awk '{print $NF}')"; }
disc() { local tag=$1; shift; mkdir -p $W/$tag; say "start $tag"
    IMGFP_LADDER=1 /usr/bin/time -v $B $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
        -t 8 -v --no-last --cache $W/ladder.bin --log-file $W/$tag/run.log -o $W/$tag/report.json "$@" 2> $W/$tag/time.txt
    say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"; }
for w in 64 256 2048; do
    corp all2-${w}T $D/IMGS-ALL2 $F/all2.bin --work-size $w
    corp all-${w}T $D/IMGS-ALL $F/all.bin --work-size $w
done
for w in 64 256 512 1024 2048; do disc z0-${w}T --work-size $w; done
say "run22 done"
