#!/bin/bash
prev=""; for a in "$@"; do [ "$prev" = "--cache" ] && rm -f "$a.matched"; prev="$a"; done
exec "$(dirname "$0")/final" "$@"
