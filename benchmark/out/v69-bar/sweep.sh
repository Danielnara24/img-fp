#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp
W=benchmark/out/v69-bar
for b in 8 6 4 3 2 16 24; do
  vendor/venv/bin/python $W/editcopies.py run $W/img-fp-bar bar$b IMGFP_WR_SAME=$b
  vendor/venv/bin/python $W/editcopies.py score bar$b >> $W/sweep.out
done
echo done >> $W/sweep.out
