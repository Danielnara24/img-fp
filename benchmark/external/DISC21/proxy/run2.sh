#!/bin/bash
X=/home/daniel/Documents/DISC21; P=$X/proxy; B=$X/lv/img-fp-m
one() { local tag=$1; shift; mkdir -p $P/$tag; sleep 30
  env "$@" /usr/bin/time -f "%U" $B $P/q --against $P/r --follow-symlinks --min-pixel-correlation 0.5 -t 8 -v --no-last --no-cache --log-file $P/$tag/run.log -o $P/$tag/report.json 2> $P/$tag/time.txt
  echo "$tag $(tail -1 $P/$tag/time.txt) $(python3 $P/cmp.py $P/$tag/report.json | tr '\n' ' ')" >> $P/results2.txt; }
LV="IMGFP_LEVELS=1 IMGFP_LOWEST=256 IMGFP_GRIDMAX=1024 IMGFP_CAPX=1.4"
one p1 X=1
one from1 $LV IMGFP_KNOWN=anchor
one none1 $LV IMGFP_KNOWN=none
one from2 $LV IMGFP_KNOWN=anchor
one p2 X=1
echo done >> $P/results2.txt
