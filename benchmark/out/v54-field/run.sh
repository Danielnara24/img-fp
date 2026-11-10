#!/bin/bash
# The field on IMGS-ALL (five corpora) and IMGS-ALL2 (hard), img-fp 0.75.0
# (plain build), every tool cold, alone, evicted, cooled. The tools that may
# run the machine out of memory go first, on both corpora, while the machine
# is freshest; bench.py makes each tool the OOM killer's first choice.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
PY=../vendor/venv/bin/python; O=out/v54-field
ALL=/home/daniel/Documents/IMGS-ALL; ALL2=/home/daniel/Documents/IMGS-ALL2
RISKY=difpy,imagededup_cnn,pdq,sscd,czkawka_tuned,imagededup_phash
REST=dupeguru,findimagedupes,imgdupes_phash,czkawka_default,fclones,imgfp
echo "=== idle $(date)"; free -m
sleep 300
IDLE=$($PY -c "
import time
s=[]
for _ in range(10):
    s.append(int(open('/sys/class/hwmon/hwmon3/temp1_input').read())/1000); time.sleep(1.5)
print(round(sum(s)/len(s),1))")
CEIL=$($PY -c "print($IDLE+3)")
echo "idle $IDLE C, ceiling $CEIL C"
for phase in RISKY REST; do
  for c in all:$ALL all2:$ALL2; do
    t=${c%%:*}; d=${c#*:}
    echo "=== $phase $t $(date)"; free -m | sed -n 2,3p
    $PY bench.py --corpus $d --out $O/$t --temp-ceiling $CEIL --timeout 6 --only ${!phase}
  done
done
echo "=== score $(date)"
for c in all:$ALL all2:$ALL2; do
  t=${c%%:*}; d=${c#*:}
  ( cd $O/$t && ls *.json | grep -v metrics > tools.txt )
  $PY score.py --corpus $d --per-transform --csv $O/$t/per_transform.csv $(sed "s|^|$O/$t/|" $O/$t/tools.txt) > $O/$t/score.txt 2>&1
  $PY analyse.py --corpus=$d --per-transform $O/$t/imgfp.json > $O/$t/analyse.txt 2>&1
  $PY suggest_score.py $O/$t/imgfp.json --manifest $d/derived/manifest.csv > $O/$t/suggest.txt 2>&1
done
echo "=== done $(date)"
