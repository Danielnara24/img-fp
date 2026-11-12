#!/bin/bash
# ab.sh NAME REPS CORPUS "A-binary A-args" "B-binary B-args": cold runs A B B A ..., user CPU-s each
n=$1; reps=$2; c=$3; A=($4); B=($5); mkdir -p ab; : > ab/$n.log
one() { local t=$1; shift; local b=$1; shift
  /usr/bin/time -f "%U %e" ./$b $c -r -x '*' -t 8 -v --no-cache --no-last "$@" -o /dev/null 2> ab/$n.err
  echo "$t $(tail -1 ab/$n.err) $(grep -o 'cpu: decode [0-9]*s, features [0-9]*s' ab/$n.err | tr -dc '0-9 ')" >> ab/$n.log; sleep 25; }
./${A[0]} $c -r -x '*' -t 8 --no-cache --no-last "${A[@]:1}" -o /dev/null 2>/dev/null; sleep 25
for i in $(seq $reps); do one A "${A[@]}"; one B "${B[@]}"; one B "${B[@]}"; one A "${A[@]}"; done
python3 - ab/$n.log <<'PY'
import sys,statistics as st
r={'A':[], 'B':[]}
for l in open(sys.argv[1]):
    p=l.split(); r[p[0]].append([float(x) for x in p[1:]])
for i,name in enumerate(['user','wall','decode','features']):
    a=[x[i] for x in r['A'] if len(x)>i]; b=[x[i] for x in r['B'] if len(x)>i]
    if a and b: print(f"{name}: A mean {st.mean(a):.1f} median {st.median(a):.1f} | B mean {st.mean(b):.1f} median {st.median(b):.1f} | B/A {st.mean(b)/st.mean(a):.4f} (median {st.median(b)/st.median(a):.4f})  n={len(a)}")
PY
