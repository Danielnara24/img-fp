#!/bin/bash
# Continues run.sh: waits for its IMGS-ALL risky bench.py (4047659) to finish,
# then the rest. difPy and SSCD are not run on IMGS-ALL2 tonight (user's
# decision, to finish in the day: 5.2 h and ~3 h each on IMGS-ALL); they
# are run later into the same out/v54-field/all2.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
PY=../vendor/venv/bin/python; O=out/v54-field
ALL=/home/daniel/Documents/IMGS-ALL; ALL2=/home/daniel/Documents/IMGS-ALL2
CEIL=81.8
while kill -0 4047659 2>/dev/null; do sleep 30; done
RISKY2=imagededup_cnn,pdq,czkawka_tuned,imagededup_phash
REST=dupeguru,findimagedupes,imgdupes_phash,czkawka_default,fclones,imgfp
echo "=== RISKY all2 ($RISKY2) $(date)"; free -m | sed -n 2,3p
$PY bench.py --corpus $ALL2 --out $O/all2 --temp-ceiling $CEIL --timeout 6 --only $RISKY2
for c in all:$ALL all2:$ALL2; do
  t=${c%%:*}; d=${c#*:}
  echo "=== REST $t $(date)"; free -m | sed -n 2,3p
  $PY bench.py --corpus $d --out $O/$t --temp-ceiling $CEIL --timeout 6 --only $REST
done
echo "=== score $(date)"
for c in all:$ALL all2:$ALL2; do
  t=${c%%:*}; d=${c#*:}
  ( cd $O/$t && ls *.json | grep -v metrics | grep -v run_meta > tools.txt )
  $PY score.py --corpus $d --per-transform --csv $O/$t/per_transform.csv $(sed "s|^|$O/$t/|" $O/$t/tools.txt) > $O/$t/score.txt 2>&1
  $PY analyse.py --corpus=$d --per-transform $O/$t/imgfp.json > $O/$t/analyse.txt 2>&1
  $PY suggest_score.py $O/$t/imgfp.json --manifest $d/derived/manifest.csv > $O/$t/suggest.txt 2>&1
done
echo "=== done $(date)"
