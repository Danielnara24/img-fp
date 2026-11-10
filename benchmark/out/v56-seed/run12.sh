#!/bin/bash
# DISC21 shard 0 (50k dev queries --against references_0), bar 0.5 with --dump, 0.80.0 then the size bar.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed; X=/home/daniel/Documents/DISC21
URL=https://dl.fbaipublicfiles.com/image_similarity_challenge/public
echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date +%T) $*" >> $O/run12.out; }

say "fetch references_0"
curl -sSL --retry 5 --retry-delay 10 -C - -o $X/zips/references_0.zip $URL/references_0.zip || { say "download failed"; exit 1; }
say "unzip"; (cd $X/refs0 && unzip -q $X/zips/references_0.zip) && rm -f $X/zips/references_0.zip
say "refs: $(find $X/refs0 -type f | wc -l) files"
for b in head ship; do mkdir -p $O/disc/$b; rm -f $X/cache/cache.bin.matched; say "start $b"
  /usr/bin/time -v $O/img-fp-$b $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache $X/cache/cache.bin --log-file $O/disc/$b/run.log -o $O/disc/$b/report.json --dump $O/disc/$b/dump.csv 2> $O/disc/$b/time.txt
  say "end $b exit $? $(grep -E 'User time' $O/disc/$b/time.txt | awk '{print $NF}') $(grep Elapsed $O/disc/$b/time.txt | awk '{print $NF}')"
done
rm -f $X/cache/cache.bin.matched
# The DISC21 proxy (its library links point into refs0/images, gone when run11 tried it).
PX=$X/proxy
for b in head ship; do rm -f $PX/plain.bin.matched; say "start proxy $b"
  $O/img-fp-$b $PX/q --against $PX/r --follow-symlinks --min-pixel-correlation 0.5 -t 8 --no-last --cache $PX/plain.bin -o $O/proxy/$b.json 2> $O/proxy/$b.err
  say "end proxy $b exit $?"; rm -f $PX/plain.bin.matched; done
rm -rf $X/refs0/images; say done
