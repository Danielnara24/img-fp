#!/bin/bash
# Overnight battery: today (cur, = 0.83.0) vs rep (the library rule, any answer) vs repfam
# (one asker per anchor component, clean answers only). Scaffolding build img-fp-re, IMGFP_REASK.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask; B=$O/../..; P=$B/../vendor/venv/bin/python
R=$O/../v58-reach; V=$R/bat
S=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57
C=$S/c.db; D=/home/daniel/Documents
for f in $P $B/pairs.py $B/families.py $B/suggest_score.py $B/against_score.py $B/analyse.py $R/disc.sh $R/abba.sh $R/img-fp-re $O/img-fp-re $C; do [ -e $f ] || { echo "missing $f"; exit 1; }; done
echo 1000 > /proc/self/oom_score_adj
mkdir -p $O/bat/against $O/bat/detail $O/bat/cliffs; cd $O
say() { echo "$(date +%T) $*" >> $O/bat/battery.out; }
has() { grep -q "^$1\$" $O/bat/done.list 2>/dev/null; }
did() { echo "$1" >> $O/bat/done.list; }
say start
# 0. keep rule on the full corpora (reports already made)
for c in all all2; do m=$D/IMGS-ALL/derived/manifest.csv; [ $c = all2 ] && m=$D/IMGS-ALL2/derived/manifest.csv
  for r in cur rep repfam; do echo "== $c $r"; $P $B/suggest_score.py $O/$c-$r.json --manifest $m 2>&1 | head -12; done
done > $O/bat/keep.out
for r in rep repfam; do
  has pairs-$r || { rm -f $C.matched; say "pairs $r"; IMGFP_REASK=$r $P $B/pairs.py run $O/img-fp-re $O/bat/pairs-$r --cache $C > $O/bat/pairs-$r.log 2>&1; rm -f $C.matched; did pairs-$r; }
  $P $B/pairs.py compare $V/pairs-ship $O/bat/pairs-$r > $O/bat/pairs-compare-$r.out 2>&1
  has fam-$r || { say "fam $r"; IMGFP_REASK=$r $P $B/families.py run $O/img-fp-re $O/bat/fam-$r --cache $C --corpus all i5 all2 hdev > $O/bat/fam-$r.log 2>&1; rm -f $C.matched; did fam-$r; }
  $P $B/families.py compare $V/fam-ship $O/bat/fam-$r > $O/bat/fam-compare-$r.out 2>&1
  has htest-$r || { say "htest $r"; IMGFP_REASK=$r $P $B/families.py run $O/img-fp-re $O/bat/fam-htest-$r --cache $C --corpus htest > $O/bat/fam-htest-$r.log 2>&1; rm -f $C.matched; did htest-$r; }
  $P $B/families.py compare $V/fam-htest-ship $O/bat/fam-htest-$r > $O/bat/htest-compare-$r.out 2>&1
  has against-$r || { rm -f $C.matched; say "against $r"
    IMGFP_REASK=$r $O/img-fp-re $D/IMGS-AGAINST/scan --against $D/IMGS-AGAINST/library --follow-symlinks -r -x '*' -t 8 --no-last --cache $C -o $O/bat/against/$r.json 2> $O/bat/against/$r.err
    rm -f $C.matched; did against-$r; }
  python3 $B/against_score.py $V/against/ship.json $O/bat/against/$r.json > $O/bat/against/score-$r.out 2>&1
  for d in 512 1024; do has detail-$r-$d && continue; rm -f $C.matched; say "detail $r $d"
    IMGFP_REASK=$r $O/img-fp-re $D/IMGS -r -x '*' -t 8 --no-last --cache $C --detail $d -o $O/bat/detail/$r-$d.json 2> $O/bat/detail/$r-$d.err
    rm -f $C.matched; did detail-$r-$d; done
done
# DISC21 (--against: rep is today's rule there, so only repfam)
for s in 0 1; do has disc-repfam$s && continue; say "disc repfam $s"; REFS=refs$s $R/disc.sh img-fp-re repfam$s IMGFP_REASK=repfam >> $O/bat/battery.out; did disc-repfam$s; done
# ABBA cost of repfam where not yet measured
for t in found all; do has abba-rf$t && continue; say "abba repfam $t"
  if [ $t = found ]; then a=(/home/daniel/Downloads/archive); else a=($D/IMGS-ALL -x '*'); fi
  $R/abba.sh rf$t ABBA "IMGFP_REASK=" "IMGFP_REASK=repfam" img-fp-re img-fp-re "${a[@]}" -r -t 8 -v > $O/bat/abba-rf$t.out 2>&1; did abba-rf$t; done
# cliffs, cached, all three rules; each report scored and dropped
an() { if [ $1 = all2 ]; then $P $B/analyse.py --corpus=$D/IMGS-ALL2 $2; else $P $B/analyse.py --corpus=$D/IMGS-ALL $2; fi; }
for bar in "--min-aligned-points 6" "--min-aligned-points 3" "--min-frame-overlap 0.6" "--min-frame-overlap 0.5" "--min-frame-overlap 0.4" "--min-frame-overlap 0.3" "--min-frame-overlap 0.25" "--min-pixel-correlation 0.6" "--min-pixel-correlation 0.5" "--min-pixel-correlation 0.4" "--min-pixel-correlation 0.3" "--min-pixel-correlation 0.2" "-k 50" "-k 10"; do
  t=$(echo "$bar" | tr -d ' -')
  for c in all all2; do corpus=$D/IMGS-ALL; [ $c = all2 ] && corpus=$D/IMGS-ALL2
    for r in cur rep repfam; do f=$O/bat/cliffs/$c-$t-$r; [ -s $f.txt ] && continue; rm -f $C.matched
      IMGFP_REASK=$r $O/img-fp-re $corpus -r -x '*' -t 8 --no-last --cache $C $bar -o $f.json 2> /dev/null; rm -f $C.matched
      an $c $f.json > $f.txt 2>&1 && rm -f $f.json; done
  done
  say "cliff $bar"
done
say done
