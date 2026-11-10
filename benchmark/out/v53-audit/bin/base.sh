#!/bin/bash
# img-fp for variant base: no kept matching is ever taken (the variants share one cache).
prev=""; for a in "$@"; do [ "$prev" = "--cache" ] && rm -f "$a.matched"; prev="$a"; done
exec "$(dirname "$0")/base" "$@"
