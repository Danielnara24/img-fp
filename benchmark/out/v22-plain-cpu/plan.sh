#!/bin/bash
# Cooled, cold, cache-evicted A/B of the 0.29.1 plain build against the build
# that shares the analysis between files with one working plane, A B B A on each
# corpus. Outputs are kept for the byte-identity check.
cd "$(dirname "$0")"
D=/home/daniel/Documents
for c in IMGS IMGS2 IMGS3 IMGS4; do
  python3 ab.py $D/$c $(echo $c | tr A-Z a-z) plain-base plain-twins plain-twins plain-base
done
echo PLAN DONE
