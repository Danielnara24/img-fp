#!/usr/bin/env python3
"""The battery a matcher or analysis change answers to: a build against a base
build on everything CLAUDE.md says a change is judged by, one run at a time,
and a table of what moved.

    validate.py run BASE NEW OUT [--stages ...] [--corpora ...] [--details ...]
    validate.py report OUT

BASE and NEW are img-fp binaries (plain x86-64 builds, `target/plain`). Each
is measured on caches it filled itself: OUT/base/cache, OUT/new/cache and the
DISC21 ones beside them, unless `--base-cache` and the like name others (a
record another analysis made is not read, so a wrong cache costs a fill, not
a wrong figure). A step whose output is in OUT is not run again: an
interrupted battery resumes, and `--base-results OLD/base` takes a base's
accuracy from an earlier battery (the costs are always run afresh, paired).

Stages, in the order run (`--stages` picks; the order is fixed):

  disc      DISC21 shard 0 (50,000 queries `--against` 50,000 references,
            529 true copies), at bars 0.5 and 0.7: true and false copies,
            µAP, recall at 90% precision. First, because it is the one that
            sees what retrieval never reached: a build level on the IMGS
            corpora lost 33 of 382 true copies here (out/v65-ladder). About
            40 minutes a build to fill its cache and 15 a run.
  proxy     only when named: a sixth of the shard (`disc_proxy.ids`: the
            queries and references of its true copies, 5,000 other queries,
            10,000 other references), five minutes a build, for a first look.
            It has the sign of the shard's losses at a half to all of their
            size, under-reads a small one, and says nothing by a few copies
            (out/v66-proxy). It reads and fills the cache `disc` does.
  corpora   IMGS-ALL2 and IMGS-ALL at the defaults, cached: F1, precision,
            recall, traps against cross-family, merges, seed pairs past 300,
            perfect transforms; the keep rule (`suggest_score.py`); whether
            the pair lists are the base's to the byte.
  fast      the same two under `--fast`.
  detail    NEW at each `--details` step (default 1024 2048 4096), finest
            first so the coarser read its records: true pairs of each step
            the next does not state (containment), and its score.
  families  `families.py` on both corpora: total joined, where siblings meet.
  cached    matching cost: IMGS-ALL2 cached, base new new base.
  cold      cost: `--no-cache`, corpus evicted, rested until the processor is
            back within 3 C of idle, base new new base on the found corpus
            (and its pair lists compared), Desktop (twice), DISC21 `r10k`,
            IMGS-ALL2 and IMGS-ALL (5 minutes' rest each).

A whole battery is about seven hours and wants ~25 GB beside OUT for a build's
caches. Start it detached (`CLAUDE_CODE_DISABLE_BG_SHELL_PRESSURE_REAP=1 nohup
... &`) and stop it by PID; nothing else large may run beside it (5 GB of
memory). Every img-fp run here exits 2 (a truncated file in the corpora).

`report` reads OUT and prints each figure for base and new with a mark:
`=` the same, `+` better, `WORSE` where the build lost something a change may
not lose (true pairs or copies, a new seed pair past 300, a true pair between
detail steps, a keep-rule loss, a family joined), `over` where its mean cost
is above the base's by more than the two builds' own ranges together.
DISC21's "base's true copies not found" is marked `lost`, not `WORSE`: two
level builds each find a dozen copies there the other does not; the count is
what is judged. Accuracy reproduces to the pair, so any difference is the build's; cost does
not, and within those ranges it is `level`.
"""
import argparse, csv, json, os, re, statistics, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
DOCS = os.path.expanduser('~/Documents')
CORPORA = {'all2': f'{DOCS}/IMGS-ALL2', 'all': f'{DOCS}/IMGS-ALL'}
FOUND = os.path.expanduser('~/Downloads/archive')
DESKTOP = f'{DOCS}/IMGS/derived/Desktop'
DISC = f'{DOCS}/DISC21'
STAGES = ['disc', 'corpora', 'fast', 'detail', 'families', 'cached', 'cold']  # what a battery runs
EXTRA = ['proxy']  # only when named
MERGE = 300  # a seed pair with this many false pairs is a merge (CLAUDE.md, *Parameters*)
SCAN = ['-r', '-x', '*', '-t', '8', '-v', '--no-last']


