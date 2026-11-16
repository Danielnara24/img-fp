#!/bin/bash
# shard 0 at bar 0.5 with the trace scaffolding on the 143 missed copies
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out; D=/home/daniel/Documents/DISC21
IMGFP_TRACE="$O/v74-reach/trace.pairs" /usr/bin/time -v "$O/v74-reach/img-fp-trace" "$D/queries/images/queries" --against "$D/refs0/images/references" \
  -t 8 -v --no-last --cache "$O/v73-disc-all/b05/cache/c.db" --min-pixel-correlation 0.5 --format json -o "$O/v74-reach/trace.json" 2> "$O/v74-reach/trace.err"
echo "exit $?" > "$O/v74-reach/trace.done"
