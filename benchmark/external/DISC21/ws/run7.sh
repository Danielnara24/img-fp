#!/bin/bash
# After run6: zip 0 at 1024 with the split extraction (512's features plus
# finer ones), analysed afresh into its own cache.
X=/home/daniel/Documents/DISC21; W=$X/ws
echo $$ > $W/run7.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $W/run6.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $W/run5.out; }
tag=z0-1024-split; mkdir -p $W/$tag; say "start $tag"
IMGFP_SPLIT=1 /usr/bin/time -v $W/img-fp-split $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache $W/ws.bin --work-size 1024 --log-file $W/$tag/run.log -o $W/$tag/report.json 2> $W/$tag/time.txt
say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"
