#!/bin/bash
V=benchmark/out/v68-recall; cd /home/daniel/Documents/Vscode_repositories/img-fp
$V/battery.sh $V/bin-j j
$V/battery.sh $V/bin-j j-late IMGFP_LATE=1
echo all > $V/queue4.done
