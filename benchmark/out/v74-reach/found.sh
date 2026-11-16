#!/bin/bash
# found.sh NAME BINARY [ENV...]: the found corpus, cached (found.db), defaults
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v74-reach
name=$1; bin=$2; shift 2
env "$@" /usr/bin/time -v "$O/$bin" /home/daniel/Downloads/archive -r -t 8 -v --no-last --cache "$O/found.db" --format json -o "$O/f-$name.json" 2> "$O/f-$name.err"
echo "exit $?" > "$O/f-$name.done"
