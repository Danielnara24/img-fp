#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
./ab.sh g3-512 4 /home/daniel/Documents/IMGS-ALL2/IMGS5 "img-fp-head" "img-fp-g3" > ab-g3-512.txt 2>&1
./ab.sh g3-256 4 /home/daniel/Documents/IMGS-ALL2/IMGS5 "img-fp-head --fast" "img-fp-g3 --detail 256" > ab-g3-256.txt 2>&1
./ab.sh g3-512-s3 3 /home/daniel/Documents/IMGS-ALL2/IMGS3 "img-fp-head" "img-fp-g3" > ab-g3-512-s3.txt 2>&1
echo done > go7.done
