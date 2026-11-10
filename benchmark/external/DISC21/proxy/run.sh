#!/bin/bash
X=/home/daniel/Documents/DISC21; P=$X/proxy; B=$X/lv/img-fp-k
one() { local tag=$1; shift; mkdir -p $P/$tag
  env "$@" /usr/bin/time -f "%U" $B $P/q --against $P/r --follow-symlinks --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $P/$tag.bin --log-file $P/$tag/run.log -o $P/$tag/report.json 2> $P/$tag/time.txt
  echo "$tag $(tail -1 $P/$tag/time.txt) $(python3 $P/cmp.py $P/$tag/report.json | tr '\n' ' ')" >> $P/results.txt; }
LV="IMGFP_LEVELS=1 IMGFP_LOWEST=256 IMGFP_GRIDMAX=1024"
one plain X=1
one lv-anchor $LV IMGFP_KNOWN=anchor
one lv-none $LV IMGFP_KNOWN=none
one lv-none-x14 $LV IMGFP_KNOWN=none IMGFP_CAPX=1.4
one lv-none-x2 $LV IMGFP_KNOWN=none IMGFP_CAPX=2
echo done >> $P/results.txt