LOG = None  # the battery's OUT, where its log is


def log(out, *words):
    line = time.strftime('%F %T ') + ' '.join(str(w) for w in words)
    with open(os.path.join(LOG or out, 'log'), 'a') as f:
        f.write(line + '\n')
    print(line, flush=True)


def temperature():
    for h in sorted(os.listdir('/sys/class/hwmon')):
        d = f'/sys/class/hwmon/{h}'
        try:
            if open(f'{d}/name').read().strip() == 'k10temp':
                return int(open(f'{d}/temp1_input').read()) // 1000
        except OSError:
            pass
    return None


def evict(root):
    """Drop a corpus's pages from the page cache (no sudo here)."""
    for d, _, fs in os.walk(root):
        for f in fs:
            try:
                fd = os.open(os.path.join(d, f), os.O_RDONLY)
                os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
                os.close(fd)
            except OSError:
                pass


def timed(binary, args, err):
    """One img-fp run under `time -v`, its stderr (and time's) in `err`."""
    with open(err, 'w') as e:
        r = subprocess.run(['/usr/bin/time', '-v', binary] + args, stderr=e, stdout=subprocess.DEVNULL)
    if r.returncode not in (0, 2):
        raise SystemExit(f'{binary} exited {r.returncode}: see {err}')


def done(err):
    return os.path.exists(err) and 'Exit status' in open(err, errors='replace').read()


def cost_of(err):
    """(user CPU-s, wall s, peak RSS MB) from a `time -v` log."""
    t = open(err, errors='replace').read()
    cpu = float(re.search(r'User time \(seconds\): ([\d.]+)', t).group(1))
    w = [float(x) for x in re.search(r'Elapsed .*: ([\d:.]+)', t).group(1).split(':')]
    wall = sum(x * 60 ** i for i, x in enumerate(reversed(w)))
    rss = int(re.search(r'Maximum resident set size \(kbytes\): (\d+)', t).group(1)) / 1024
    return cpu, wall, rss


def write_pairs(report, dest):
    """A report's pairs, one a line, each and all sorted: what two runs are compared by."""
    rows = sorted('\t'.join(sorted((p['a'], p['b']))) for p in json.load(open(report))['pairs'])
    with open(dest, 'w') as f:
        f.write('\n'.join(rows) + '\n')


def cached_run(out, binary, cache, corpus, name, extra=()):
    """A run on `cache` (filled by the run if it is not), scored; the report is not kept."""
    f = os.path.join(out, name)
    if os.path.exists(f + '.an'):
        return
    os.makedirs(os.path.dirname(cache), exist_ok=True)
    matched = cache + '.matched'
    if os.path.exists(matched):
        os.remove(matched)
    timed(binary, [corpus] + SCAN + ['--cache', cache, '-o', f + '.json'] + list(extra), f + '.err')
    if os.path.exists(matched):
        os.remove(matched)
    write_pairs(f + '.json', f + '.pairs')
    with open(f + '.keep', 'w') as k:
        subprocess.run([sys.executable, '-I', f'{HERE}/suggest_score.py', f + '.json', '--manifest', f'{corpus}/derived/manifest.csv'],
                       stdout=k, stderr=subprocess.STDOUT)
    with open(f + '.an.part', 'w') as a:
        subprocess.run([sys.executable, '-I', f'{HERE}/analyse.py', f'--corpus={corpus}', f + '.json'], stdout=a, stderr=subprocess.STDOUT)
    os.remove(f + '.json')
    os.rename(f + '.an.part', f + '.an')
    log(os.path.dirname(out), name, os.path.basename(out), '%.0f CPU-s %.0f s' % cost_of(f + '.err')[:2])


def disc_ids(kind):
    """The ids a DISC21 stage holds: shard 0 whole (`disc`), or the proxy's
    (`disc_proxy.ids`: the shard's 529 true copies' queries and references,
    5,000 other queries and 10,000 other references)."""
    if kind == 'disc':
        return None, {l.strip() for l in open(f'{DISC}/refs0.ids')}
    ids = {l.strip() for l in open(f'{HERE}/disc_proxy.ids')}
    return {i for i in ids if i[0] == 'Q'}, {i for i in ids if i[0] == 'R'}


