import csv, json, re, sys
for t in ('all', 'all2'):
    rows = list(csv.DictReader(open(f'{t}/per_transform.csv')))
    tools = [c for c in rows[0] if c.endswith('.json')]
    perf = {}
    for c in tools:
        n = tot = 0
        for r in rows:
            v = r[c]
            m = re.match(r'(\d+)/(\d+)', v or '')
            if not m or m.group(2) == '0': continue
            tot += 1; n += m.group(1) == m.group(2)
        perf[c[:-5]] = (n, tot)
    sc = {}
    for line in open(f'{t}/score.txt'):
        m = re.match(r'(\S+)\.json\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d.]+)%\s+([\d.]+)%\s+([\d.]+)', line)
        if m: sc[m.group(1)] = (float(m.group(8)), m.group(6), m.group(7))
    met = {m['tool']: m for m in json.load(open(f'{t}/metrics.json'))}
    nfiles = json.load(open(f'{t}/run_meta.json'))['corpus_files']
    print(f'== {t} ({nfiles} files)')
    print('| tool | F1 | precision | recall | perfect | wall | CPU s | CPU% | PSS | anon+swap | proc | img/s | MHz | start C |')
    for name in sorted(met, key=lambda k: -sc.get(k, (0,))[0]):
        m = met[name]; s = sc.get(name)
        f1 = f'{s[0]:.3f}' if s else '-'
        print(f"| {name} | {f1} | {s[1] if s else '-'}% | {s[2] if s else '-'}% | {perf.get(name, ('-','-'))[0]} | {m['wall_seconds']:,.0f} s | {m['cpu_seconds']:,.0f} | {m['cpu_percent']:.0f}% | {m['peak_pss_mb']:,.0f} MB | {m['peak_anon_swap_mb']:,.0f} MB | {m['max_processes']} | {nfiles / m['wall_seconds']:.0f} | {m.get('mhz_mean')} | {m['temp_start_c']:.0f} |")
    print('perfect denominators', {k: v[1] for k, v in perf.items()})
