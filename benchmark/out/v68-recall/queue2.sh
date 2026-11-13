#!/bin/bash
V=benchmark/out/v68-recall; cd /home/daniel/Documents/Vscode_repositories/img-fp
B="IMGFP_MIRROR=1 IMGFP_TONE=1 IMGFP_MID=1"
$V/battery.sh $V/bin-f f12 $B
NOPAIRS=1 $V/battery.sh $V/bin-f f-same20 $B IMGFP_WR_SAME=20
NOPAIRS=1 $V/battery.sh $V/bin-f f-same32 $B IMGFP_WR_SAME=32
NOPAIRS=1 $V/battery.sh $V/bin-f f-same48 $B IMGFP_WR_SAME=48
echo all > $V/queue2.done
