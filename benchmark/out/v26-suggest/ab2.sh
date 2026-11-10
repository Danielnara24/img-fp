#!/bin/bash
# The found corpus again, in the reverse order: new old old new.
cd /home/daniel/Documents/Vscode_repositories/img-fp
export CLAUDE_CODE_DISABLE_BG_SHELL_PRESSURE_REAP=1
OUT=benchmark/out/v26-suggest
i=4
for which in new old old new; do
  i=$((i+1))
  bin=target/headplain/release/img-fp; [ $which = new ] && bin=target/plain/release/img-fp
  python3 benchmark/bench.py --corpus /home/daniel/Downloads/archive --only imgfp --imgfp-bin $bin --out $OUT/found_${i}_${which}
done
