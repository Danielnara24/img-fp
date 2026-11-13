#!/bin/bash
# The two-file families for each bar of the crop check, with open files deleting and mirrors taking part.
cd /home/daniel/Documents/Vscode_repositories/img-fp
V=benchmark/out/v68-recall
for b in 12 16 28 60; do
  IMGFP_MIRROR=1 IMGFP_WR_IN=$b vendor/venv/bin/python benchmark/pairs.py run $V/bin-a $V/pairs-in$b --cache benchmark/out/v67-keep/cache/c.db > $V/pairs-in$b.log 2>&1
  echo "in$b done" >> $V/pairs_sweep.log
done
