#!/bin/bash
# Shipped size bar: byte-identity to the switched runs, keep rule on pairs, HOLI-TEST once, cached ABBA cost.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
echo 1000 > /proc/self/oom_score_adj
unset IMGFP_SEED IMGFP_SIZE_BAR IMGFP_ONE2ONE
log() { echo "$(date +%T) $*" >> $O/run10.out; }
one() { local tag=$1 bin=$2 corpus=$3 cache=$4
    rm -f $cache.matched
    log start $tag
    /usr/bin/time -v $O/$bin "$corpus" -r -x '*' -t 8 -v --no-last --cache "$cache" --log-file $O/$tag.log -o $O/$tag.json 2> $O/$tag.time
    log end $tag exit $? $(grep -E 'User time|System time|Elapsed' $O/$tag.time | awk '{print $NF}' | tr '\n' ' ')
    rm -f $cache.matched
}
one ship-all img-fp-ship $D/IMGS-ALL $F/all.bin
one ship-all2 img-fp-ship $D/IMGS-ALL2 $F/all2.bin
for b in head ship; do
  rm -f $F/all.bin.matched; log start pairs $b
  python3 $B/pairs.py run $O/img-fp-$b $O/pairs-$b --cache $F/all.bin >> $O/run10.err 2>&1
  log end pairs $b; rm -f $F/all.bin.matched
done
for b in head ship; do
  rm -f $O/fresh.bin.matched; log start htest $b
  python3 $B/families.py run $O/img-fp-$b $O/fam-test-$b --cache $O/fresh.bin --corpus htest >> $O/run10.err 2>&1
  log end htest $b; rm -f $O/fresh.bin.matched
done
cool() { sleep 60; }
for b in head ship ship head; do cool; one cost-$b-$(date +%H%M%S) img-fp-$b $D/IMGS-ALL2 $F/all2.bin; done
log done
