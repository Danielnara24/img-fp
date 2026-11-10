import re, os
steps=['default']+'minalignedpoints8 minalignedpoints6 minalignedpoints5 minalignedpoints4 minalignedpoints3 minframeoverlap0.75 minframeoverlap0.65 minframeoverlap0.6 minframeoverlap0.5 minframeoverlap0.4 minframeoverlap0.3 minframeoverlap0.25 minpixelcorrelation0.9 minpixelcorrelation0.8 minpixelcorrelation0.6 minpixelcorrelation0.5 minpixelcorrelation0.4 minpixelcorrelation0.3 minpixelcorrelation0.2 k100 k50 k25 k10'.split()
def read(p):
    s=open(p).read()
    m=re.search(r'F1 ([0-9.]+)\s+precision ([0-9.]+)%\s+recall ([0-9.]+)%',s)
    tp=re.search(r'TP ([0-9,]+)\s+FP ([0-9,]+)',s)
    cf=re.search(r'cross-family ([0-9,]+)',s)
    mer=[(a.split('/')[-1].rsplit('.',1)[0]+'|'+b.split('/')[-1].rsplit('.',1)[0],int(n.replace(',',''))) for a,b,n in re.findall(r'cross (.+?) <-> (.+?): ([0-9,]+) pairs',s)]
    return dict(f1=float(m[1]),p=float(m[2]),r=float(m[3]),tp=int(tp[1].replace(',','')),fp=int(tp[2].replace(',','')),cf=int(cf[1].replace(',','')),big=[f'{k} {n:,}' for k,n in mer if n>=300])
for c in ('all','all2'):
    print(f'== {"IMGS-ALL" if c=="all" else "IMGS-ALL2"}   (base | fixed, asker 3)')
    print(f'{"step":>24} {"F1":>15} {"recall":>13} {"TP diff":>8} {"FP diff":>8} {"cross-family":>15}  merges >= 300 (base || fixed)')
    for s in steps:
        if s=='default': a,b=read(f'ss-def-{c}-head.txt'),read(f'ss-def-{c}-ship.txt')
        elif not os.path.exists(f'cliffs/{c}-{s}-ship.txt'): continue
        else: a,b=read(f'cliffs/{c}-{s}-head.txt'),read(f'cliffs/{c}-{s}-ship.txt')
        same='same' if a['big']==b['big'] else 'DIFFER'
        print(f'{s:>24} {a["f1"]:.4f}|{b["f1"]:.4f} {a["r"]:6.2f}|{b["r"]:5.2f} {b["tp"]-a["tp"]:>+8,} {b["fp"]-a["fp"]:>+8,} {a["cf"]:>7,}|{b["cf"]:<7,}  {same}: {", ".join(a["big"]) or "none"} || {", ".join(b["big"]) or "none"}')
    print()
