#!/bin/bash
# Seeding: shared words (off) against fixed hashing (IMGFP_SEED=bits,tables), cached, IMGS-ALL and IMGS-ALL2.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo 1000 > /proc/self/oom_score_adj
one() { local tag=$1 seed=$2 corpus=$3 cache=$4
    rm -f $cache.matched
    echo "$(date +%T) start $tag" >> $O/run1.out
    if [ $seed = off ]; then unset IMGFP_SEED; else export IMGFP_SEED=$seed; fi
    /usr/bin/time -v $O/img-fp-seed "$corpus" -r -x '*' -t 8 -v --no-last --cache "$cache" --log-file $O/$tag.log -o $O/$tag.json 2> $O/$tag.time
    echo "$(date +%T) end $tag exit $? $(grep -E 'User time|System time|Elapsed|Maximum resident' $O/$tag.time | awk '{print $NF}' | tr '\n' ' ')" >> $O/run1.out
    rm -f $cache.matched; unset IMGFP_SEED
}
for s in off 14,5 11,4 18,7; do
  one all-$s $s $D/IMGS-ALL $F/all.bin
  one all2-$s $s $D/IMGS-ALL2 $F/all2.bin
done
echo "$(date +%T) all done" >> $O/run1.out
