#!/bin/bash
# The checks not run for the size bar: cold cost ABBA, DISC21 proxy, --detail, cliffs. HEAD (0.80.0) against ship.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents; PX=/home/daniel/Documents/DISC21/proxy
echo 1000 > /proc/self/oom_score_adj
unset IMGFP_SEED IMGFP_SIZE_BAR IMGFP_ONE2ONE
say() { echo "$(date +%T) $*" >> $O/run11.out; }
temp() { sensors | grep -m1 Tctl | grep -o '[0-9]*\.[0-9]' | head -1; }
sleep 240; IDLE=$(temp); say "idle $IDLE"
cool() { sleep 60; local n=0; while (( $(echo "$(temp) > $IDLE + 3" | bc) )) && (( n < 60 )); do sleep 10; n=$((n+1)); done; }
tm() { echo "$(grep -E 'User time|System time' $1 | awk '{print $NF}' | tr '\n' ' ')$(grep Elapsed $1 | awk '{print $NF}') maxrss $(grep Maximum $1 | awk '{print $NF}')"; }
# 1. cold cost, IMGS-ALL, ABBA
for b in head ship ship head; do cool; tag=cold/$b-$(date +%H%M%S); say "start $tag $(temp)"
  /usr/bin/time -v $O/img-fp-$b $D/IMGS-ALL -r -x '*' -t 8 -v --no-last --no-cache -o $O/$tag.json --log-file $O/$tag.log 2> $O/$tag.time
  say "end $tag exit $? $(tm $O/$tag.time)"; done
# 2. DISC21 proxy, cached analysis
for b in head ship; do rm -f $PX/plain.bin.matched; say "start proxy $b"
  $O/img-fp-$b $PX/q --against $PX/r --follow-symlinks --min-pixel-correlation 0.5 -t 8 --no-last --cache $PX/plain.bin -o $O/proxy/$b.json 2> $O/proxy/$b.err
  say "end proxy $b exit $?"; rm -f $PX/plain.bin.matched; done
# 3. --detail on IMGS-ALL2: the cache made once at 2048, then read at 1024 and 2048
C=$O/detail/a2.bin
for run in "head 2048" "ship 2048" "head 1024" "ship 1024"; do set -- $run; rm -f $C.matched; say "start detail $1 $2"
  $O/img-fp-$1 $D/IMGS-ALL2 -r -x '*' -t 8 --no-last --cache $C --detail $2 -o $O/detail/$1-$2.json 2> $O/detail/$1-$2.err
  say "end detail $1 $2 exit $?"; done
rm -f $C $C.*
# 4. cliffs, cached
for bar in "--min-aligned-points 8" "--min-aligned-points 6" "--min-aligned-points 5" "--min-aligned-points 4" "--min-aligned-points 3" "--min-frame-overlap 0.75" "--min-frame-overlap 0.65" "--min-frame-overlap 0.6" "--min-frame-overlap 0.5" "--min-frame-overlap 0.4" "--min-frame-overlap 0.3" "--min-frame-overlap 0.25" "--min-pixel-correlation 0.9" "--min-pixel-correlation 0.8" "--min-pixel-correlation 0.6" "--min-pixel-correlation 0.5" "--min-pixel-correlation 0.4" "--min-pixel-correlation 0.3" "--min-pixel-correlation 0.2" "-k 100" "-k 50" "-k 25" "-k 10"; do
  t=$(echo "$bar" | tr -d ' -')
  for c in all all2; do
    if [ $c = all ]; then corpus=$D/IMGS-ALL; cache=$F/all.bin; else corpus=$D/IMGS-ALL2; cache=$F/all2.bin; fi
    for b in head ship; do rm -f $cache.matched
      $O/img-fp-$b $corpus -r -x '*' -t 8 --no-last --cache $cache $bar -o $O/cliffs/$c-$t-$b.json 2> /dev/null
      rm -f $cache.matched; done
  done
  say "cliff $bar"
done
say done
