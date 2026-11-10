#!/bin/bash
# The battery for the geometric look at depth 1,000 (img-fp-ship, the clean build) against
# HEAD's outputs of the v57 battery (same build, same cache): keep rule, pairs.py, small
# families + HOLI-TEST, IMGS-AGAINST, --detail containment, DISC21 shards 0 and 1.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v58-reach; B=$O/../..; P=$B/../vendor/venv/bin/python
V=$O/../v57-fixed/battery
S=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57
C=$S/c.db; D=/home/daniel/Documents; X=$D/DISC21
echo 1000 > /proc/self/oom_score_adj
mkdir -p $O/bat/against $O/bat/detail; cd $O
say() { echo "$(date +%T) $*" >> $O/bat/battery.out; }
has() { grep -q "^$1\$" $O/bat/done.list 2>/dev/null; }
did() { echo "$1" >> $O/bat/done.list; }
# 0. keep rule on the full corpora
for c in all all2; do m=$D/IMGS-ALL/derived/manifest.csv; [ $c = all2 ] && m=$D/IMGS-ALL2/derived/manifest.csv
  for r in hbase ship; do echo "== $c $r"; $P $B/suggest_score.py $O/$c-$r.json --manifest $m 2>&1 | head -12; done
done > $O/bat/keep.out
# 1. pairs.py
has pairs || { rm -f $C.matched; say pairs; $P $B/pairs.py run $O/img-fp-ship $O/bat/pairs-ship --cache $C > $O/bat/pairs-ship.log 2>&1; rm -f $C.matched; did pairs; }
$P $B/pairs.py compare $V/pairs-head $O/bat/pairs-ship > $O/bat/pairs-compare.out 2>&1
# 2. small families and HOLI-TEST
has fam || { say fam; $P $B/families.py run $O/img-fp-ship $O/bat/fam-ship --cache $C --corpus all i5 all2 hdev > $O/bat/fam-ship.log 2>&1; rm -f $C.matched; did fam; }
$P $B/families.py compare $S/fam-base $O/bat/fam-ship > $O/bat/fam-compare.out 2>&1
has htest || { say htest; $P $B/families.py run $O/img-fp-ship $O/bat/fam-htest-ship --cache $C --corpus htest > $O/bat/fam-htest-ship.log 2>&1; rm -f $C.matched; did htest; }
$P $B/families.py compare $V/fam-htest-head $O/bat/fam-htest-ship > $O/bat/htest-compare.out 2>&1
# 3. IMGS-AGAINST
has against || { rm -f $C.matched; say against
  $O/img-fp-ship $D/IMGS-AGAINST/scan --against $D/IMGS-AGAINST/library --follow-symlinks -r -x '*' -t 8 --no-last --cache $C -o $O/bat/against/ship.json 2> $O/bat/against/ship.err
  rm -f $C.matched; did against; }
python3 $B/against_score.py $V/against/head.json $O/bat/against/ship.json > $O/bat/against/score.out 2>&1
# 4. --detail containment on IMGS
for d in 512 1024; do has detail-$d && continue; rm -f $C.matched; say "detail $d"
  $O/img-fp-ship $D/IMGS -r -x '*' -t 8 --no-last --cache $C --detail $d -o $O/bat/detail/ship-$d.json 2> $O/bat/detail/ship-$d.err
  rm -f $C.matched; did detail-$d; done
# 5. DISC21, both shards, the real build
for s in 0 1; do has disc$s && continue; say "disc $s"; REFS=refs$s $O/disc.sh img-fp-ship ship$s >> $O/bat/battery.out; did disc$s; done
say done
