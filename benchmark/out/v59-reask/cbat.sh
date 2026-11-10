#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask; echo 1000 > /proc/self/oom_score_adj
for c in all all2; do ./crun.sh $c cur; ./crun.sh $c rep IMGFP_REASK=rep; ./crun.sh $c all IMGFP_REASK=all; done
