#!/bin/bash
O=$(cd "$(dirname "$0")" && pwd)
PY=/home/daniel/Documents/Vscode_repositories/img-fp/vendor/venv/bin/python
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
$PY $B/pairs.py run "$O/img-fp-final" "$O/pairs-final" --cache "$O/../v40-keep/cache.bin" > "$O/pairs-run.log" 2>&1
$PY $B/pairs.py compare "$B/out/v39-audit/pairs-new" "$O/pairs-final" > "$O/pairs-compare.txt" 2>&1
echo pairs-done >> "$O/progress.txt"
