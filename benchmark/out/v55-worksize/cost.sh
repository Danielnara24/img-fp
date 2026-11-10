#!/bin/bash
# Cost of tying features to work size: IMGS-ALL2 cold at 1024, ABBA.
# A = img-fp-tie (600 features, tied bound), B = img-fp-both (1200).
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v55-worksize; C=/home/daniel/Documents/IMGS-ALL2
echo $$ > $O/cost.pid
temp() { awk '{print $1/1000}' /sys/class/hwmon/hwmon3/temp1_input; }
sleep 120; idle=$(temp); echo "$(date +%T) idle $idle C" >> $O/cost.out
one() { local tag=$1 bin=$2
    sleep 60; while [ "$(awk -v t=$(temp) -v i=$idle 'BEGIN{print (t>i+3)}')" = 1 ]; do sleep 10; done
    python3 $O/evict.py $C
    echo "$(date +%T) start $tag at $(temp) C" >> $O/cost.out
    /usr/bin/time -v $O/$bin "$C" -r -x '*' -t 8 --no-cache --no-last --work-size 1024 -o $O/$tag.json 2> $O/$tag.time
    echo "$(date +%T) end $tag exit $?: $(grep -E 'User time|System time|Elapsed' $O/$tag.time | awk '{print $NF}' | tr '\n' ' ')" >> $O/cost.out; }
one costA1 img-fp-tie; one costB1 img-fp-both; one costB2 img-fp-both; one costA2 img-fp-tie
echo "$(date +%T) cost done" >> $O/cost.out
