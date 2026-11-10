#!/bin/bash
# IMGS-ALL and IMGS-ALL2 at --work-size 2048 (img-fp-dumpfix: 2,048 features,
# bound 64), then DISC21 zip 0 and zip 1 at 1024 with both ties, one after
# the other. No --dump (it moves pairs; see CLAUDE.md).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v55-worksize
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents; X=$D/DISC21
echo $$ > $O/run4.pid; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $O/run4.out; }
one() { local tag=$1 corpus=$2 cache=$3; shift 3
    say "start $tag"
    /usr/bin/time -v $O/img-fp-dumpfix "$corpus" -r -x '*' -t 8 -v --no-last --cache "$cache" \
        --log-file $O/$tag.log -o $O/$tag.json "$@" 2> $O/$tag.time
    say "end $tag exit $?"; }
one all-2048 $D/IMGS-ALL $F/all.bin --work-size 2048
one all2-2048 $D/IMGS-ALL2 $F/all2.bin --work-size 2048
for n in 0 1; do
    OUT=$X/w1024b_$n; mkdir -p $OUT; say "start disc shard $n"
    /usr/bin/time -v $O/img-fp-dumpfix $X/queries/images/queries --against $X/refs$n/images/references \
        --work-size 1024 --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $X/cache/w1024.bin \
        --log-file $OUT/run.log -o $OUT/report.json 2> $OUT/time.txt
    say "end disc shard $n exit $?: $(grep Elapsed $OUT/time.txt | awk '{print $NF}')"
done
say "run4 done"
