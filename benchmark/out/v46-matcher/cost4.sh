#!/bin/bash
# ABBA: -k counting rejections off / on (guided D320 + floor r143). Cached; kept matching removed.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
run() { sleep 60; rm -f cache.bin.matched; /usr/bin/time -f "$1 $2 %e s %U+%S cpu" env IMGFP_GUIDE_D=320 IMGFP_WORD_R=143 $1 ./img-fp-claims -v --from-file $2 -x '*' --cache cache.bin --no-last -o /dev/null 2>&1 | grep -E "k counts|cpu$"; }
for L in g320/i5-full.list base/all-f.list; do run X=0 $L; run IMGFP_K_REJECT=1 $L; run IMGFP_K_REJECT=1 $L; run X=0 $L; done
echo cost done
