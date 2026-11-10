#!/usr/bin/env python3
"""Score the `action` column of an img-fp JSON report against the generated ground truth.

The one requirement is that a deletion loses no picture: a DELETE counts as a loss unless
some file that is not deleted holds the deleted one's content (see `holds`). Also reports how
many families keep their original, or an exact or lossless copy, and a faithful full copy; and
the same two questions without composites and mirror images or rotations counting as holders
(`strict`), which IMGS-ALL has dozens of in every family and a real library rarely has.
Every IMGS-ALL family holds lossless copies of its original, which the keep rule ranks first,
so that figure cannot see which of several lossy copies is kept: `score_provenance.py` does.

Usage: suggest_score.py REPORT.json [--manifest IMGS-ALL/derived/manifest.csv] [v]
"""
import json, math, collections, csv, sys
EDITED = {'watermark_corner','tiled_watermark','redaction_bars','blur_patch','qr_sticker','brightness','contrast',
  'saturation','greyscale','temperature','gamma','hue_rotate','invert','posterize','cinematic_grade','duotone',
  'channel_swap','perspective_top','keystone_side','shear','barrel_distort','pincushion','swirl','wave_vertical',
  'perspective','perspective_two','wave','pinch',  # IMGS-ALL2's names for the warps
  'noise','blur','motion_blur','sharpen','halftone','chromatic_aberration','vignette','print_scan','photo_of_screen',
  'wall_poster','jpeg_low','jpeg_generations','scale_nearest'}
TRAPS = {'column_roll','tile_shuffle','quadrant_rotate'}
# composites whose canvas is content of its own; blur_fill's canvas is the photo itself, blurred
HOSTS = {'browser_window','caption_bar','collage_cell','contact_sheet','embed_small','embed_tiny','magazine_spread',
  'pdf_page','phone_screenshot','picture_in_picture','slide_deck','wall_poster'}

def build(F, man):
    o = {}
    for i, f in F.items():
        m = man.get(f['path'])
        if m and m['variant'] == 'ORIGINAL': o[m['seed']] = (int(m['width']), int(m['height']))
    G = {}
    for i, f in F.items():
        m = man.get(f['path'])
        if not m: G[i] = None; continue
        W0, H0 = o[m['seed']]
        r = json.loads(m['region']); A = max((r[2]-r[0])*(r[3]-r[1]), 1e-9)
        w, h, cov = int(m['width']), int(m['height']), float(m['coverage'])
        eff = math.sqrt(cov*w*h/(A*W0*H0))
        if m['variant'] == 'aspect_distort': eff = min(w/W0, h/H0)
        G[i] = dict(seed=m['seed'], var=m['variant'], region=r, eff=min(eff, 1.0), sha=m['sha256'],
                    edited=m['variant'] in EDITED, trap=m['variant'] in TRAPS, host=m['variant'] in HOSTS)
    return G

# How far, as a fraction of the seed, a deleted file's region may reach past its holder's and
# still count as held. 0.02 is the measure every figure up to 0.79.0 was read with; it is the
# keep rule's old corner tolerance, and was blind to the deletions that tolerance let through (a
# 1-2% crop kept, its original deleted). `exact` reads the regions as generated, to a rounding.
SLACK, EXACT = 0.02, 1e-4

def holds(k, d, slack=SLACK):
    if k is None or d is None: return False
    if k['sha'] == d['sha']: return True
    if d['trap'] or d['host'] or k['trap'] or k['seed'] != d['seed']: return False
    rk, rd = k['region'], d['region']
    if rd[0] < rk[0]-slack or rd[1] < rk[1]-slack or rd[2] > rk[2]+slack or rd[3] > rk[3]+slack: return False
    if k['eff'] < d['eff']*0.95: return False
    if not d['edited'] and k['edited']: return False
    return True

# A composite or a mirror image or rotation of the picture "holds" it to `holds`, which is
# the content a deletion is measured against; in a real library there is rarely one, and
# IMGS-ALL's families have dozens. `strict` scores without them: what a deletion leaves must
# be the picture itself, the right way round.
GEOM = {'flip_h', 'rot90', 'rot180', 'transpose_diag', 'mirror_crop', 'exif_rot90'}


