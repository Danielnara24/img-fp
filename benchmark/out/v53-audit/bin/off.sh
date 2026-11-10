#!/bin/bash
# the new build with no switch: B and C only.
prev=""; for a in "$@"; do [ "$prev" = "--cache" ] && rm -f "$a.matched"; prev="$a"; done
exec "$(dirname "$0")/new" "$@"
