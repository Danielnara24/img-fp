#!/bin/bash
X=/home/daniel/Documents/DISC21; P=$X/proxy; B=$X/lv/img-fp-n
one() { local tag=$1; shift; mkdir -p $P/$tag; sleep 30
  env "$@" /usr/bin/time -f "%U" $B $P/q --against $P/r --follow-symlinks --min-pixel-correlation 0.5 -t 8 -v --no-last --no-cache --log-file $P/$tag/run.log -o $P/$tag/report.json 2> $P/$tag/time.txt
  echo "$tag $(tail -1 $P/$tag/time.txt) $(python3 $P/cmp.py $P/$tag/report.json | tr '\n' ' ')" >> $P/results3.txt; }
LV="IMGFP_LEVELS=1 IMGFP_LOWEST=256 IMGFP_GRIDMAX=1024 IMGFP_CAPX=1.4 IMGFP_KNOWN=anchor"

one sh1 $LV IMGFP_SHAREVOC=1
one sh2 $LV IMGFP_SHAREVOC=1 IMGFP_VOCX=2
one sh1b $LV IMGFP_SHAREVOC=1
one p4 X=1
echo done >> $P/results3.txt
