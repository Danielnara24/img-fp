#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57
B=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark
PY=$B/../vendor/venv/bin/python
rm -f c.db.matched
./img-fp-new ~/Documents/IMGS-ALL -r -x '*' -t 8 -v --no-last --cache c.db -o all-new.json 2> all-new.err; rm -f c.db.matched
cd $B
$PY analyse.py /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/all-new.json --corpus=/home/daniel/Documents/IMGS-ALL --per-transform > /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/scoreall-new.txt 2>&1; echo all-new; sed -n 3,6p /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/scoreall-new.txt
for r in all-basec all-new; do echo "keep $r"; $PY suggest_score.py /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/$r.json --manifest /home/daniel/Documents/IMGS-ALL/derived/manifest.csv 2>&1 | head -30; done
for r in all2-basec all2b-B1; do echo "keep $r"; $PY suggest_score.py /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/$r.json --manifest /home/daniel/Documents/IMGS-ALL2/derived/manifest.csv 2>&1 | head -30; done
$PY families.py run /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/img-fp-new /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/fam-new --cache /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/c.db --corpus all i5 all2 hdev > /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/fam-new.out 2>&1; rm -f /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/c.db.matched
$PY families.py compare /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/fam-base /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/fam-cr1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/fam-new
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57; python3 $B/out/v56-seed/invariance.py fam-base fam-new
for d in 512 1024; do ./img-fp-new ~/Documents/IMGS -r -x '*' -t 8 --no-last --cache c.db --detail $d -o imgs-$d.json 2> imgs-$d.err; rm -f c.db.matched; done
python3 - <<'PY'
import json,csv
rel={}
for r in csv.DictReader(open('/home/daniel/Documents/IMGS/derived/pairs.csv')):
    rel[tuple(sorted((r['a'],r['b'])))]=r['relation']
def tp(p):
    d=json.load(open(p)); s=set()
    for x in d['pairs']:
        k=tuple(sorted((x['a'],x['b'])))
        if rel.get(k) in ('SAME','CROP'): s.add(k)
    return s
a,b=tp('imgs-512.json'),tp('imgs-1024.json')
print('containment IMGS 512 -> 1024: true pairs', len(a), len(b), 'lost', len(a-b), 'added', len(b-a))
PY
