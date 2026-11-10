#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask; echo 1000 > /proc/self/oom_score_adj
./mrun.sh repfam IMGFP_REASK=repfam
for c in found all all2; do ./crun.sh $c repfam IMGFP_REASK=repfam; done
cd ../v58-reach; ./abba.sh rfall2 ABBA "IMGFP_REASK=" "IMGFP_REASK=repfam" img-fp-re img-fp-re /home/daniel/Documents/IMGS-ALL2 -x '*' -r -t 8 -v
