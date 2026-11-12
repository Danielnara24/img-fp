#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
./run.sh f3-voc img-fp-f3 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 IMGFP_VOC512=1 --
./sc.sh f3-voc > go5.sc; rm out/f3-voc.json
./run.sh f3 img-fp-f3 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 --
./sc.sh f3 >> go5.sc; rm out/f3.json
./run.sh f3-voc-s10 img-fp-f3 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 IMGFP_VOC512=1 IMGFP_W0=10 --
./sc.sh f3-voc-s10 >> go5.sc; rm out/f3-voc-s10.json
echo done >> go5.sc
