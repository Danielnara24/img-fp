#!/bin/bash
cd "$(dirname "$0")"; X=/home/daniel/Documents/DISC21; c=$X/cache66/c.db; rm -f $c.matched
IMGFP_SL512=1 /usr/bin/time -v ./img-fp-h2 $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $c -o h1/disc-sl.json 2> h1/disc-sl.err; rm -f $c.matched
python3 -I dscore.py h1/disc-sl.json > h1/disc-sl.score 2>&1
L=$PWD/../lad8/l.db; rm -f $L.matched
IMGFP_SL512=1 /usr/bin/time -v ./img-fp-h2 /home/daniel/Documents/IMGS-ALL2 -r -x '*' -t 8 -v --no-last --cache $L -o h1/all2-sl.json 2> h1/all2-sl.err; rm -f $L.matched
python3 -I /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/analyse.py --corpus=/home/daniel/Documents/IMGS-ALL2 h1/all2-sl.json > h1/all2-sl.an 2>&1; rm -f h1/all2-sl.json
echo DONE > h1/sl.done
