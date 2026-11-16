#!/bin/bash
# each missed true copy scanned alone (the two files in a folder), bar 0.5, with --dump
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v73-disc-all; D=/home/daniel/Documents/DISC21
mkdir -p "$O/why/alone"
while read q r; do
  d="$O/why/alone/$q"; mkdir -p "$d/f"
  ln -f "$D/queries/images/queries/$q.jpg" "$D/refs0/images/references/$r.jpg" "$d/f/"
  "$O/img-fp" "$d/f" -t 2 --no-last --cache "$O/b05/cache/c.db" --min-pixel-correlation 0.5 \
    --format json -o "$d/out.json" --dump "$d/dump.csv" 2> "$d/err"
done < "$O/why/missed.txt"
echo done > "$O/why/alone.done"
