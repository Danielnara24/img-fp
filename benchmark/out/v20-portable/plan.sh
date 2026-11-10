#!/bin/bash
# Cooled, cold, cache-evicted A/B of the release flavour (x86-64-v3) against the
# portable builds (plain x86-64 and x86-64-v2, both dispatching the AVX2 kernels
# at start-up). Outputs are kept for scoring.
cd "$(dirname "$0")"
D=/home/daniel/Documents
python3 ab.py $D/IMGS imgs img-fp-x86-64-v3 img-fp-x86-64 img-fp-x86-64-v2 img-fp-x86-64-v2 img-fp-x86-64 img-fp-x86-64-v3 img-fp-x86-64-v3 img-fp-x86-64 img-fp-x86-64-v2
for c in IMGS2 IMGS3 IMGS4; do
  python3 ab.py $D/$c $(echo $c | tr A-Z a-z) img-fp-x86-64-v3 img-fp-x86-64 img-fp-x86-64 img-fp-x86-64-v3
done
python3 ab.py /home/daniel/Downloads/archive found img-fp-x86-64-v3 img-fp-x86-64 img-fp-x86-64 img-fp-x86-64-v3
echo PLAN DONE
