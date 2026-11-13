#!/bin/bash
# Fill one cache with HEAD's analyses of IMGS-ALL2 and IMGS-ALL, and keep HEAD's reports.
cd "$(dirname "$0")"
for c in ALL2 ALL; do
  ./base/img-fp /home/daniel/Documents/IMGS-$c -r -x '*' -t 8 -v --no-last --cache cache/c.db -o base/$c.json > base/$c.err 2>&1
  echo "$c exit $?" >> fill.log
done
