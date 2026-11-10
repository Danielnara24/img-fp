#!/bin/bash
# measure.sh DIR OUT: one cold img-fp run over DIR with its own cache,
# sampling every 2 s the process tree's RssAnon + swap, the disk used on
# the cache's filesystem (the spill file is unlinked, so only df sees it)
# and the cache file's size.
set -u
DIR=$1; OUT=$2
BIN=/home/daniel/Documents/Vscode_repositories/img-fp/target/plain/release/img-fp
mkdir -p "$OUT"; rm -f "$OUT"/cache.bin*
used() { df -B1 --output=used "$OUT" | tail -1; }
base=$(used)
/usr/bin/time -v "$BIN" "$DIR" -r -x '*' -t 8 -v --cache "$OUT/cache.bin" \
    --log-file "$OUT/run.log" -o "$OUT/report.json" 2> "$OUT/time.txt" &
pid=$!
echo -e "t\tanon_swap_kb\tdisk_delta_b\tcache_b" > "$OUT/samples.tsv"
t0=$(date +%s)
while kill -0 $pid 2>/dev/null; do
    kb=0
    for p in $pid $(pgrep -P $pid) $(pgrep -P "$(pgrep -P $pid | head -1)" 2>/dev/null); do
        s=$(awk '/^RssAnon|^VmSwap/{s+=$2} END{print s+0}' /proc/$p/status 2>/dev/null)
        kb=$((kb + ${s:-0}))
    done
    c=$(stat -c %s "$OUT/cache.bin" 2>/dev/null || echo 0)
    echo -e "$(( $(date +%s) - t0 ))\t$kb\t$(( $(used) - base ))\t$c" >> "$OUT/samples.tsv"
    sleep 2
done
wait $pid; echo "exit $?" >> "$OUT/time.txt"
