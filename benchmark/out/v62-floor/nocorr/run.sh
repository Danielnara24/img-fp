#!/bin/bash
# the floor with corroboration off (IMGFP_NO_CORR, temporary switch), at 0.7, kept JSON for a diff
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v62-floor
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db
for c in all all2; do m=/home/daniel/Documents/IMGS-ALL; [ $c = all2 ] && m=/home/daniel/Documents/IMGS-ALL2
  rm -f $C.matched
  IMGFP_NO_CORR=1 ./img-fp-floor-sw $m -r -x '*' -t 8 --no-last --cache $C --min-pixel-correlation 0.7 -o nocorr/$c.json 2> nocorr/$c.err
  rm -f $C.matched
  python3 -I ../../score.py --corpus $m nocorr/$c.json --csv nocorr/$c.pt.csv > nocorr/$c.score 2>&1
  python3 -I ../../analyse.py --corpus=$m nocorr/$c.json > nocorr/$c.an 2>&1
done
echo done > nocorr/done
