#!/bin/bash
# One-to-one inlier count (IMGFP_ONE2ONE=1) against the count by source position, words seeding.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo 1000 > /proc/self/oom_score_adj
unset IMGFP_SEED
fam() { local corpus=$1 cache=$2
    rm -f $cache.matched
    echo "$(date +%T) start fam $corpus" >> $O/run8.out
    IMGFP_ONE2ONE=1 python3 $B/families.py run $O/img-fp-o2o $O/fam-o2o --cache $cache --corpus $corpus 2>> $O/run8.err
    echo "$(date +%T) end fam $corpus" >> $O/run8.out
    rm -f $cache.matched
}
one() { local tag=$1 sw=$2 corpus=$3 cache=$4
    rm -f $cache.matched
    echo "$(date +%T) start $tag" >> $O/run8.out
    IMGFP_ONE2ONE=$sw /usr/bin/time -v $O/img-fp-o2o "$corpus" -r -x '*' -t 8 -v --no-last --cache "$cache" --log-file $O/$tag.log -o $O/$tag.json 2> $O/$tag.time
    echo "$(date +%T) end $tag exit $? $(grep -E 'User time|System time|Elapsed' $O/$tag.time | awk '{print $NF}' | tr '\n' ' ')" >> $O/run8.out
    rm -f $cache.matched
}
fam hdev $O/fresh.bin
fam all $F/all.bin
fam all2 $F/all2.bin
fam i5 $O/fresh.bin
one o2o-all 1 $D/IMGS-ALL $F/all.bin
one o2o-all2 1 $D/IMGS-ALL2 $F/all2.bin
one o2o-all-off 0 $D/IMGS-ALL $F/all.bin
one o2o-all2-off 0 $D/IMGS-ALL2 $F/all2.bin
echo "$(date +%T) done" >> $O/run8.out
