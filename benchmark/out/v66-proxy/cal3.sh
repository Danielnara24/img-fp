#!/bin/bash
cd "$(dirname "$0")"; H=/home/daniel/Documents/DISC21/cache57/c.db
./one.sh q-head head $H p2
./one.sh q-k50 head $H p2 -k 50
./one.sh q-nomirror head $H p2 --skip-mirrored
./one.sh q-h1 h1 ch1/c.db p2
./one.sh q-h3 h3 ch3/c.db p2
echo DONE3 >> log
