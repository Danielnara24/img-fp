#!/bin/bash
# Profile full IMGS5 (cached, kept matching removed): 0.72.0-equivalent (all off), guided, guided + -k, the stack.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
run() {
  sleep 60; rm -f cache.bin.matched
  /usr/bin/time -f "$1 %e s %U+%S cpu" env $2 ./img-fp-prof3 -v --from-file g320/i5-full.list -x '*' --cache cache.bin --no-last -o /dev/null > prof5/$1.txt 2>&1
  tail -1 prof5/$1.txt
}
run off X=0
run g IMGFP_GUIDE_D=320
run gk "IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1"
run gkp "IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1 IMGFP_PART_RULE=1"
run gkp2 "IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1 IMGFP_PART_RULE=1"
run off2 X=0
echo prof done
