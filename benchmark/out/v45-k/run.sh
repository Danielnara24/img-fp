#!/bin/bash
# run TAG [img-fp args...]: cached run on v40-keep's cache, report TAG.json, log TAG.log
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v45-k
C=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v40-keep/cache.bin
tag=$1; shift
[ -s "$O/$tag.json" ] && exit 0
/usr/bin/time -f "%e s wall %U+%S cpu %M KB" "$O/img-fp" --cache "$C" --no-last "$@" -o "$O/$tag.json" > "$O/$tag.log" 2>&1
