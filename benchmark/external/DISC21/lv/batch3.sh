#!/bin/bash
# DISC21 zip 0, levels at 512 / 256 / 1024 (accuracy; one cache per size).
L=/home/daniel/Documents/DISC21/lv; X=/home/daniel/Documents/DISC21; B=$L/img-fp-g
echo $$ > $L/batch3.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $L/batch2.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $L/batch1.out; }
disc() { local tag=$1 ws=$2; mkdir -p $L/$tag; sleep 60; say "start $tag"
  IMGFP_LEVELS=1 IMGFP_LOWEST=256 IMGFP_KNOWN=anchor IMGFP_GRIDMAX=1024 /usr/bin/time -v $B $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
     -t 8 -v --no-last --cache $L/disc-$ws.bin --work-size $ws --log-file $L/$tag/run.log -o $L/$tag/report.json 2> $L/$tag/time.txt
  say "end $tag exit $? $(grep -E 'User time' $L/$tag/time.txt | awk '{print $NF}') $(grep Elapsed $L/$tag/time.txt | awk '{print $NF}')"; }
disc z0-L512 512
disc z0-L256 256
disc z0-L1024 1024
say "batch3 done"
