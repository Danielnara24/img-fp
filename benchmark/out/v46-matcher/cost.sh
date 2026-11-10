#!/bin/bash
# ABBA: off vs guided (D=150), cached found mix, 60 s apart; the kept matching removed before each run.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
for t in off on on off; do
  sleep 60
  D=0; [ $t = on ] && D=150
  rm -f cache.bin.matched
  /usr/bin/time -f "$t %e s %U+%S cpu" env IMGFP_GUIDE_D=$D ./img-fp-guided --from-file base/all-f.list -x '*' --cache cache.bin --no-last -o /dev/null 2>&1 | tail -1
  grep MHz /proc/cpuinfo | awk '{s+=$4} END {print "  MHz after", int(s/NR)}'
done
