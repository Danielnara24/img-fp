#!/bin/bash
# Small families (families.py): words against hashed seeding, four corpora.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
F=/home/daniel/Documents/DISC21/fixtest
echo 1000 > /proc/self/oom_score_adj
fam() { local corpus=$1 cache=$2 cfg=$3
    rm -f $cache.matched
    if [ $cfg = off ]; then unset IMGFP_SEED; else export IMGFP_SEED=$cfg; fi
    echo "$(date +%T) start $corpus $cfg" >> $O/run7.out
    python3 $B/families.py run $O/img-fp-seedE $O/fam-$cfg --cache $cache --corpus $corpus 2>> $O/run7.err
    echo "$(date +%T) end $corpus $cfg" >> $O/run7.out
    rm -f $cache.matched; unset IMGFP_SEED
}
for cfg in off 14,3 14,5; do
  fam hdev $O/fresh.bin $cfg
  fam all $F/all.bin $cfg
  fam all2 $F/all2.bin $cfg
  fam i5 $O/fresh.bin $cfg
done
echo "$(date +%T) done" >> $O/run7.out
