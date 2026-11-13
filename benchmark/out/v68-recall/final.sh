#!/bin/bash
# The final rule (new/img-fp, plain build) against HEAD (v67-keep/base/img-fp) on everything the keep rule is judged by.
cd /home/daniel/Documents/Vscode_repositories/img-fp
V=benchmark/out/v68-recall; O=benchmark/out/v67-keep; D=/home/daniel/Documents; C=$O/cache/c.db
{
for c in ALL2 ALL; do
  $V/new/img-fp $D/IMGS-$c -r -x '*' -t 8 -v --no-last --cache $C -o $V/new/$c.json > $V/new/$c.err 2>&1
  echo "== $c"; vendor/venv/bin/python $V/redundant.py $D/IMGS-$c/derived/manifest.csv $O/base/$c.json $V/new/$c.json | grep -v '^$'
  vendor/venv/bin/python benchmark/suggest_score.py $V/new/$c.json --manifest $D/IMGS-$c/derived/manifest.csv | cut -c1-200 | sed -n '1,5p;7p'
done
for w in base new new base; do
  b=$O/base/img-fp; [ $w = new ] && b=$V/new/img-fp
  $b $D/IMGS-ALL2 -r -x '*' -t 8 -v --no-last --cache $C -o $V/cost.json 2>&1 | grep -E 'pace: grouping' | sed "s/^/$w /"
done
vendor/venv/bin/python $V/small.py $O/base/img-fp s-head
vendor/venv/bin/python $V/small.py $V/new/img-fp new/small
echo "-- by the half-octave allowance (IMGFP_EFF=0.7)"
for c in ALL2 ALL; do IMGFP_EFF=0.7 vendor/venv/bin/python $V/redundant.py $D/IMGS-$c/derived/manifest.csv $O/base/$c.json $V/new/$c.json | grep precision; done
IMGFP_EFF=0.7 vendor/venv/bin/python $V/small.py $O/base/img-fp s-head
IMGFP_EFF=0.7 vendor/venv/bin/python $V/small.py $V/new/img-fp new/small
for t in dated same-time; do
  $V/new/img-fp $D/IMGS-PROVENANCE/$t -r -x '*' -t 8 --no-last --cache $C -o $V/new/prov-$t.json > /dev/null 2>&1
  vendor/venv/bin/python benchmark/score_provenance.py $V/new/prov-$t.json | head -8
done
$V/new/img-fp $D/IMGS-AGAINST/scan --against $D/IMGS-AGAINST/library --follow-symlinks -r -x '*' -t 8 --no-last --cache $C -o $V/new/against.json > /dev/null 2>&1
vendor/venv/bin/python benchmark/against_score.py $V/new/against.json | sed -n 4,13p
$V/new/img-fp /home/daniel/Downloads/archive -r -t 8 --no-last --cache $C -o $V/new/found.json > /dev/null 2>&1
python3 -c "
import json,collections
def acts(f):
    a={}
    for g in json.load(open(f))['groups']:
        for x in g['files']: a[x['path']]=x.get('action')
    return a
a,b=acts('$O/base/found.json'),acts('$V/new/found.json'); print('found corpus HEAD', dict(collections.Counter(a.values())), 'new', dict(collections.Counter(b.values())), 'moved', collections.Counter((a[p],b[p]) for p in a if a[p]!=b[p]).most_common())
print('found pairs identical', json.load(open('$O/base/found.json'))['pairs']==json.load(open('$V/new/found.json'))['pairs'])"
vendor/venv/bin/python benchmark/pairs.py run $V/new/img-fp $V/new/pairs --cache $C > $V/new/pairs.log 2>&1
vendor/venv/bin/python benchmark/pairs.py compare $O/base/pairs $V/new/pairs | awk 'NR<3 || ($2!=$7 || $3!=$8 || $4!=$9 || $5!=$10)'
echo "-- two-file families by the half-octave allowance"
IMGFP_EFF=0.7 vendor/venv/bin/python benchmark/pairs.py compare $O/base/pairs $V/new/pairs | awk 'NR<3 || $5>0 || $10>0 || $1=="TOTAL"'
for c in ALL2 ALL; do vendor/venv/bin/python $O/noupscale.py $V/new/$c.json $D/IMGS-$c/derived/manifest.csv; done
vendor/venv/bin/python benchmark/out/v69-bar/editcopies.py run $V/new/img-fp final
vendor/venv/bin/python benchmark/out/v69-bar/editcopies.py score head final
echo FINISHED
} > $V/final.out 2>&1
