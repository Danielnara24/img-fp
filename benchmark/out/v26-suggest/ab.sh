#!/bin/bash
# Cold A/B of 0.33.1 (old) against the build with the keep/delete suggestion
# (new): bench.py's protocol, one run per invocation, in the order old new new old.
cd /home/daniel/Documents/Vscode_repositories/img-fp
export CLAUDE_CODE_DISABLE_BG_SHELL_PRESSURE_REAP=1
OUT=benchmark/out/v26-suggest
OLD=target/headplain/release/img-fp
NEW=target/plain/release/img-fp
for corpus in found all; do
  case $corpus in found) C=/home/daniel/Downloads/archive;; all) C=/home/daniel/Documents/IMGS-ALL;; esac
  i=0
  for which in old new new old; do
    i=$((i+1))
    bin=$OLD; [ $which = new ] && bin=$NEW
    python3 benchmark/bench.py --corpus $C --only imgfp --imgfp-bin $bin --out $OUT/${corpus}_${i}_${which}
  done
done
