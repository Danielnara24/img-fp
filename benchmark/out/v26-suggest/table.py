import json, glob, os
rows = []
for d in sorted(glob.glob('*_*_*')):
    p = os.path.join(d, 'metrics.json')
    if not os.path.exists(p): continue
    m = json.load(open(p))[0]
    rows.append((d, m['wall_seconds'], m['cpu_seconds'], m['peak_pss_swap_mb'], m['mhz_mean'], m['temp_start_c']))
print(f"{'run':16s} {'wall s':>8s} {'CPU s':>8s} {'peak MB':>8s} {'MHz':>6s} {'start C':>7s}")
for r in rows: print(f"{r[0]:16s} {r[1]:8.1f} {r[2]:8.1f} {r[3]:8.0f} {r[4]:6d} {r[5]:7.1f}")
