#!/bin/bash
# After score11: compress the scored cliff reports (32 GB filled the disk), then shard 0 again (curl resumes).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v56-seed
until grep -q "^scored" $O/score11.log 2>/dev/null; do sleep 30; done
for f in $O/cliffs/*.json; do [ -s ${f%.json}.txt ] && gzip -1 $f; done
echo "$(date +%T) cliffs compressed, $(df -h /home | tail -n 1 | awk '{print $4}') free" >> $O/run12.out
sed -i 's/^until grep -q " done\$" .*$//' $O/run12.sh
exec $O/run12.sh
