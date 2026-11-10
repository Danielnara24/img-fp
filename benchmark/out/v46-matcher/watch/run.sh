#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
for D in 0 320; do
  for t in all i5; do
    rm -f cache.bin.matched
    IMGFP_WATCH=watch/sib.tsv IMGFP_GUIDE_D=$D ./img-fp-watch --from-file g320/$t-n0.list -x '*' --cache cache.bin --no-last -o watch/$t-n0-$D.json 2> watch/$t-n0-$D.log
    rm -f cache.bin.matched
    IMGFP_WATCH=watch/sib.tsv IMGFP_GUIDE_D=$D ./img-fp-watch --from-file g320/$t-full.list -x '*' --cache cache.bin --no-last -o watch/$t-full-$D.json 2> watch/$t-full-$D.log
  done
done
echo watch done
