#!/bin/bash
# runv.sh NAME BINARY [ENV=VAL...]: families.py subsets + full IMGS5 under NAME/
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
name=$1; bin=$2; shift 2
env "$@" python3 ../../families.py run $bin $name --cache cache.bin
ls /home/daniel/Documents/IMGS5 > /dev/null
env "$@" $bin -r -x '*' /home/daniel/Documents/IMGS5 --cache cache.bin --no-last -o $name/i5-full.json 2> $name/i5-full.log
echo done $name
