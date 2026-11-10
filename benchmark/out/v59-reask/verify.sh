#!/bin/bash
# verify.sh: the hybrid build (img-fp-hyb) against the scaffolding's repfam runs (plain scans)
# and today's build (--against); reports compared less runtime_seconds.
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v59-reask; R=$O/../v58-reach
C=/tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/5e1f28b2-cadc-4f27-bda2-a6acef097ec4/scratchpad/v57/c.db
D=/home/daniel/Documents; X=$D/DISC21; H=$O/hyb; mkdir -p $H
for f in $O/img-fp-hyb $C $O/mir.db $R/bat/against/ship.json $R/disc-ship0/report.json $O/bat/detail/repfam-512.json $O/all-repfam.json $O/all2-repfam.json $O/found-repfam.json $O/mir-repfam.json; do [ -e $f ] || { echo "missing $f"; exit 1; }; done
echo 1000 > /proc/self/oom_score_adj
same() { python3 - "$1" "$2" <<'P'
import json,sys
a,b=(json.load(open(f)) for f in sys.argv[1:3])
for x in (a,b): x.pop('runtime_seconds',None)
print('IDENTICAL' if a==b else 'DIFFERENT', sys.argv[1].split('/')[-1], sys.argv[2].split('/')[-1])
P
}
run() { n=$1 c=$2; shift 2; rm -f $c.matched; /usr/bin/time -f "%U user %e wall" $O/img-fp-hyb "$@" -t 8 -v --no-last --cache $c -o $H/$n.json 2> $H/$n.err; rm -f $c.matched; echo "$n $(tail -1 $H/$n.err) $(grep 'variants:' $H/$n.err | sed 's/.*variants: //')"; }
run mir $O/mir.db $D/MIRFAM -r; same $H/mir.json $O/mir-repfam.json
run found $C /home/daniel/Downloads/archive -r; same $H/found.json $O/found-repfam.json
run all $C $D/IMGS-ALL -x '*' -r; same $H/all.json $O/all-repfam.json
run all2 $C $D/IMGS-ALL2 -x '*' -r; same $H/all2.json $O/all2-repfam.json
for d in 512 1024; do run detail-$d $C $D/IMGS -r -x '*' --detail $d; same $H/detail-$d.json $O/bat/detail/repfam-$d.json; done
run against $C $D/IMGS-AGAINST/scan --against $D/IMGS-AGAINST/library --follow-symlinks -r -x '*'; same $H/against.json $R/bat/against/ship.json
run disc0 $X/cache57/c.db $X/queries/images/queries --against $X/refs0/images/references --min-pixel-correlation 0.5; same $H/disc0.json $R/disc-ship0/report.json
echo done
