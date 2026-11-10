#!/bin/bash
# ABBA x2: (old guided D150 vs new guided D150), then (off vs new D320). Cached found mix, 60 s apart; kept matching removed first.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
run() { sleep 60; rm -f cache.bin.matched; /usr/bin/time -f "$1 %e s %U+%S cpu" env IMGFP_GUIDE_D=$3 $2 --from-file base/all-f.list -x '*' --cache cache.bin --no-last -o /dev/null 2>&1 | tail -1; grep MHz /proc/cpuinfo | awk '{s+=$4} END {print "  MHz after", int(s/NR)}'; }
run old150 ./img-fp-guided 150; run new150 ./img-fp-guided2 150; run new150 ./img-fp-guided2 150; run old150 ./img-fp-guided 150
run off ./img-fp-guided2 0; run new320 ./img-fp-guided2 320; run new320 ./img-fp-guided2 320; run off ./img-fp-guided2 0
