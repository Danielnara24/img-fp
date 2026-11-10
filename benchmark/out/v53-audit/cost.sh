#!/bin/bash
# Matching cost on the found mix (IMGS-ALL's 364 originals among the 9,285 found photos), cached analysis.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
O=out/v53-audit; C=$O/cache.bin
temp() { echo $(( $(cat /sys/class/hwmon/hwmon3/temp1_input) / 1000 )); }
mhz() { awk '/MHz/{s+=$4;n++} END{printf "%d", s/n}' /proc/cpuinfo; }
IDLE=$(temp)
cool() { sleep 60; for i in $(seq 120); do [ $(temp) -le $((IDLE+3)) ] && return; sleep 5; done; }
for v in base floor sample sample floor base; do
  cool; t0=$(temp); m0=$(mhz)
  /usr/bin/time -f "%e %U %S %M" -o $O/cost.time $O/bin/$v.sh --from-file $O/fam-base/all-f.list -x '*' --cache $C --no-last -o $O/cost-$v.json -t 8 > $O/cost-$v.log 2>&1
  echo "$v $(tail -1 $O/cost.time) temp0=$t0 temp1=$(temp) mhz0=$m0 mhz1=$(mhz)" | tee -a $O/cost.txt
done
echo "=== done $(date)" >> $O/cost.txt
