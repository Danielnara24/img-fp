#!/bin/bash
# zip 0 at 1024: split extraction + retrieval on the 512 words beside the full ranking.
X=/home/daniel/Documents/DISC21; W=$X/ws
echo $$ > $W/run11.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $W/run5.out; }
tag=z0-1024-retr; mkdir -p $W/$tag; say "start $tag"
IMGFP_SPLIT=1 IMGFP_RETR512=1 /usr/bin/time -v $W/img-fp-retr $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache $W/ws.bin --work-size 1024 --log-file $W/$tag/run.log -o $W/$tag/report.json --dump $W/$tag/dump.csv 2> $W/$tag/time.txt
say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"
