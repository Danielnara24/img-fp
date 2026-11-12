#!/bin/bash
# disc.sh NAME BINARY [ENV=..]... : DISC21 shard 0 at 0.5, cached on cache65
X=/home/daniel/Documents/DISC21; n=$1; b=$2; shift 2
c=$X/cache65/c.db; [ "$b" = head ] && c=$X/cache57/c.db
rm -f $c.matched
env "$@" /usr/bin/time -v ./img-fp-$b $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation ${BAR:-0.5} -t 8 -v --no-last --cache $c -o disc/$n.json 2> disc/$n.err
rm -f $c.matched
echo "$(date +%T) $n $(grep -E 'User time' disc/$n.err | awk '{print $NF}') $(grep Elapsed disc/$n.err | awk '{print $NF}')" >> disc/log
python3 -I dscore.py disc/$n.json | grep -E "bar 0.[57]" | sed "s/^/$n /" >> disc/log
