#!/bin/bash
# SSCD on IMGS-ALL2 (left out of the 2026-10-28 run), then all2 rescored.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
PY=../vendor/venv/bin/python; O=out/v54-field; ALL2=/home/daniel/Documents/IMGS-ALL2
sleep 180
IDLE=$($PY -c "
import time
s=[]
for _ in range(10):
    s.append(int(open('/sys/class/hwmon/hwmon3/temp1_input').read())/1000); time.sleep(1.5)
print(round(sum(s)/len(s),1))")
echo "idle $IDLE C $(date)"; free -m | sed -n 2,3p
$PY bench.py --corpus $ALL2 --out $O/all2 --temp-ceiling $($PY -c "print($IDLE+3)") --timeout 6 --only sscd
( cd $O/all2 && ls *.json | grep -v metrics | grep -v run_meta > tools.txt )
$PY score.py --corpus $ALL2 --per-transform --csv $O/all2/per_transform.csv $(sed "s|^|$O/all2/|" $O/all2/tools.txt) > $O/all2/score.txt 2>&1
echo "=== done $(date)"
