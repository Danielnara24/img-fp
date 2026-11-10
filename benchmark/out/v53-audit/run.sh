#!/bin/bash
# Audit A (vocabulary twins) and B (damaged files): variants on IMGS-ALL,
# IMGS-ALL2 and the small families, one shared analysis cache.
#   base   = 0.74.0
#   off    = new build, no switch (B and C only)
#   floor  = new build, IMGFP_WORD_R=143 (leaves of one parent within r are one word)
#   sample = new build, IMGFP_SAMPLE_R=143 (deepest-level k-means trains on no two members within r)
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
O=out/v53-audit; C=$O/cache.bin; PY=../vendor/venv/bin/python
ALL=/home/daniel/Documents/IMGS-ALL; ALL2=/home/daniel/Documents/IMGS-ALL2
temp() { echo $(( $(cat /sys/class/hwmon/hwmon3/temp1_input) / 1000 )); }
mhz() { awk '/MHz/{s+=$4;n++} END{printf "%d", s/n}' /proc/cpuinfo; }
IDLE=$(temp)
cool() { sleep 60; for i in $(seq 120); do [ $(temp) -le $((IDLE+3)) ] && return; sleep 5; done; }
run() { # run TAG VARIANT CORPUS
  local t0=$(temp); local m0=$(mhz)
  /usr/bin/time -f "%e %U %S %M" -o $O/$1.time $O/bin/$2.sh "$3" -r -x '*' --cache $C --no-last -o $O/$1.json -t 8 > $O/$1.log 2>&1
  echo "$1 $2 exit=$? $(cat $O/$1.time) temp0=$t0 temp1=$(temp) mhz0=$m0 mhz1=$(mhz)" | tee -a $O/times.txt
}
echo "=== start $(date) idle $IDLE C"
run all-fill off $ALL
cool; run all-base1 base $ALL
cool; run all-floor1 floor $ALL
cool; run all-sample1 sample $ALL
cool; run all-sample2 sample $ALL
cool; run all-floor2 floor $ALL
cool; run all-base2 base $ALL
echo "=== all2 $(date)"
run all2-fill off $ALL2
cool; run all2-base base $ALL2
cool; run all2-floor floor $ALL2
cool; run all2-sample sample $ALL2
echo "=== families $(date)"
for v in base floor sample; do
  $PY families.py run $O/bin/$v.sh $O/fam-$v --cache $C --corpus all all2 > $O/fam-$v.out 2>&1
  $PY families.py score $O/fam-$v > $O/fam-$v/score.out 2>&1
done
echo "=== score $(date)"
for t in all-fill all-base1 all-floor1 all-sample1; do
  $PY analyse.py --corpus=$ALL --per-transform $O/$t.json > $O/$t.analyse 2>&1
  $PY suggest_score.py $O/$t.json --manifest $ALL/derived/manifest.csv > $O/$t.suggest 2>&1
done
for t in all2-fill all2-base all2-floor all2-sample; do
  $PY analyse.py --corpus=$ALL2 --per-transform $O/$t.json > $O/$t.analyse 2>&1
  $PY suggest_score.py $O/$t.json --manifest $ALL2/derived/manifest.csv > $O/$t.suggest 2>&1
done
echo "=== done $(date)"
