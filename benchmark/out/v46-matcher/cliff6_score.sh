#!/bin/bash
# analyse.py on every stack cliff step (and the defaults), then the table against 0.72.0 (v44-flat-contrast).
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v46-matcher
P=../../../vendor/venv/bin/python
mkdir -p cliff6
an() { if [ $1 = i5 ]; then $P ../../analyse.py --corpus=/home/daniel/Documents/IMGS5 $2; else $P ../../analyse.py $2; fi; }
for t in i5 all; do
  an $t p-on3/$t-full.json > cliff6/$t-default.txt 2>&1
  for s in points8 points6 points5 points4 points3 overlap0.75 overlap0.65 overlap0.6 overlap0.5 overlap0.4 overlap0.3 overlap0.25 corr0.9 corr0.8 corr0.6 corr0.5 corr0.4 corr0.3 corr0.2 k100 k50 k25 k10; do
    case $s in points*) f=minalignedpoints${s#points};; overlap*) f=minframeoverlap${s#overlap};; corr*) f=minpixelcorrelation${s#corr};; k*) f=$s;; esac
    an $t cliff-on3/$t-$f.json > cliff6/$t-$s.txt 2>&1
  done
done
an i5 cliff-on3/check-0720-i5-overlap0.6.json > cliff6/check-0720-i5-overlap0.6.txt 2>&1
$P cliff6_table.py > cliff6_table.out
echo scored
