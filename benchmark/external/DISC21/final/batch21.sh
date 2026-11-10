#!/bin/bash
# Final checks: default / --fast / --against / --dump identical to HEAD; DISC21 proxy.
X=/home/daniel/Documents/DISC21; F=$X/final; H=$X/commit/img-fp; N=$F/img-fp
A=/home/daniel/Documents/IMGS-ALL; A2=/home/daniel/Documents/IMGS-ALL2; I=/home/daniel/Documents/IMGS; G=/home/daniel/Documents/IMGS-AGAINST
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $F/batch21.out; }
run() { local tag=$1 bin=$2; shift 2; say "start $tag"
  /usr/bin/time -f "%U %e" $bin "$@" -t 8 --no-last --log-file $F/$tag.log 2> $F/$tag.time
  say "end $tag exit $? $(tail -1 $F/$tag.time)"; }
rm -f $X/fixtest/all.bin.matched $X/fixtest/all2.bin.matched
run all-new $N $A -r -x '*' --cache $X/fixtest/all.bin -o $F/all-new.json
rm -f $X/fixtest/all2.bin.matched
run all2-new $N $A2 -r -x '*' --cache $X/fixtest/all2.bin -o $F/all2-new.json
rm -f $X/fixtest/all2.bin.matched
run all2-dump-new $N $A2 -r -x '*' --cache $X/fixtest/all2.bin -o $F/all2-dn.json --dump $F/all2-dump-new.csv
rm -f $X/fixtest/all2.bin.matched
run all2-dump-head $H $A2 -r -x '*' --cache $X/fixtest/all2.bin -o $F/all2-dh.json --dump $F/all2-dump-head.csv
run ag-new $N $G/scan --against $G/library --follow-symlinks -r -x '*' --cache $X/fixtest/all.bin -o $F/ag-new.json
rm -f $X/fixtest/all.bin.matched
run ag-head $H $G/scan --against $G/library --follow-symlinks -r -x '*' --cache $X/fixtest/all.bin -o $F/ag-head.json
rm -f $X/fixtest/all.bin.matched
run imgs-fast-new $N $I -r -x '*' --no-cache --fast -o $F/imgs-fast-new.json
run imgs-256-head $H $I -r -x '*' --no-cache --work-size 256 -o $F/imgs-256-head.json
P=$X/proxy
for t in px-new:$N: px-new1024:$N:--detail px-head1024:$H:--work-size; do IFS=: read tag bin flag <<< "$t"
  run $tag $bin $P/q --against $P/r --follow-symlinks --min-pixel-correlation 0.5 -v --no-cache ${flag:+$flag 1024} -o $F/$tag.json; done
say "batch21 done"
