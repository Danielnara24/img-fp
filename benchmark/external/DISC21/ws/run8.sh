#!/bin/bash
# After run7: the cap test again (killed by OOM beside a scoring script).
X=/home/daniel/Documents/DISC21; W=$X/ws
echo $$ > $W/run8.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $W/run7.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $W/run5.out; }
tag=z0-1024-cap209; rm -rf $W/$tag; mkdir -p $W/$tag; say "start $tag (again)"
IMGFP_SAMPLE_CAP=2090000 /usr/bin/time -v $W/img-fp-cap $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache $X/cache/w1024.bin --work-size 1024 --log-file $W/$tag/run.log -o $W/$tag/report.json 2> $W/$tag/time.txt
say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"
