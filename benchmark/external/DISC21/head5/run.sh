#!/bin/bash
# head5/run.sh: HEAD's default (512) on shard 5, as the old shard.sh ran it, to see which build the 36 "lost" pairs belong to.
set -u
D=/home/daniel/Documents/DISC21; O=$D/head5; Z=$D/zips/references_5.zip
URL=https://dl.fbaipublicfiles.com/image_similarity_challenge/public/references_5.zip
echo "$(date '+%T') download" >> $O/log
curl -sSL --retry 5 --retry-delay 10 -C - -o "$Z" "$URL" || { echo "download failed" >> $O/log; exit 1; }
unzip -q "$Z" -d "$D/refs5" && rm -f "$Z"
echo "$(date '+%T') unzipped $(find $D/refs5 -type f | wc -l)" >> $O/log
/usr/bin/time -v $D/commit/img-fp "$D/queries/images/queries" --against "$D/refs5/images/references" --min-pixel-correlation 0.5 \
  -t 8 -v --no-last --cache "$D/cache/cache.bin" --log-file "$O/run.log" -o "$O/report.json" --dump "$O/dump.csv" 2> "$O/time.txt"
echo "$(date '+%T') ended exit $?" >> $O/log