def disc_sides(top, kind):
    """The two folders a DISC21 stage scans: the shard's own, or links to the
    proxy's files under OUT."""
    sides = f'{DISC}/queries/images/queries', f'{DISC}/refs0/images/references'
    if kind == 'disc':
        return sides
    farm = os.path.join(top, 'disc-proxy')
    if not os.path.exists(farm):
        for name, src, ids in zip('qr', sides, disc_ids(kind)):
            os.makedirs(os.path.join(farm + '.part', name))
            for i in ids:
                os.symlink(f'{src}/{i}.jpg', os.path.join(farm + '.part', name, i + '.jpg'))
        os.rename(farm + '.part', farm)
    return os.path.join(farm, 'q'), os.path.join(farm, 'r')


def disc_run(top, out, binary, cache, kind, bar):
    f = os.path.join(out, f'{kind}-{bar}')
    if os.path.exists(f + '.score'):
        return
    os.makedirs(os.path.dirname(cache), exist_ok=True)
    matched = cache + '.matched'
    if os.path.exists(matched):
        os.remove(matched)
    q, r = disc_sides(top, kind)
    timed(binary, [q, '--against', r, '--follow-symlinks', '--min-pixel-correlation', str(bar),
                   '-t', '8', '-v', '--no-last', '--cache', cache, '-o', f + '.json'], f + '.err')
    if os.path.exists(matched):
        os.remove(matched)
    json.dump(disc_score(f + '.json', kind, bar), open(f + '.score', 'w'))
    os.remove(f + '.json')
    log(top, f'{kind} {bar}', os.path.basename(out), '%.0f CPU-s %.0f s' % cost_of(f + '.err')[:2])


def disc_score(report, kind, bar):
    """A DISC21 stage from a report's pairs at `bar`, ranked by aligned points +
    correlation / 2 (the shards' scorer's way): true and false copies, µAP,
    recall at 90% precision."""
    ident = lambda p: os.path.splitext(os.path.basename(p))[0]
    queries, refs = disc_ids(kind)
    gt = {(r[0], r[1]) for r in csv.reader(open(f'{DISC}/gt/dev_ground_truth.csv'))
          if len(r) > 1 and r[1] in refs and (queries is None or r[0] in queries)}
    best = {}
    for p in json.load(open(report))['pairs']:
        a, b = ident(p['a']), ident(p['b'])
        if {a[0], b[0]} != {'Q', 'R'}:
            continue
        k = (a, b) if a[0] == 'Q' else (b, a)
        v = (p['aligned_points'] or 0, p['pixel_correlation'])
        best[k] = max(best.get(k, v), v)
    ranked = sorted(((v[0] + v[1] / 2, k in gt) for k, v in best.items() if v[1] >= bar), reverse=True)
    tp = ap = r90 = 0
    for i, (_, t) in enumerate(ranked, 1):
        if t:
            tp += 1
            ap += tp / i
        if tp / i >= 0.9:
            r90 = max(r90, tp / len(gt))
    return {'copies': len(gt), 'true': tp, 'false': len(ranked) - tp, 'uap': 100 * ap / len(gt), 'r90': 100 * r90,
            'found': sorted('|'.join(k) for k, v in best.items() if k in gt and v[1] >= bar)}


def rest(out, corpus, secs, idle):
    evict(corpus)
    time.sleep(secs)
    waited = 0
    while idle is not None and (temperature() or 0) > idle + 3 and waited < 900:
        time.sleep(10)
        waited += 10


def paired(out, tag, order, run, between):
    """`order` is a string of b / n; each run is OUT/cost/TAG-i-build."""
    for i, c in enumerate(order, 1):
        which = 'base' if c == 'b' else 'new'
        f = os.path.join(out, 'cost', f'{tag}-{i}-{which}')
        if done(f + '.err'):
            continue
        between()
        t0 = temperature()
        run(which, f)
        log(out, 'cost', tag, i, which, f'{t0}C -> {temperature()}C', '%.0f CPU-s %.1f s %.0f MB' % cost_of(f + '.err'))


