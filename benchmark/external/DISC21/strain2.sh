#!/bin/bash
# strain2.sh: dev queries --against zip 0, then zip 1, each alone, at
# --work-size 1024 from the w1024 cache; outputs in w1024_N/.
set -u
D=/home/daniel/Documents/DISC21
BIN=/home/daniel/Documents/Vscode_repositories/img-fp/target/plain/release/img-fp
echo $$ > "$D/strain2.pid"
echo 1000 > /proc/self/oom_score_adj
for n in 0 1; do
    OUT=$D/w1024_$n; mkdir -p "$OUT"
    echo "$(date '+%F %T') shard $n started" >> "$D/strain2.log"
    /usr/bin/time -v "$BIN" "$D/queries/images/queries" --against "$D/refs$n/images/references" \
        --work-size 1024 --min-pixel-correlation 0.5 -t 8 -v --cache "$D/cache/w1024.bin" --log-file "$OUT/run.log" \
        -o "$OUT/report.json" --dump "$OUT/dump.csv" 2> "$OUT/time.txt"
    st=$?; echo "exit $st" >> "$OUT/time.txt"
    echo "$(date '+%F %T') shard $n ended: exit $st, $(grep Elapsed "$OUT/time.txt" | awk '{print $NF}')" >> "$D/strain2.log"
done
echo "$(date '+%F %T') done" >> "$D/strain2.log"
