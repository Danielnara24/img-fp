#!/bin/bash
# -k counts rejections (IMGFP_K_REJECT), on guided D 320 + floor r 143.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
for t in i5 all; do
  for s in n0 full; do
    rm -f cache.bin.matched
    /usr/bin/time -f "$t-$s %e s %U+%S cpu" env IMGFP_WATCH=watch/sib.tsv IMGFP_GUIDE_D=320 IMGFP_WORD_R=143 IMGFP_K_REJECT=1 ./img-fp-krej -v --from-file g320/$t-$s.list -x '*' --cache cache.bin --no-last -o watch/$t-$s-kr.json 2> watch/$t-$s-kr.log
    tail -1 watch/$t-$s-kr.log
  done
done
echo runk done
