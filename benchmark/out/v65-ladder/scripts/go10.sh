#!/bin/bash
# whole IMGS-ALL2 cold, the other order (G head head G), five minutes' rest before each
cd "$(dirname "$0")"; mkdir -p baab; p=/home/daniel/Documents/IMGS-ALL2
temp() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat $h/name)" = k10temp ] && echo $(( $(cat $h/temp1_input) / 1000 )); done; }
i=0
for b in g5 head head g5 head g5 g5 head; do i=$((i+1))
  python3 -I evict.py $p; sleep 300; t0=$(temp)
  /usr/bin/time -v ./img-fp-$b $p -r -x '*' -t 8 -v --no-cache --no-last -o /dev/null 2> baab/$i-$b.err
  echo "$(date +%T) $i $b start ${t0}C end $(temp)C $(grep -E 'User time|Elapsed' baab/$i-$b.err | awk '{print $NF}' | tr '\n' ' ') $(grep -o 'cpu: decode.*' baab/$i-$b.err)" >> baab.log
done
echo done > go10.done
