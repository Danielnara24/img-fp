#!/bin/bash
# Baselines for the work-size fix, current tree (both ties + dump fix):
# DISC21 zip 0 at 512 / 256 / 2048, then the -k 300 diagnostic at 1024,
# then IMGS-ALL and IMGS-ALL2 at 256.
X=/home/daniel/Documents/DISC21; W=$X/ws; B=$W/img-fp
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo $$ > $W/run5.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $W/run5.out; }
disc() { local tag=$1 cache=$2; shift 2; mkdir -p $W/$tag; say "start $tag"
    /usr/bin/time -v $B $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
        -t 8 -v --no-last --cache $cache --log-file $W/$tag/run.log -o $W/$tag/report.json "$@" 2> $W/$tag/time.txt
    say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"; }
corp() { local tag=$1 corpus=$2 cache=$3; shift 3; say "start $tag"
    /usr/bin/time -v $B $corpus -r -x '*' -t 8 -v --no-last --cache $cache --log-file $W/$tag.log -o $W/$tag.json "$@" 2> $W/$tag.time
    say "end $tag exit $?"; }
disc z0-512 $X/cache/cache.bin --work-size 512
disc z0-256 $W/ws.bin --work-size 256
disc z0-2048 $W/ws.bin --work-size 2048
disc z0-1024-k300 $X/cache/w1024.bin --work-size 1024 -k 300
corp all-256 $D/IMGS-ALL $F/all.bin --work-size 256
corp all2-256 $D/IMGS-ALL2 $F/all2.bin --work-size 256
say "run5 done"
