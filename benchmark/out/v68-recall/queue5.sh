#!/bin/bash
V=benchmark/out/v68-recall; cd /home/daniel/Documents/Vscode_repositories/img-fp
$V/battery.sh $V/bin-k k-composite IMGFP_LATE=1
echo all > $V/queue5.done
