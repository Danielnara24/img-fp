#!/bin/bash
# ABBA: 0.72.0 against the stack (guided D320, -k counting rejections, shared-part rule v3). Cached; kept matching removed.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
run() {
  sleep 60; rm -f cache.bin.matched
  if [ $1 = base ]; then B=./img-fp-0.72.0; E=X=0; else B=./img-fp-part3; E="IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1 IMGFP_PART_RULE=1"; fi
  /usr/bin/time -f "$1 $2 %e s %U+%S cpu" env $E $B --from-file $2 -x '*' --cache cache.bin --no-last -o /dev/null 2>&1 | tail -1
  grep MHz /proc/cpuinfo | awk '{s+=$4} END {print "  MHz after", int(s/NR)}'
}
for L in base/all-f.list g320/i5-full.list; do run base $L; run stack $L; run stack $L; run base $L; done
echo cost done
