#!/bin/bash
# Shared-part rule off / on, on guided D 320 + -k counting rejections (no floor).
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
P=../../../vendor/venv/bin/python
for r in off on; do
  rm -f cache.bin.matched
  if [ $r = on ]; then export IMGFP_PART_RULE=1; else unset IMGFP_PART_RULE; fi
  IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1 $P ../../families.py run ./img-fp-part p-$r --cache cache.bin --only n0 n1-d1 n1-d2 n1-d3 n2-d1 n2-d2 n2-d3 f full
done
unset IMGFP_PART_RULE
# Cliffs: one bar moved at a time, full corpora.
for r in off on; do
  for bar in "--min-aligned-points 8" "--min-aligned-points 5" "--min-aligned-points 3" "--min-frame-overlap 0.75" "--min-frame-overlap 0.6" "--min-frame-overlap 0.5" "--min-frame-overlap 0.4" "--min-frame-overlap 0.3" "--min-pixel-correlation 0.8" "--min-pixel-correlation 0.6" "--min-pixel-correlation 0.4" "--min-pixel-correlation 0.2" "-k 50" "-k 25"; do
    tag=$(echo "$bar" | tr -d ' -' )
    for t in i5 all; do
      rm -f cache.bin.matched
      if [ $r = on ]; then PR=IMGFP_PART_RULE=1; else PR=X=0; fi
      env IMGFP_GUIDE_D=320 IMGFP_K_REJECT=1 $PR ./img-fp-part --from-file g320/$t-full.list -x '*' --cache cache.bin --no-last $bar -o cliff-$r/$t-$tag.json 2> /dev/null
    done
    echo "$r $bar"
  done
done
echo part done
