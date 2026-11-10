#!/bin/bash
# After run9: zip 0 at 512 with --dump (cached), the reference verdicts.
X=/home/daniel/Documents/DISC21; W=$X/ws
echo $$ > $W/run10.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $W/run9.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $W/run5.out; }
tag=z0-512-dump; mkdir -p $W/$tag; say "start $tag"
/usr/bin/time -v $W/img-fp $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache $X/cache/cache.bin --work-size 512 --log-file $W/$tag/run.log -o $W/$tag/report.json --dump $W/$tag/dump.csv 2> $W/$tag/time.txt
say "end $tag exit $?: $(grep Elapsed $W/$tag/time.txt | awk '{print $NF}')"
