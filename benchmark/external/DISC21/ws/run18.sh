#!/bin/bash
# After run17: IMGS-ALL2 at 512 and 1024 with the ladder again, cached, with --dump.
X=/home/daniel/Documents/DISC21; W=$X/ws; B=$W/img-fp-ladder; F=$X/fixtest; D=/home/daniel/Documents
echo $$ > $W/run18.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $W/run17.pid) -f /dev/null
say() { echo "$(date +%T) $*" >> $W/run5.out; }
for w in 512 1024; do tag=all2-${w}L-dump; say "start $tag"
    IMGFP_LADDER=1 $B $D/IMGS-ALL2 -r -x '*' -t 8 --no-last --cache $F/all2.bin --work-size $w -o $W/$tag.json --dump $W/$tag.csv > /dev/null 2>&1
    say "end $tag exit $?"; done
