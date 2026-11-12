#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
./run.sh e1-cold img-fp-e1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 --
./run.sh e1-w3 img-fp-e1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=3 --
./run.sh e1-w15 img-fp-e1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db IMGFP_W512=15 --
./run.sh e1-d256 img-fp-e1 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad3/l.db -- --detail 256
