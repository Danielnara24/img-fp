#!/bin/bash
# --detail containment from one cache: 512 cold into it, then 1024 read from it (ship build).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed; C=$O/trace/a2.bin; D=/home/daniel/Documents
say() { echo "$(date +%T) $*" >> $O/trace/run.out; }
for d in 512 1024; do rm -f $C.matched; say "start $d"
  $O/img-fp-ship $D/IMGS-ALL2 -r -x '*' -t 8 --no-last --cache $C --detail $d -o $O/trace/ship-$d.json 2> $O/trace/ship-$d.err
  say "end $d exit $?"; done
say done
