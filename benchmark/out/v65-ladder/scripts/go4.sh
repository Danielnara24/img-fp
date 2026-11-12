#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
./run.sh f2-voc img-fp-f2 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 IMGFP_VOC512=1 --
./sc.sh f2-voc > go4.sc; rm out/f2-voc.json
./run.sh f2-voc-drop img-fp-f2 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 IMGFP_VOC512=1 IMGFP_DEMOTE=0 IMGFP_SINGLE=0 --
./sc.sh f2-voc-drop >> go4.sc; rm out/f2-voc-drop.json
echo done >> go4.sc
