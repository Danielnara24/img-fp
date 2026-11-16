#!/bin/bash
# shard 0 at --min-pixel-correlation 0.3, as loop.sh, on b05's cache (cache57 copied, used once)
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v73-disc-all; D=/home/daniel/Documents/DISC21
/usr/bin/time -v "$O/img-fp" "$D/queries/images/queries" --against "$D/refs0/images/references" \
  -t 8 -v --no-last --cache "$O/b05/cache/c.db" --min-pixel-correlation 0.3 --format json -o "$O/b03/shard0.json" 2> "$O/b03/shard0.err"
echo "exit $?" > "$O/b03/done"
