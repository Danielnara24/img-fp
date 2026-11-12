#!/bin/bash
# util.sh BINARY CACHE OUT: a cached IMGS-ALL2 run, with the process's CPU time sampled every half second
b=$1; c=$2; o=$3; rm -f $c.matched
./$b /home/daniel/Documents/IMGS-ALL2 -r -x '*' -t 8 -v --no-last --cache $c -o /dev/null 2> $o.err &
pid=$!; t0=$(date +%s.%N)
while kill -0 $pid 2>/dev/null; do read -r -a s < /proc/$pid/stat 2>/dev/null || break; echo "$(echo "$(date +%s.%N) - $t0" | bc) $(( s[13] + s[14] ))"; sleep 0.5; done > $o.cpu
rm -f $c.matched
python3 - $o <<'PY'
import sys,re
o=sys.argv[1]
cpu=[(float(a),int(b)/100) for a,b in (l.split() for l in open(o+'.cpu'))]
def at(t):
    import bisect
    i=min(bisect.bisect_left(cpu,(t,0)),len(cpu)-1); return cpu[i][1]
prev=None
for l in open(o+'.err'):
    m=re.match(r'\[ *([0-9.]+)s\] (.{0,45})',l)
    if not m or re.search(r'round [2-9]|pace:',l): continue
    t=float(m.group(1))
    if prev is not None and t-prev[0]>=1.0:
        print(f'{prev[0]:7.1f} -> {t:7.1f}  {t-prev[0]:5.1f}s wall  {(at(t)-at(prev[0])):6.1f} cpu  {(at(t)-at(prev[0]))/(t-prev[0]):4.1f} cores   ... {m.group(2)}')
    prev=(t,m.group(2))
PY
