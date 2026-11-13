#!/bin/bash
V=benchmark/out/v68-recall; cd /home/daniel/Documents/Vscode_repositories/img-fp
$V/battery.sh $V/bin-e e12 IMGFP_MIRROR=1 IMGFP_TONE=1 IMGFP_MID=1
NOPAIRS=1 $V/battery.sh $V/bin-e e12-notone IMGFP_MIRROR=1 IMGFP_MID=1
$V/battery.sh $V/bin-e e16 IMGFP_MIRROR=1 IMGFP_TONE=1 IMGFP_MID=1 IMGFP_WR_IN=16 IMGFP_WR_SAME=16
$V/battery.sh $V/bin-e e24 IMGFP_MIRROR=1 IMGFP_TONE=1 IMGFP_MID=1 IMGFP_WR_IN=28 IMGFP_WR_SAME=24
echo all > $V/queue.done
