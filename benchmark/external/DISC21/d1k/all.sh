#!/bin/bash
# d1k/all.sh: DISC21 at --detail 1024. Shards 0 then 1 (shared cache, as the
# full run shares it), then 0 + 1 in one run (own cold cache), then shards
# 2..19 through loop.sh (downloads two ahead), each into d1k/shardN.
set -u
D=/home/daniel/Documents/DISC21; K=$D/d1k
echo 1000 > /proc/self/oom_score_adj
log() { echo "$(date '+%F %T') $*" >> "$K/loop.log"; }
for n in 0 1; do
    log "shard $n started"; "$K/shard.sh" $n
    log "shard $n ended: $(tail -1 $K/shard$n/time.txt), $(grep Elapsed $K/shard$n/time.txt | awk '{print $NF}')"
done
log "both started"; TAG=both CACHE=$K/both.bin "$K/shard.sh" 0 1
log "both ended: $(tail -1 $K/both/time.txt), $(grep Elapsed $K/both/time.txt | awk '{print $NF}')"
rm -f $K/both.bin $K/both.bin.*
grep -q "exit [02]" $K/shard0/time.txt && grep -q "exit [02]" $K/shard1/time.txt && rm -rf $D/refs0/images $D/refs1/images && log "deleted refs0, refs1 images"
"$K/loop.sh" 2 19
log "all done"
