#!/bin/bash
# v62: the hard correlation floor against HEAD. Cost, DISC21 shard 0, the
# correlation sweep 0.50-1.00, cliffs, IMGS-PROVENANCE. One run at a time.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v62-floor; B=$O/../..
D=/home/daniel/Documents; X=$D/DISC21
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db
cd $O; echo 1000 > /proc/self/oom_score_adj
say() { echo "$(date '+%F %T') $*" >> $O/batch.log; }
temp() { for h in /sys/class/hwmon/hwmon*; do [ "$(cat $h/name)" = k10temp ] && echo $(( $(cat $h/temp1_input) / 1000 )); done; }
corpus() { [ $1 = all ] && echo $D/IMGS-ALL || echo $D/IMGS-ALL2; }
# score one full-corpus report, keep the text, drop the JSON
keep() { local c=$1 f=$2; local m=$(corpus $c)
  python3 -I $B/score.py --corpus $m $f.json --csv $f.pt.csv > $f.score 2>&1
  python3 -I $B/analyse.py --corpus=$m $f.json > $f.an 2>&1
  rm -f $f.json; }

# ---- 1. cost: cold, no cache, evicted, cooled, ABBA
mkdir -p cost
if [ ! -s cost/done ]; then
  sleep 60; idle=$(temp); say "cost: idle $idle C"
  for w in found:/home/daniel/Downloads/archive all2:$D/IMGS-ALL2; do n=${w%%:*}; p=${w#*:}; i=0
    for b in head floor floor head; do i=$((i+1))
      python3 -I evict.py $p; sleep 60; t=0; while [ $(temp) -gt $((idle+3)) ] && [ $t -lt 900 ]; do sleep 10; t=$((t+10)); done
      t0=$(temp)
      /usr/bin/time -v ./img-fp-$b $p -r -x '*' -t 8 --no-cache --no-last -o cost/$n-$i-$b.json 2> cost/$n-$i-$b.err
      say "cost $n $i $b start ${t0}C end $(temp)C $(grep -E 'User time|Elapsed' cost/$n-$i-$b.err | awk '{print $NF}' | tr '\n' ' ')"
      [ $n = all2 ] && rm -f cost/$n-$i-$b.json
    done
  done
  echo ok > cost/done
fi

# ---- 2. DISC21 shard 0, cached (cache57), bar 0.4 then 0.7, both builds
mkdir -p disc
for r in head:0.4 floor:0.4 floor:0.7 head:0.7; do b=${r%%:*}; bar=${r#*:}; f=disc/$b-$bar
  [ -s $f.json ] && continue; rm -f $X/cache57/c.db.matched
  /usr/bin/time -v ./img-fp-$b $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation $bar -t 8 -v --no-last --cache $X/cache57/c.db --log-file $f.log -o $f.json 2> $f.err
  rm -f $X/cache57/c.db.matched
  say "disc $b $bar $(grep -E 'User time|Elapsed|Maximum res' $f.err | awk '{print $NF}' | tr '\n' ' ')"
done

# ---- 3. correlation sweep 0.50-1.00, both corpora, both builds
mkdir -p sweep
for bar in 0.5 0.55 0.6 0.65 0.7 0.75 0.8 0.85 0.9 0.95 1.0; do
  for c in all all2; do for b in head floor; do f=sweep/$c-$bar-$b
    [ -s $f.an ] && continue; rm -f $C.matched
    ./img-fp-$b $(corpus $c) -r -x '*' -t 8 --no-last --cache $C --min-pixel-correlation $bar -o $f.json 2> $f.err
    rm -f $C.matched; keep $c $f; say "sweep $c $bar $b"
  done; done
done

# ---- 4. cliffs
mkdir -p cliffs
for bar in "--min-aligned-points 6" "--min-aligned-points 3" "--min-frame-overlap 0.6" "--min-frame-overlap 0.5" "--min-frame-overlap 0.4" "--min-frame-overlap 0.3" "--min-frame-overlap 0.25" "--min-pixel-correlation 0.4" "--min-pixel-correlation 0.3" "--min-pixel-correlation 0.2" "-k 50" "-k 10"; do
  t=$(echo "$bar" | tr -d ' -')
  for c in all all2; do for b in head floor; do f=cliffs/$c-$t-$b
    [ -s $f.an ] && continue; rm -f $C.matched
    ./img-fp-$b $(corpus $c) -r -x '*' -t 8 --no-last --cache $C $bar -o $f.json 2> $f.err
    rm -f $C.matched; keep $c $f; say "cliff $c $t $b"
  done; done
done

# ---- 5. IMGS-PROVENANCE
mkdir -p prov
for t in dated same-time; do for b in head floor; do f=prov/$t-$b
  [ -s $f.score ] && continue
  ./img-fp-$b $D/IMGS-PROVENANCE/$t -r --cache $C --no-last -o $f.json 2> $f.err
  python3 -I $B/score_provenance.py $f.json --manifest $D/IMGS-PROVENANCE/manifest.csv > $f.score 2>&1
  say "prov $t $b"
done; done
say ALL DONE