def losses(action, G, near, strict=False, slack=SLACK):
    """near[i]: the files i is paired with (any pair). A deletion must be held by one of them, or by
    any surviving file of its seed (the tool need not have the witnessing pair)."""
    by_seed = collections.defaultdict(list)
    for i, a in action.items():
        if a != 'DELETE' and G.get(i): by_seed[G[i]['seed']].append(i)
    out = collections.Counter(); ex = collections.defaultdict(list)
    for i, a in action.items():
        if a != 'DELETE': continue
        d = G.get(i)
        if d is None: out['not in corpus'] += 1; continue
        if any(holds(G.get(k), d, slack) and not (strict and (G[k]['host'] or G[k]['var'] in GEOM)) for k in by_seed[d['seed']]): continue
        why = 'trap' if d['trap'] else 'composite' if d['host'] else 'unedited, no unedited holder' if not d['edited'] else 'edit, no holder'
        out[why] += 1; ex[why].append(d['var'])
    return out, ex



def main():
    args = [a for a in sys.argv[1:] if a != 'v']
    manifest = '/home/daniel/Documents/IMGS-ALL/derived/manifest.csv'
    if '--manifest' in args:
        manifest = args[args.index('--manifest') + 1]
    rep = json.load(open(args[0]))
    man = {r['path']: r for r in csv.DictReader(open(manifest))}
    action = {}
    for g in rep['groups']:
        for f in g['files']:
            a = f.get('action')
            if f['path'] in action and action[f['path']] != a: print('INCONSISTENT', f['path'])
            action[f['path']] = a
    F = {p: {'path': p} for p in man}
    G = build(F, man)
    act = {p: a for p, a in action.items() if p in G}
    lo, ex = losses(act, G, None)
    print('actions', dict(collections.Counter(action.values())))
    print('content lost by deletion:', dict(lo), {k: collections.Counter(v).most_common(8) for k, v in ex.items()})
    elo, eex = losses(act, G, None, slack=EXACT)
    print('content lost, regions exact (no slack):', sum(elo.values()), dict(elo),
          {k: collections.Counter(v).most_common(8) for k, v in eex.items()})
    slo, sex = losses(act, G, None, strict=True)
    print('content lost, holders neither composites nor mirrored/rotated copies:', sum(slo.values()), dict(slo),
          {k: collections.Counter(v).most_common(8) for k, v in sex.items()})
    KEEPV = {'ORIGINAL', 'copy_exact', 'strip_metadata', 'to_png', 'to_tiff', 'webp_lossless'}
    FAITH = KEEPV | {'jpeg_high', 'jpeg_mid', 'jpeg_magick', 'jpeg_ffmpeg', 'webp_lossy', 'to_avif', 'to_heif', 'to_jxl',
                     'alpha_on_white', 'alpha_on_black', 'flip_h', 'rot90', 'rot180', 'transpose_diag', 'messaging_app'}
    seeds = collections.defaultdict(list)
    for p, g in G.items():
        if g: seeds[g['seed']].append(p)
    ok = faith = 0; miss = collections.Counter(); only_held = []
    for s, ps in seeds.items():
        surv = [p for p in ps if act.get(p) != 'DELETE']
        ok += any(G[p]['var'] in KEEPV for p in surv)
        if any(G[p]['var'] in FAITH for p in surv): faith += 1
        if not any(G[p]['var'] in FAITH - GEOM for p in surv): only_held.append(s)
        else: miss[tuple(sorted(G[p]['var'] for p in surv if act.get(p) == 'KEEP' and not G[p]['host'] and not G[p]['trap']))[:6]] += 1
    print('families whose original or exact/lossless copy survives:', ok, '/', len(seeds))
    print('families left with a faithful full copy:', faith, '/', len(seeds), miss.most_common(6))
    print('families whose picture survives only in a composite, a mirror image or rotation, or an edit:',
          len(only_held), '/', len(seeds), sorted(only_held))
    cat = collections.Counter()
    for p, a in act.items():
        g = G[p]; v = g['var']
        c = 'original/lossless copy' if v in KEEPV else 'trap' if g['trap'] else 'composite' if g['host'] else 'edit' if g['edited'] else 'unedited version'
        cat[(a, c)] += 1
    for k, v in sorted(cat.items()): print(f"{k[0]:7s} {k[1]:30s} {v}")
    if 'v' in sys.argv:
        bv = collections.defaultdict(collections.Counter)
        for p, a in act.items(): bv[G[p]['var']][a] += 1
        for v, c in sorted(bv.items()): print(f"{v:22s}", dict(c))


if __name__ == '__main__':
    main()
