#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
for p in IMGS IMGS2 IMGS4; do ./ab.sh g5-$p 2 /home/daniel/Documents/IMGS-ALL2/$p "img-fp-head" "img-fp-g5" > ab-g5-$p.txt 2>&1; done
echo done > go9.done
