#!/bin/bash
# After run5: zip 0 at 1024 from the cache with the vocabulary sample cap
# raised to 2.09 M (the most 21-bit words allow).
X=/home/daniel/Documents/DISC21; W=$X/ws
echo $$ > $W/run6.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $W/run5.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $W/run5.out; }
tag=z0-1024-cap209; mkdir -p $W/$tag; say "start $tag"
IMGFP_SAMPLE_CAP=2090000 /usr/bin/time -v $W/img-fp-cap $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache $X/cache/w1024.bin --work-size 1024 --log-file $W/$tag/run.log -o $W/$tag/report.json 2> $W/$tag/time.txt
say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"
