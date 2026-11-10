#!/bin/bash
# The stack (guided D 320 + -k counting rejections + shared-part rule v3) at the 0.72.0 cliff steps not yet run (cliff-on3 has the rest).
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
export IMGFP_PART_RULE=1 IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1
for bar in "--min-aligned-points 6" "--min-aligned-points 4" "--min-frame-overlap 0.65" "--min-frame-overlap 0.25" "--min-pixel-correlation 0.9" "--min-pixel-correlation 0.5" "--min-pixel-correlation 0.3" "-k 100" "-k 10"; do
  tag=$(echo "$bar" | tr -d ' -' )
  for t in i5 all; do
    rm -f cache.bin.matched
    ./img-fp-part3 --from-file g320/$t-full.list -x '*' --cache cache.bin --no-last $bar -o cliff-on3/$t-$tag.json 2> /dev/null
  done
  echo "on3 $bar"
done
# check: 0.72.0 from the same lists reproduces v44's cliff file at one step
rm -f cache.bin.matched
env -u IMGFP_PART_RULE -u IMGFP_GUIDE_D -u IMGFP_K_REJECT ./img-fp-0.72.0 --from-file g320/i5-full.list -x '*' --cache cache.bin --no-last --min-frame-overlap 0.6 -o cliff-on3/check-0720-i5-overlap0.6.json 2> /dev/null
echo cliff6 done
