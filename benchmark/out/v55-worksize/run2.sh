#!/bin/bash
# Tied scale bound (img-fp-tie), features 600 / 1200 (IMGFP_FEATURES), at 1024;
# and 512 to check nothing moves there.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v55-worksize
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo $$ > $O/run2.pid; echo 1000 > /proc/self/oom_score_adj
one() { # tag features corpus cache extra...
    local tag=$1 feat=$2 corpus=$3 cache=$4; shift 4
    echo "$(date +%T) start $tag" >> $O/run2.out
    IMGFP_FEATURES=$feat /usr/bin/time -v $O/img-fp-tie "$corpus" -r -x '*' -t 8 -v --no-last --cache "$cache" \
        --log-file $O/$tag.log -o $O/$tag.json "$@" 2> $O/$tag.time
    echo "$(date +%T) end $tag exit $?" >> $O/run2.out
}
one all2-512-tie 600 $D/IMGS-ALL2 $F/all2.bin
one all2-1024-tie 600 $D/IMGS-ALL2 $F/all2.bin --work-size 1024
one all-1024-tie 600 $D/IMGS-ALL $F/all.bin --work-size 1024
one all2-1024-f1200 1200 $D/IMGS-ALL2 $F/all2.bin --work-size 1024
one all-1024-f1200 1200 $D/IMGS-ALL $F/all.bin --work-size 1024
echo "$(date +%T) all done" >> $O/run2.out
