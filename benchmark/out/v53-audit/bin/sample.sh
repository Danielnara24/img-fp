#!/bin/bash
# img-fp for variant sample: no kept matching is ever taken (the variants share one cache).
prev=""; for a in "$@"; do [ "$prev" = "--cache" ] && rm -f "$a.matched"; prev="$a"; done
IMGFP_SAMPLE_R=143 exec "$(dirname "$0")/new" "$@"
