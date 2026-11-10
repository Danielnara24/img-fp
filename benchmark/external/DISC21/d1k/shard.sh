#!/bin/bash
# d1k/shard.sh N [EXTRA_REFS...]: as ../shard.sh, at --detail 1024, into d1k/shardN
# (or d1k/$TAG when TAG is set), sharing d1k/cache.bin (or $CACHE).
set -u
N=$1; shift; D=/home/daniel/Documents/DISC21; K=$D/d1k; OUT=$K/${TAG:-shard$N}
BIN=$K/img-fp; CACHE=${CACHE:-$K/cache.bin}
mkdir -p "$OUT"
used() { df -B1 --output=used "$D" | tail -1; }
base=$(used)
ag=(--against "$D/refs$N/images/references"); for e in "$@"; do ag+=(--against "$D/refs$e/images/references"); done
/usr/bin/time -v "$BIN" "$D/queries/images/queries" "${ag[@]}" --detail 1024 --min-pixel-correlation 0.5 \
    -t 8 -v --no-last --cache "$CACHE" --log-file "$OUT/run.log" \
    -o "$OUT/report.json" --dump "$OUT/dump.csv" 2> "$OUT/time.txt" &
pid=$!; echo $pid > "$OUT/pid"
echo -e "t\tanon_swap_kb\tdisk_delta_b" > "$OUT/samples.tsv"
t0=$(date +%s)
while kill -0 $pid 2>/dev/null; do
    kb=$(for p in $pid $(pgrep -P $pid); do awk '/^RssAnon|^VmSwap/{s+=$2} END{print s+0}' /proc/$p/status 2>/dev/null; done | awk '{s+=$1} END{print s+0}')
    echo -e "$(( $(date +%s) - t0 ))\t${kb:-0}\t$(( $(used) - base ))" >> "$OUT/samples.tsv"
    sleep 2
done
wait $pid; echo "exit $?" >> "$OUT/time.txt"
