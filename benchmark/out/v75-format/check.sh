#!/bin/bash
# The format change is lossless: the commit before it and the new build, cached, on both corpora and IMGS-AGAINST.
cd "$(dirname "$0")"; mkdir -p check cache
for c in IMGS-ALL IMGS-ALL2; do
  for b in pre new; do
    rm -f cache/c.db.matched
    ./img-fp-$b ~/Documents/$c -r -x '*' -t 8 --no-last --cache cache/c.db -o check/$c-$b.json 2> check/$c-$b.err
    echo "$c $b exit $?"
  done
  python3 same.py check/$c-pre.json check/$c-new.json
done
A=~/Documents/IMGS-AGAINST
for b in pre new; do
  rm -f cache/c.db.matched
  ./img-fp-$b $(ls -d $A/scan* | head -1) --against $(ls -d $A/lib* | head -1) --follow-symlinks -r -x '*' -t 8 --no-last --cache cache/c.db -o check/against-$b.json 2> check/against-$b.err
  echo "against $b exit $?"
done
python3 same.py check/against-pre.json check/against-new.json
rm -f cache/c.db.matched
echo CHECK DONE
