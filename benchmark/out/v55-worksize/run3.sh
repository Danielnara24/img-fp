#!/bin/bash
# The 1200-feature runs again, matched afresh (--dump takes no kept matching).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v55-worksize
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo $$ > $O/run3.pid; echo 1000 > /proc/self/oom_score_adj
tail --pid=$(cat $O/run2.pid) -f /dev/null
one() { local tag=$1 feat=$2 corpus=$3 cache=$4; shift 4
    echo "$(date +%T) start $tag" >> $O/run2.out
    IMGFP_FEATURES=$feat /usr/bin/time -v $O/img-fp-tie "$corpus" -r -x '*' -t 8 -v --no-last --cache "$cache" \
        --log-file $O/$tag.log -o $O/$tag.json --dump /dev/null "$@" 2> $O/$tag.time
    echo "$(date +%T) end $tag exit $?" >> $O/run2.out; }
one all2-1024-f1200 1200 $D/IMGS-ALL2 $F/all2.bin --work-size 1024
one all-1024-f1200 1200 $D/IMGS-ALL $F/all.bin --work-size 1024
echo "$(date +%T) run3 done" >> $O/run2.out
