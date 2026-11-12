#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
./run.sh f1-w15 img-fp-f1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=15 --
./run.sh f1-w3 img-fp-f1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 --
./run.sh f1-w3-s10 img-fp-f1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 IMGFP_W0=10 --
./sc.sh f1-w15 f1-w3 f1-w3-s10 > go2.sc
