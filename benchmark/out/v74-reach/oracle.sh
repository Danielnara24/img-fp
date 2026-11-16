#!/bin/bash
# shard 0: the oracle scaffolding on the 143 missed copies (exits after the index is built)
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out; D=/home/daniel/Documents/DISC21
BIN=${1:-img-fp-oracle}; TAG=${2:-oracle}; shift 2
env "$@" IMGFP_ORACLE=1 IMGFP_TRACE="$O/v74-reach/trace.pairs" /usr/bin/time -v "$O/v74-reach/$BIN" "$D/queries/images/queries" --against "$D/refs0/images/references" \
  -t 8 -v --no-last --cache "$O/v73-disc-all/b05/cache/c.db" --min-pixel-correlation 0.5 --format json -o "$O/v74-reach/$TAG.json" 2> "$O/v74-reach/$TAG.err"
echo "exit $?" > "$O/v74-reach/$TAG.done"
