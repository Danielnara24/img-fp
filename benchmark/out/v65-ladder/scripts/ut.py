import sys,re,bisect
o=sys.argv[1]
cpu=[(float(a),int(b)/100) for a,b in (l.split() for l in open(o+'.cpu'))]
def at(t):
    i=min(bisect.bisect_left(cpu,(t,0)),len(cpu)-1); return cpu[i][1]
prev=None
for l in open(o+'.err'):
    m=re.match(r'\[ *([0-9.]+)s\] (.{0,45})',l)
    if not m or re.search(r'round [2-9]|pace:',l): continue
    t=float(m.group(1))
    if prev is not None and t-prev[0]>=0.25:
        print(f'{prev[0]:7.1f} -> {t:7.1f}  {t-prev[0]:5.1f}s wall  {(at(t)-at(prev[0])):6.1f} cpu  {(at(t)-at(prev[0]))/(t-prev[0]):4.1f} cores   ... {m.group(2)}')
    prev=(t,m.group(2))
