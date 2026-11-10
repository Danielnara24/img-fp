#!/bin/bash
# The shared-part rule with lone files resolved (img-fp-part2), on guided D 320 + -k counting rejections.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
P=../../../vendor/venv/bin/python
export IMGFP_PART_RULE=1
rm -f cache.bin.matched
IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1 $P ../../families.py run ./img-fp-part2 p-on2 --cache cache.bin --only n0 n1-d1 n1-d2 n1-d3 n2-d1 n2-d2 n2-d3 f full
mkdir -p cliff-on2
for bar in "--min-aligned-points 8" "--min-aligned-points 5" "--min-aligned-points 3" "--min-frame-overlap 0.75" "--min-frame-overlap 0.6" "--min-frame-overlap 0.5" "--min-frame-overlap 0.4" "--min-frame-overlap 0.3" "--min-pixel-correlation 0.8" "--min-pixel-correlation 0.6" "--min-pixel-correlation 0.4" "--min-pixel-correlation 0.2" "-k 50" "-k 25"; do
  tag=$(echo "$bar" | tr -d ' -' )
  for t in i5 all; do
    rm -f cache.bin.matched
    IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1 ./img-fp-part2 --from-file g320/$t-full.list -x '*' --cache cache.bin --no-last $bar -o cliff-on2/$t-$tag.json 2> /dev/null
  done
  echo "on2 $bar"
done
echo part2 done
