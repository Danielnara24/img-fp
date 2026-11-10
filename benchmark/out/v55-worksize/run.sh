#!/bin/bash
# Work size 512 vs 1024 on IMGS-ALL and IMGS-ALL2, build after 0.76.0 (copied
# here), defaults otherwise. 512 from the fixtest caches (accuracy only); 1024
# into the same caches (its own records). Cost read off 1024's -v pace lines.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v55-worksize
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo $$ > $O/run.pid; echo 1000 > /proc/self/oom_score_adj
one() { # tag corpus cache extra...
    local tag=$1 corpus=$2 cache=$3; shift 3
    echo "$(date +%T) start $tag" >> $O/run.out
    /usr/bin/time -v $O/img-fp "$corpus" -r -x '*' -t 8 -v --no-last --cache "$cache" \
        --log-file $O/$tag.log -o $O/$tag.json "$@" 2> $O/$tag.time
    echo "$(date +%T) end $tag exit $?" >> $O/run.out
}
one all-512 $D/IMGS-ALL $F/all.bin
one all2-512 $D/IMGS-ALL2 $F/all2.bin
one all-1024 $D/IMGS-ALL $F/all.bin --work-size 1024
one all2-1024 $D/IMGS-ALL2 $F/all2.bin --work-size 1024
echo "$(date +%T) all done" >> $O/run.out
