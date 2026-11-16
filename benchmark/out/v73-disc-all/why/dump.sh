#!/bin/bash
# shard 0 at bar 0.5 (as b05) with --dump: every verdict the matcher computed
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v73-disc-all; D=/home/daniel/Documents/DISC21
/usr/bin/time -v "$O/img-fp" "$D/queries/images/queries" --against "$D/refs0/images/references" \
  -t 8 -v --no-last --cache "$O/b05/cache/c.db" --min-pixel-correlation 0.5 --format json -o "$O/why/shard0.json" --dump "$O/why/dump.csv" 2> "$O/why/dump.err"
echo "exit $?" > "$O/why/dump.done"
