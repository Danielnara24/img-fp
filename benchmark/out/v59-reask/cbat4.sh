#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask; echo 1000 > /proc/self/oom_score_adj
./mrun.sh repclean2 IMGFP_REASK=repclean2
for c in found all all2; do ./crun.sh $c repclean2 IMGFP_REASK=repclean2; done
cd ../v58-reach; ./abba.sh rc2all2 ABBA "IMGFP_REASK=" "IMGFP_REASK=repclean2" img-fp-re img-fp-re /home/daniel/Documents/IMGS-ALL2 -x '*' -r -t 8 -v
