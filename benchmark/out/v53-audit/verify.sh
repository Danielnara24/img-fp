#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark
O=out/v53-audit; C=$O/cache.bin; PY=../vendor/venv/bin/python
for c in all:/home/daniel/Documents/IMGS-ALL all2:/home/daniel/Documents/IMGS-ALL2; do
  t=${c%%:*}; d=${c#*:}
  $O/bin/final.sh "$d" -r -x '*' --cache $C --no-last -o $O/$t-final.json -t 8 > $O/$t-final.log 2>&1
done
$PY families.py run $O/bin/final.sh $O/fam-final --cache $C --corpus all all2 > $O/fam-final.out 2>&1
$PY families.py score $O/fam-final > $O/fam-final/score.out 2>&1
echo "=== done $(date)" >> $O/verify.done
