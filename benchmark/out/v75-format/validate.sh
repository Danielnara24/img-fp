#!/bin/bash
# validate.py: v0.85.0 (base) against 0.85.0 + the 0.65 / 5 defaults + identical sets stated once (new).
# Both builds share one analysis (each reads the other's records), so they share caches.
cd "$(dirname "$0")"
python3 ../../validate.py run "$PWD/img-fp-085" "$PWD/img-fp-new" "$PWD/validate" \
  --base-cache "$PWD/cache/c.db" --new-cache "$PWD/cache/c.db" \
  --base-disc-cache "$PWD/disc-cache/c.db" --new-disc-cache "$PWD/disc-cache/c.db"
echo "VALIDATE EXIT $?"
python3 ../../validate.py report validate > validate.report 2>&1
echo REPORT DONE
