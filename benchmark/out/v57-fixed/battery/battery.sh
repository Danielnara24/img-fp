#!/bin/bash
# The battery after v57 section 6: base (HEAD, img-fp-head) against the fixed seeding with asker 3 paths
# (img-fp-ship): pairs.py, HOLI-TEST, IMGS-AGAINST, --detail containment, cliffs, DISC21 shard 0 + proxy.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v57-fixed/battery; B=$O/../../..; P=$B/../vendor/venv/bin/python
S=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57
C=$S/c.db; D=/home/daniel/Documents; X=$D/DISC21
echo 1000 > /proc/self/oom_score_adj
unset IMGFP_AP IMGFP_OP IMGFP_RP IMGFP_RLAST
cd $O
say() { echo "$(date +%T) $*" >> $O/battery.out; }
an() { if [ $1 = all2 ]; then $P $B/analyse.py --corpus=$D/IMGS-ALL2 $2; else $P $B/analyse.py --corpus=$D/IMGS-ALL $2; fi; }
has() { grep -q "^$1\$" $O/done.list 2>/dev/null; }
did() { echo "$1" >> $O/done.list; }
# 1. pairs.py
for b in head ship; do has pairs-$b && continue; rm -f $C.matched; say "pairs $b"
  $P $B/pairs.py run $O/img-fp-$b $O/pairs-$b --cache $C > $O/pairs-$b.log 2>&1; rm -f $C.matched; did pairs-$b; done
$P $B/pairs.py compare $O/pairs-head $O/pairs-ship > $O/pairs-compare.out 2>&1
# 2. HOLI-TEST
for b in head ship; do has htest-$b && continue; say "htest $b"
  $P $B/families.py run $O/img-fp-$b $O/fam-htest-$b --cache $C --corpus htest > $O/fam-htest-$b.log 2>&1; rm -f $C.matched; did htest-$b; done
$P $B/families.py compare $O/fam-htest-head $O/fam-htest-ship > $O/htest-compare.out 2>&1
# 3. IMGS-AGAINST
for b in head ship; do has against-$b && continue; rm -f $C.matched; say "against $b"
  $O/img-fp-$b $D/IMGS-AGAINST/scan --against $D/IMGS-AGAINST/library --follow-symlinks -r -x '*' -t 8 --no-last --cache $C -o $O/against/$b.json 2> $O/against/$b.err
  rm -f $C.matched; did against-$b; done
python3 $B/against_score.py $O/against/head.json $O/against/ship.json > $O/against/score.out 2>&1
# 4. --detail containment on IMGS, both builds
for b in head ship; do for d in 512 1024; do has detail-$b-$d && continue; rm -f $C.matched; say "detail $b $d"
  $O/img-fp-$b $D/IMGS -r -x '*' -t 8 --no-last --cache $C --detail $d -o $O/detail/$b-$d.json 2> $O/detail/$b-$d.err
  rm -f $C.matched; did detail-$b-$d; done; done
# 5. cliffs, cached; each report scored and dropped
for bar in "--min-aligned-points 8" "--min-aligned-points 6" "--min-aligned-points 5" "--min-aligned-points 4" "--min-aligned-points 3" "--min-frame-overlap 0.75" "--min-frame-overlap 0.65" "--min-frame-overlap 0.6" "--min-frame-overlap 0.5" "--min-frame-overlap 0.4" "--min-frame-overlap 0.3" "--min-frame-overlap 0.25" "--min-pixel-correlation 0.9" "--min-pixel-correlation 0.8" "--min-pixel-correlation 0.6" "--min-pixel-correlation 0.5" "--min-pixel-correlation 0.4" "--min-pixel-correlation 0.3" "--min-pixel-correlation 0.2" "-k 100" "-k 50" "-k 25" "-k 10"; do
  t=$(echo "$bar" | tr -d ' -')
  for c in all all2; do corpus=$D/IMGS-ALL; [ $c = all2 ] && corpus=$D/IMGS-ALL2
    for b in head ship; do f=$O/cliffs/$c-$t-$b; [ -s $f.txt ] && continue; rm -f $C.matched
      $O/img-fp-$b $corpus -r -x '*' -t 8 --no-last --cache $C $bar -o $f.json 2> /dev/null; rm -f $C.matched
      an $c $f.json > $f.txt 2>&1 && rm -f $f.json; done
  done
  say "cliff $bar"
done
# 6. DISC21 shard 0 and the proxy (whose library links into refs0/images)
URL=https://dl.fbaipublicfiles.com/image_similarity_challenge/public
if ! has disc-ship; then
  if [ ! -d $X/refs0/images ]; then say "fetch references_0"
    curl -sSL --retry 5 --retry-delay 10 -C - -o $X/zips/references_0.zip $URL/references_0.zip || { say "download failed"; exit 1; }
    (cd $X/refs0 && unzip -q $X/zips/references_0.zip) && rm -f $X/zips/references_0.zip
    say "refs: $(find $X/refs0 -type f | wc -l) files"; fi
  mkdir -p $X/cache57
  for b in head ship; do has disc-$b && continue; mkdir -p $O/disc/$b; rm -f $X/cache57/c.db.matched; say "disc $b"
    /usr/bin/time -v $O/img-fp-$b $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5 \
      -t 8 -v --no-last --cache $X/cache57/c.db --log-file $O/disc/$b/run.log -o $O/disc/$b/report.json --dump $O/disc/$b/dump.csv 2> $O/disc/$b/time.txt
    say "end disc $b exit $? $(grep -E 'User time' $O/disc/$b/time.txt | awk '{print $NF}') $(grep Elapsed $O/disc/$b/time.txt | awk '{print $NF}')"
    rm -f $X/cache57/c.db.matched; did disc-$b; done
fi
PX=$X/proxy
for b in head ship; do has proxy-$b && continue; rm -f $X/cache57/c.db.matched; say "proxy $b"
  $O/img-fp-$b $PX/q --against $PX/r --follow-symlinks --min-pixel-correlation 0.5 -t 8 --no-last --cache $X/cache57/c.db -o $O/proxy/$b.json 2> $O/proxy/$b.err
  rm -f $X/cache57/c.db.matched; did proxy-$b; done
python3 $PX/cmp.py $O/proxy/head.json $O/proxy/ship.json > $O/proxy/cmp.out 2>&1
python3 $O/disc/score.py > $O/disc/score.out 2>&1
say done
