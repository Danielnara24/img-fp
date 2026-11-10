#!/bin/bash
# After run8: the split run again from its cache, with --dump, to see whether
# the true copies it loses were verified at all.
X=/home/daniel/Documents/DISC21; W=$X/ws
echo $$ > $W/run9.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $W/run8.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $W/run5.out; }
tag=z0-1024-split-dump; mkdir -p $W/$tag; say "start $tag"
IMGFP_SPLIT=1 /usr/bin/time -v $W/img-fp-split $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache $W/ws.bin --work-size 1024 --log-file $W/$tag/run.log -o $W/$tag/report.json --dump $W/$tag/dump.csv 2> $W/$tag/time.txt
say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"