def run(a):
    global LOG
    out = LOG = os.path.abspath(a.out)
    os.makedirs(os.path.join(out, 'cost'), exist_ok=True)
    try:
        open('/proc/self/oom_score_adj', 'w').write('1000')  # under pressure, this and its runs go first
    except OSError:
        pass
    binary = {'base': os.path.abspath(a.base), 'new': os.path.abspath(a.new)}
    res = {'base': os.path.abspath(a.base_results) if a.base_results else os.path.join(out, 'base'), 'new': os.path.join(out, 'new')}
    cache = {'base': a.base_cache or os.path.join(res['base'], 'cache', 'c.db'), 'new': a.new_cache or os.path.join(out, 'new', 'cache', 'c.db')}
    dcache = {'base': a.base_disc_cache or os.path.join(res['base'], 'disc-cache', 'c.db'),
              'new': a.new_disc_cache or os.path.join(out, 'new', 'disc-cache', 'c.db')}
    for d in res.values():
        os.makedirs(d, exist_ok=True)
    json.dump({'base': binary['base'], 'new': binary['new'], 'base_results': res['base'], 'details': a.details}, open(os.path.join(out, 'run.json'), 'w'))
    stages = [s for s in ['proxy'] + STAGES if s in a.stages]
    log(out, 'stages', *stages)
    both = ('base', 'new')
    corpora = {t: CORPORA[t] for t in a.corpora}
    for kind in ('proxy', 'disc'):
        if kind in stages:
            for w in both:
                for bar in (0.5, 0.7):
                    disc_run(out, res[w], binary[w], dcache[w], kind, bar)
    if 'corpora' in stages:
        for tag, corpus in corpora.items():
            for w in both:
                cached_run(res[w], binary[w], cache[w], corpus, f'{tag}-512')
    if 'fast' in stages:
        for tag, corpus in corpora.items():
            for w in both:
                cached_run(res[w], binary[w], cache[w], corpus, f'{tag}-fast', ['--fast'])
    if 'detail' in stages:
        for tag, corpus in corpora.items():
            cached_run(res['new'], binary['new'], cache['new'], corpus, f'{tag}-512')
            for d in sorted(a.details, reverse=True):
                cached_run(res['new'], binary['new'], cache['new'], corpus, f'{tag}-{d}', ['--detail', str(d)])
    if 'families' in stages:
        for w in both:
            f = os.path.join(res[w], 'fam')
            if os.path.exists(f + '.score'):
                continue
            with open(f + '.out', 'w') as o:
                subprocess.run([sys.executable, '-I', f'{HERE}/families.py', 'run', binary[w], f, '--cache', cache[w], '--corpus'] + sorted(corpora),
                               stdout=o, stderr=subprocess.STDOUT)
            with open(f + '.score.part', 'w') as o:
                subprocess.run([sys.executable, '-I', f'{HERE}/families.py', 'score', f], stdout=o, stderr=subprocess.STDOUT)
            os.rename(f + '.score.part', f + '.score')
            log(out, 'families', w)
    if 'cached' in stages:
        def one(which, f):
            m = cache[which] + '.matched'
            if os.path.exists(m):
                os.remove(m)
            timed(binary[which], [CORPORA['all2']] + SCAN + ['--cache', cache[which], '-o', f + '.json'], f + '.err')
            os.remove(f + '.json')
            if os.path.exists(m):
                os.remove(m)
        for w in both:  # a cache to read, where the corpora stage was not run
            cached_run(res[w], binary[w], cache[w], CORPORA['all2'], 'all2-512')
        paired(out, 'cached-all2', 'bnnb', one, lambda: time.sleep(60))
    if 'cold' in stages:
        time.sleep(90)
        idle = temperature()
        log(out, 'cold: idle', idle, 'C')

        def cold(corpus, keep_pairs=False, extra=()):
            def one(which, f):
                timed(binary[which], [corpus] + SCAN[:-1] + ['--no-cache', '--no-last', '-o', f + '.json'] + list(extra), f + '.err')
                if keep_pairs:
                    write_pairs(f + '.json', f + '.pairs')
                os.remove(f + '.json')
            return one
        paired(out, 'found', 'bnnb', cold(FOUND, True), lambda: rest(out, FOUND, 60, idle))
        paired(out, 'found-fast', 'bnnb', cold(FOUND, True, ['--fast']), lambda: rest(out, FOUND, 60, idle))
        paired(out, 'desktop', 'bnnbnbbn', cold(DESKTOP), lambda: rest(out, DESKTOP, 60, idle))
        paired(out, 'r10k', 'bnnb', cold(f'{DISC}/r10k'), lambda: rest(out, f'{DISC}/r10k', 60, idle))
        for tag, corpus in corpora.items():
            paired(out, tag, 'bnnb', cold(corpus), lambda: rest(out, corpus, 300, idle))
    log(out, 'DONE')
    return report(a)


