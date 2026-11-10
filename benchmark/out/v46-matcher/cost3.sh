#!/bin/bash
# ABBA: guided D320 without / with the word floor (r 143). Full IMGS5, then the found mix. Cached; kept matching removed.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
run() { sleep 60; rm -f cache.bin.matched; /usr/bin/time -f "$1 $2 %e s %U+%S cpu" env IMGFP_GUIDE_D=320 IMGFP_WORD_R=$1 ./img-fp-floor --from-file $2 -x '*' --cache cache.bin --no-last -o /dev/null 2>&1 | tail -1; }
for L in g320/i5-full.list base/all-f.list; do run 0 $L; run 143 $L; run 143 $L; run 0 $L; done
