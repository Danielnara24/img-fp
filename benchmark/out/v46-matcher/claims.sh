#!/bin/bash
# Claim-graph rules, each off alone, on guided D 320 + floor r 143 + -k counting rejections.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
for off in none bridge cover cycle clean; do
  rm -f cache.bin.matched
  IMGFP_GUIDE_D=320 IMGFP_WORD_R=143 IMGFP_K_REJECT=1 IMGFP_CLAIMS_OFF=$off \
    ../../../vendor/venv/bin/python ../../families.py run ./img-fp-claims c-$off --cache cache.bin --only n0 n1-d1 n1-d2 n1-d3 n2-d1 n2-d2 n2-d3 f full
  echo "$off done"
done
echo claims done