# ---- the table ----

def read_an(path):
    """analyse.py's printed score, as numbers."""
    t = open(path, errors='replace').read()
    num = lambda s: int(s.replace(',', ''))
    m = re.search(r'F1 ([\d.]+)\s+precision ([\d.]+)%\s+recall ([\d.]+)%', t)
    c = re.search(r'proposed ([\d,]+)\s+TP ([\d,]+)\s+FP ([\d,]+)', t)
    f = re.search(r'FP: trap ([\d,]+)\s+cross-family ([\d,]+)\s+seed-pairs ([\d,]+)\s+merges ([\d,]+)', t)
    p = re.search(r'perfect transforms: (\d+) of (\d+)', t)
    past = {f'{os.path.basename(x)}|{os.path.basename(y)}': num(n) for x, y, n in re.findall(r'cross (.+?) <-> (.+?): ([\d,]+) pairs', t) if num(n) >= MERGE}
    return {'f1': float(m.group(1)), 'p': float(m.group(2)), 'r': float(m.group(3)), 'tp': num(c.group(2)), 'fp': num(c.group(3)),
            'trap': num(f.group(1)), 'cross': num(f.group(2)), 'merges': num(f.group(4)), 'perfect': int(p.group(1)) if p else None, 'past': past}


def read_keep(path):
    t = open(path, errors='replace').read()
    lost = re.search(r'content lost by deletion: (\{.*?\})', t)
    kept = re.search(r'original or exact/lossless copy survives: (\d+) / (\d+)', t)
    faithful = re.search(r'faithful full copy: (\d+) / (\d+)', t)
    n = sum(int(x) for x in re.findall(r': (\d+)', lost.group(1))) if lost else None
    return {'lost': n, 'kept': int(kept.group(1)) if kept else None, 'faithful': int(faithful.group(1)) if faithful else None}


def read_families(path):
    """{corpus tag: total joined} from `families.py score`."""
    res, tag = {}, None
    for line in open(path, errors='replace'):
        if line.startswith('== '):
            tag = line[3:].strip()
        elif line.startswith('mean ') and tag:
            res[tag] = float(re.search(r'total joined ([\d.]+)', line).group(1))
    return res


def positives(corpus):
    pos = set()
    for r in csv.DictReader(open(f'{corpus}/derived/pairs.csv')):
        if r['relation'] in ('SAME', 'CROP'):
            pos.add('\t'.join(sorted((r['a'], r['b']))))
    return pos


def lines(path):
    return set(open(path).read().split('\n')) - {''}


def same_file(a, b):
    return os.path.getsize(a) == os.path.getsize(b) and open(a, 'rb').read() == open(b, 'rb').read()


