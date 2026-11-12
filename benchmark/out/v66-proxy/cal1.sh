#!/bin/bash
cd "$(dirname "$0")"; H=/home/daniel/Documents/DISC21/cache57/c.db
./one.sh p-head head $H proxy
./one.sh p-k50 head $H proxy -k 50
./one.sh p-nomirror head $H proxy --skip-mirrored
mkdir -p ch3; ./one.sh p-h3 h3 ch3/c.db proxy
./one.sh s-head head $H shard
./one.sh s-k50 head $H shard -k 50
./one.sh s-nomirror head $H shard --skip-mirrored
echo DONE >> log
