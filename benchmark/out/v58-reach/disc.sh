#!/bin/bash
# disc.sh BIN NAME [ENV=V ...]: DISC21 shard 0 cached, bar 0.5, trace of the true pairs
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v58-reach; X=/home/daniel/Documents/DISC21
bin=$1 n=$2; shift 2
mkdir -p $O/disc-$n; cp $O/trace.txt $O/disc-$n/trace.txt
rm -f $X/cache57/c.db.matched
env ${TRACE:+IMGFP_TRACE=$O/disc-$n/trace.txt} "$@" /usr/bin/time -v $O/$bin $X/queries/images/queries --against $X/${REFS:-refs0}/images/references --min-pixel-correlation 0.5 -t 8 -v --no-last --cache $X/cache57/c.db --log-file $O/disc-$n/run.log -o $O/disc-$n/report.json 2> $O/disc-$n/time.txt
rm -f $X/cache57/c.db.matched
echo "disc-$n $(grep 'User time' $O/disc-$n/time.txt | awk '{print $NF}') $(grep Elapsed $O/disc-$n/time.txt | awk '{print $NF}')"
