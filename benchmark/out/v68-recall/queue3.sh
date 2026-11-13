#!/bin/bash
V=benchmark/out/v68-recall; cd /home/daniel/Documents/Vscode_repositories/img-fp
$V/battery.sh $V/bin-h h-composite IMGFP_COMPOSITE=1
echo all > $V/queue3.done
