#!/bin/bash
# disc.sh NAME BINARY [ENV...]: shard 0 at bar 0.5 (as v73 b05) on b05's cache
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out; D=/home/daniel/Documents/DISC21
name=$1; bin=$2; shift 2
env "$@" /usr/bin/time -v "$O/v74-reach/$bin" "$D/queries/images/queries" --against "$D/refs0/images/references" \
  -t 8 -v --no-last --cache "$O/v73-disc-all/b05/cache/c.db" --min-pixel-correlation 0.5 --format json -o "$O/v74-reach/d-$name.json" 2> "$O/v74-reach/d-$name.err"
echo "exit $?" > "$O/v74-reach/d-$name.done"
