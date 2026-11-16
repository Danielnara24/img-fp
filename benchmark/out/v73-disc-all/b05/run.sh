#!/bin/bash
# shard 0 at --min-pixel-correlation 0.5, same build and command as loop.sh, on a fresh copy of cache57
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v73-disc-all; D=/home/daniel/Documents/DISC21
/usr/bin/time -v "$O/img-fp" "$D/queries/images/queries" --against "$D/refs0/images/references" \
  -t 8 -v --no-last --cache "$O/b05/cache/c.db" --min-pixel-correlation 0.5 --format json -o "$O/b05/shard0.json" 2> "$O/b05/shard0.err"
echo "exit $?" > "$O/b05/done"
