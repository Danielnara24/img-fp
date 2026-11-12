#!/bin/bash
# g4: fill a fresh cache on IMGS-ALL2, then cached default and --detail 256, scored
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65; mkdir -p /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad5
until [ -f img-fp-g4 ]; do sleep 10; done; sleep 5
./run.sh g4-fill img-fp-g4 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad5/l.db --
./run.sh g4 img-fp-g4 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad5/l.db --
./run.sh g4-d256fill img-fp-g4 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad5/l.db -- --detail 256
./run.sh g4-d256 img-fp-g4 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/lad5/l.db -- --detail 256
rm -f out/g4-fill.json out/g4-d256fill.json
./sc.sh g4 g4-d256 > go8.sc 2>&1
echo done > go8.done
