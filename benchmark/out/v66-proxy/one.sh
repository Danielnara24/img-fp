#!/bin/bash
# one.sh NAME BINARY CACHE proxy|shard [args...]
X=/home/daniel/Documents/DISC21; n=$1; b=$2; c=$3; w=$4; shift 4
cd "$(dirname "$0")"
[ -s $n.found ] && exit 0
if [ $w = p2 ]; then set -- p2/q --against p2/r --follow-symlinks "$@"; ids=p2/refs.ids
elif [ $w = proxy ]; then set -- $X/proxy/q --against $X/proxy/r --follow-symlinks "$@"; ids=$X/proxy/refs.ids
else set -- $X/queries/images/queries --against $X/refs0/images/references "$@"; ids=$X/refs0.ids; fi
rm -f $c.matched
/usr/bin/time -v ./img-fp-$b "$@" --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $c -o $n.json 2> $n.err
rm -f $c.matched
echo "$(date +%T) $n $(grep 'User time' $n.err | awk '{print $NF}') $(grep Elapsed $n.err | awk '{print $NF}')" >> log
python3 -I score.py $n.json $ids | sed "s/^/$n /" >> log
rm -f $n.json
