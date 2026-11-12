#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
r() { n=$1; shift; ./run.sh $n img-fp-f3 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 IMGFP_VOC512=1 "$@" --; ./sc.sh $n >> go6.sc; rm out/$n.json; }
r f3v-drop IMGFP_DEMOTE=0
r f3v-drop-nosingle IMGFP_DEMOTE=0 IMGFP_SINGLE=0
r f3v-lone IMGFP_DEMOTE=2
r f3v-w15 IMGFP_W512=15
echo done >> go6.sc