def report(a):
    out = os.path.abspath(a.out)
    cfg = json.load(open(os.path.join(out, 'run.json')))
    res = {'base': cfg['base_results'], 'new': os.path.join(out, 'new')}
    rows, worse = [], []

    def row(what, b, n, mark):
        rows.append((what, b, n, mark))
        if mark in ('WORSE', 'over'):
            worse.append(what)

    def more(what, b, n, fmt='{:,}'):  # more is better
        row(what, fmt.format(b), fmt.format(n), '=' if n == b else '+' if n > b else 'WORSE')

    def fewer(what, b, n, fmt='{:,}', hard=True):  # fewer is better
        row(what, fmt.format(b), fmt.format(n), '=' if n == b else '+' if n < b else 'WORSE' if hard else 'more')

    have = lambda w, name: os.path.exists(os.path.join(res[w], name))
    for kind, name in (('proxy', 'DISC21 proxy'), ('disc', 'DISC21')):
        for bar in (0.5, 0.7):
            if have('base', f'{kind}-{bar}.score') and have('new', f'{kind}-{bar}.score'):
                b, n = (json.load(open(os.path.join(res[w], f'{kind}-{bar}.score'))) for w in ('base', 'new'))
                more(f'{name} {bar} true copies of {b["copies"]}', b['true'], n['true'])
                lost = len(set(b['found']) - set(n['found']))
                row(f'{name} {bar} base\'s true copies not found', '', str(lost), '=' if lost == 0 else 'lost')
                more(f'{name} {bar} µAP', b['uap'], n['uap'], '{:.2f}')
                more(f'{name} {bar} recall at 90% precision', b['r90'], n['r90'], '{:.2f}')
                fewer(f'{name} {bar} false copies', b['false'], n['false'], hard=False)
    for tag, corpus in CORPORA.items():
        for kind in ('512', 'fast'):
            name = f'{tag}-{kind}'
            if not (have('base', name + '.an') and have('new', name + '.an')):
                continue
            label = f'{tag} {"default" if kind == "512" else "--fast"}'
            if same_file(os.path.join(res['base'], name + '.pairs'), os.path.join(res['new'], name + '.pairs')):
                row(f'{label} pair list', '', '', 'identical')
            b, n = (read_an(os.path.join(res[w], name + '.an')) for w in ('base', 'new'))
            row(f'{label} F1', f'{b["f1"]:.4f}', f'{n["f1"]:.4f}', '=' if b['f1'] == n['f1'] else '+' if n['f1'] > b['f1'] else 'lower')
            more(f'{label} true pairs (recall {b["r"]:.2f} / {n["r"]:.2f}%)', b['tp'], n['tp'])
            fewer(f'{label} false pairs (precision {b["p"]:.2f} / {n["p"]:.2f}%)', b['fp'], n['fp'], hard=False)
            fewer(f'{label} traps', b['trap'], n['trap'], hard=False)
            fewer(f'{label} cross-family', b['cross'], n['cross'], hard=False)
            fewer(f'{label} merges', b['merges'], n['merges'], hard=False)
            row(f'{label} seed pairs past {MERGE}', str(len(b['past'])), str(len(n['past'])),
                'WORSE' if set(n['past']) - set(b['past']) else '=' if set(n['past']) == set(b['past']) else '+')
            for k in sorted(set(b['past']) | set(n['past'])):
                cell = lambda d: f'{d[k]:,}' if k in d else 'under'
                row(f'    {k.replace(".jpg", "").replace("|", " / ")}', cell(b['past']), cell(n['past']), '' if k in b['past'] else 'new')
            if b['perfect'] is not None and n['perfect'] is not None:
                more(f'{label} perfect transforms', b['perfect'], n['perfect'])
            if kind == '512':
                kb, kn = (read_keep(os.path.join(res[w], name + '.keep')) for w in ('base', 'new'))
                if kb['lost'] is not None and kn['lost'] is not None:
                    fewer(f'{label} keep rule: content lost', kb['lost'], kn['lost'])
                    more(f'{label} keep rule: original or lossless kept', kb['kept'], kn['kept'])
                    more(f'{label} keep rule: faithful copy kept', kb['faithful'], kn['faithful'])
        steps = [s for s in ['512'] + [str(d) for d in sorted(cfg['details'])] if have('new', f'{tag}-{s}.pairs')]
        if len(steps) > 1:
            pos = positives(corpus)
            below = lines(os.path.join(res['new'], f'{tag}-{steps[0]}.pairs'))
            for lo, hi in zip(steps, steps[1:]):
                here = lines(os.path.join(res['new'], f'{tag}-{hi}.pairs'))
                gone = below - here
                true = len(gone & pos)
                an = read_an(os.path.join(res['new'], f'{tag}-{hi}.an'))
                row(f'{tag} {lo} -> {hi}: true pairs lost (of {len(gone):,} pairs lost)', '', str(true), '=' if true == 0 else 'WORSE')
                row(f'{tag} --detail {hi}: F1 {an["f1"]:.4f}, precision {an["p"]:.2f}, recall {an["r"]:.2f}, {len((here - below) & pos):,} true pairs added', '', '', '')
                below = here
    if have('base', 'fam.score') and have('new', 'fam.score'):
        b, n = (read_families(os.path.join(res[w], 'fam.score')) for w in ('base', 'new'))
        for tag in sorted(set(b) & set(n)):
            fewer(f'small families, {tag}: total joined', b[tag], n[tag], '{:.1f}')
    cost = os.path.join(out, 'cost')
    tags = sorted({re.sub(r'-\d+-(base|new)\.err$', '', f) for f in os.listdir(cost) if f.endswith('.err')}) if os.path.isdir(cost) else []
    for tag in tags:
        runs = {'base': [], 'new': []}
        for f in sorted(os.listdir(cost)):
            m = re.fullmatch(re.escape(tag) + r'-\d+-(base|new)\.err', f)
            if m and done(os.path.join(cost, f)):
                runs[m.group(1)].append(cost_of(os.path.join(cost, f)))
        if not (runs['base'] and runs['new']):
            continue
        for k, unit, name in ((0, 'CPU-s', 'CPU'), (1, 's', 'wall'), (2, 'MB', 'peak RSS')):
            b, n = [r[k] for r in runs['base']], [r[k] for r in runs['new']]
            ratio = statistics.mean(n) / statistics.mean(b)
            # Apart only when the means differ by more than the two builds'
            # own ranges together: with two runs a side, "past every base
            # run" alone called a build under itself.
            gap = abs(statistics.mean(n) - statistics.mean(b)) > (max(b) - min(b)) + (max(n) - min(n))
            mark = 'over' if gap and ratio > 1 else 'under' if gap else 'level'
            if k == 2 and mark == 'over':
                mark = 'more'
            show = lambda xs: ' '.join(f'{x:,.0f}' if x >= 100 else f'{x:.1f}' for x in xs)
            row(f'cost {tag}: {name} ({unit})', show(b), show(n), f'{mark} ({ratio - 1:+.1%})')
        ps = {w: sorted(f for f in os.listdir(cost) if re.fullmatch(re.escape(tag) + rf'-\d+-{w}\.pairs', f)) for w in ('base', 'new')}
        if ps['base'] and ps['new']:
            alike = all(same_file(os.path.join(cost, ps['base'][0]), os.path.join(cost, f)) for f in ps['base'] + ps['new'])
            count = lambda f: sum(1 for _ in open(os.path.join(cost, f)))
            row(f'{tag}: pair lists', f'{count(ps["base"][0]):,}', f'{count(ps["new"][0]):,}', 'identical' if alike else 'differ')
    w0 = max((len(r[0]) for r in rows), default=10)
    w1 = max((len(r[1]) for r in rows), default=4)
    w2 = max((len(r[2]) for r in rows), default=4)
    text = [f'base {cfg["base"]}', f'new  {cfg["new"]}', '', f'{"":<{w0}}  {"base":>{w1}}  {"new":>{w2}}']
    text += [f'{a:<{w0}}  {b:>{w1}}  {n:>{w2}}  {m}' for a, b, n, m in rows]
    text += ['', 'WORSE or over: ' + ('; '.join(worse) if worse else 'nothing')]
    open(os.path.join(out, 'report.txt'), 'w').write('\n'.join(text) + '\n')
    print('\n'.join(text))
    return 1 if worse else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    r = sub.add_parser('run')
    r.add_argument('base')
    r.add_argument('new')
    r.add_argument('out')
    r.add_argument('--stages', nargs='+', default=STAGES, choices=STAGES + EXTRA)
    r.add_argument('--corpora', nargs='+', default=list(CORPORA), choices=list(CORPORA), help='the generated corpora measured (default both)')
    r.add_argument('--details', nargs='+', type=int, default=[1024, 2048, 4096], choices=[1024, 2048, 4096])
    r.add_argument('--base-results', help="an earlier battery's OUT/base (or OUT/new) made by this base binary")
    for w in ('base', 'new'):
        r.add_argument(f'--{w}-cache', help=f'the cache {w} reads and fills for the IMGS corpora')
        r.add_argument(f'--{w}-disc-cache', help=f'the cache {w} reads and fills for DISC21')
    p = sub.add_parser('report')
    p.add_argument('out')
    a = ap.parse_args()
    sys.exit(run(a) if a.cmd == 'run' else report(a))


if __name__ == '__main__':
    main()
