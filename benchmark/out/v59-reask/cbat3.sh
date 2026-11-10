#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask; echo 1000 > /proc/self/oom_score_adj
./mrun.sh repclean IMGFP_REASK=repclean
for c in found all all2; do ./crun.sh $c repclean IMGFP_REASK=repclean; done
