#!/bin/bash
# piece.sh [OUTNAME [img-fp options]]: 25k dev queries --against 25k references (symlink farms in p25/),
# sharing cache/cache.bin, writing shardN/: report, dump, log, samples.
set -u
D=/home/daniel/Documents/DISC21; OUT=$D/p25/${1:-out}; shift; EXTRA=("$@")
BIN=/home/daniel/Documents/Vscode_repositories/img-fp/target/plain/release/img-fp
mkdir -p "$OUT"
used() { df -B1 --output=used "$D" | tail -1; }
base=$(used)
/usr/bin/time -v "$BIN" "$D/p25/q" --against "$D/p25/r" --follow-symlinks "${EXTRA[@]}" \
    -t 8 -v --cache "$D/cache/cache.bin" --log-file "$OUT/run.log" \
    -o "$OUT/report.json" --dump "$OUT/dump.csv" 2> "$OUT/time.txt" &
pid=$!
echo -e "t\tanon_swap_kb\tdisk_delta_b" > "$OUT/samples.tsv"
t0=$(date +%s)
while kill -0 $pid 2>/dev/null; do
    kb=$(for p in $pid $(pgrep -P $pid); do awk '/^RssAnon|^VmSwap/{s+=$2} END{print s+0}' /proc/$p/status 2>/dev/null; done | awk '{s+=$1} END{print s+0}')
    echo -e "$(( $(date +%s) - t0 ))\t${kb:-0}\t$(( $(used) - base ))" >> "$OUT/samples.tsv"
    sleep 2
done
wait $pid; echo "exit $?" >> "$OUT/time.txt"
