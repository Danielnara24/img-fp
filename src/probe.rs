//! Scaffolding (session v46): verdicts of file pairs under correspondences that
//! do not depend on a vocabulary. Not shipped.
//!
//! `PROBE_PAIRS=pairs.tsv PROBE_OUT=out.tsv cargo test --release --lib -- --ignored --nocapture probe_pairs`
//! Each input line: `kind<TAB>a<TAB>b`.

use crate::analysis::{analyse, Item, Planes};
use crate::sift::{self, Variant};
use crate::verify;
use crate::FEATURES;
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

fn line(v: &verify::Verdict) -> String {
    format!(
        "{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}",
        v.n_match, v.n_in, v.ov_a, v.ov_b, v.blk, v.blk_min, v.framed as u8, v.centred as u8
    )
}

/// Every keypoint of A against every keypoint of B: `correspond` keeps the nearest.
fn all_pairs(na: usize, nb: usize) -> Vec<(u32, u32)> {
    let mut c = Vec::with_capacity(na * nb);
    for i in 0..na as u32 {
        for j in 0..nb as u32 {
            c.push((i, j));
        }
    }
    c
}

/// p-stable LSH over the descriptors: `l` tables of `k` projections each,
/// bucket width `w`. A keypoint's entries are its `l` bucket ids.
pub(crate) fn lsh_words(f: &crate::sift::Features, k: usize, l: usize, w: f32) -> crate::index::WordList {
    let mut s = 0x2545_f491_4f6c_dd1du64;
    let mut unif = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut proj = vec![0f32; k * l * 128];
    for x in proj.iter_mut() {
        let (u1, u2) = (unif().max(1e-12), unif());
        *x = ((-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()) as f32;
    }
    let off: Vec<f32> = (0..k * l).map(|_| unif() as f32 * w).collect();
    let mut e = Vec::with_capacity(f.len() * l);
    for i in 0..f.len() {
        let d = f.d(i);
        for t in 0..l {
            let mut h = 0xcbf2_9ce4_8422_2325u64 ^ t as u64;
            for j in 0..k {
                let r = &proj[(t * k + j) * 128..][..128];
                let dot: f32 = r.iter().zip(d).map(|(a, &b)| a * b as f32).sum();
                let q = ((dot + off[t * k + j]) / w).floor() as i64;
                h = (h ^ q as u64).wrapping_mul(0x100_0000_01b3);
            }
            e.push((((h >> 20) as u32) & ((1 << 21) - 1), i as u32));
        }
    }
    e.sort_unstable();
    e.dedup();
    crate::index::WordList::from_sorted(&e)
}

#[test]
#[ignore]
fn probe_pairs() {
    let input = std::fs::read_to_string(std::env::var("PROBE_PAIRS").unwrap()).unwrap();
    let rows: Vec<Vec<String>> = input.lines().filter(|l| !l.is_empty()).map(|l| l.split('\t').map(String::from).collect()).collect();
    let mut files: Vec<String> = rows.iter().flat_map(|r| [r[1].clone(), r[2].clone()]).collect();
    files.sort();
    files.dedup();
    let sp = sift::Params { max_features: FEATURES, ..Default::default() };
    let planes = Planes::default();
    let lsh: Vec<(usize, usize, f32)> = std::env::var("PROBE_LSH")
        .map(|v| v.split(';').map(|c| { let x: Vec<&str> = c.split(',').collect(); (x[0].parse().unwrap(), x[1].parse().unwrap(), x[2].parse().unwrap()) }).collect())
        .unwrap_or_default();
    let items: HashMap<String, Item> = files.par_iter().map(|f| (f.clone(), analyse(Path::new(f), 512, &sp, None, &planes).0)).collect();
    eprintln!("analysed {}", items.len());
    let out: Vec<String> = rows
        .par_iter()
        .map(|r| {
            let (a, b) = (&items[&r[1]], &items[&r[2]]);
            let p = verify::Pair { fa: &a.feats, fb: &b.feats, ta: &a.thumb, tb: &b.thumb };
            let mut s = format!("{}\t{}\t{}", r[0], r[1], r[2]);
            if lsh.is_empty() {
                let cands = all_pairs(a.feats.len(), b.feats.len());
                let mut mt = Vec::new();
                let v = verify::verify(&p, &cands, Variant::SAME, (3, 0.2), &mut mt, &mut verify::Scratch::default());
                s += &format!("\t{}", line(&v));
                if std::env::var("PROBE_STATS").is_ok() {
                    // Over the verdict's matches (guided: all inliers): descriptor
                    // distance, residual, and the share of A's keypoints landing in B
                    // that found one.
                    let (fa, fb) = (&a.feats, &b.feats);
                    let mut dd = Vec::new();
                    let mut rr = Vec::new();
                    for &(i, j) in &mt {
                        let (ka, kb) = (&fa.kps[i as usize], &fb.kps[j as usize]);
                        let (x, y) = (v.m[0] * ka.x + v.m[1] * ka.y + v.m[2], v.m[3] * ka.x + v.m[4] * ka.y + v.m[5]);
                        rr.push(((x - kb.x).powi(2) + (y - kb.y).powi(2)).sqrt());
                        dd.push((crate::index::dist2(fa.d(i as usize).try_into().unwrap(), fb.d(j as usize)) as f32).sqrt());
                    }
                    let inside = fa.kps.iter().filter(|k| { let (x, y) = (v.m[0] * k.x + v.m[1] * k.y + v.m[2], v.m[3] * k.x + v.m[4] * k.y + v.m[5]); x >= 0.0 && y >= 0.0 && x < fb.w as f32 && y < fb.h as f32 }).count();
                    let q = |v: &mut Vec<f32>, f: f32| { if v.is_empty() { return -1.0 } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[((v.len() - 1) as f32 * f) as usize] };
                    let diag = ((fb.w * fb.w + fb.h * fb.h) as f32).sqrt();
                    let inv = { let m = v.m; let d = m[0] * m[4] - m[1] * m[3]; [m[4] / d, -m[1] / d, (m[1] * m[5] - m[4] * m[2]) / d, -m[3] / d, m[0] / d, (m[3] * m[2] - m[0] * m[5]) / d] };
                    let inside_b = fb.kps.iter().filter(|k| { let (x, y) = (inv[0] * k.x + inv[1] * k.y + inv[2], inv[3] * k.x + inv[4] * k.y + inv[5]); x >= 0.0 && y >= 0.0 && x < fa.w as f32 && y < fa.h as f32 }).count();
                    let mut bs: Vec<u32> = mt.iter().map(|p| p.1).collect();
                    bs.sort_unstable();
                    bs.dedup();
                    let cover2 = (mt.len() as f32 / inside.max(1) as f32).max(bs.len() as f32 / inside_b.max(1) as f32);
                    s += &format!("\t{}\t{}\t{:.3}\t{:.0}\t{:.0}\t{:.4}\t{:.4}\t{:.3}", mt.len(), inside, mt.len() as f32 / inside.max(1) as f32, q(&mut dd, 0.5), q(&mut dd, 0.9), q(&mut rr, 0.5) / diag, q(&mut rr, 0.9) / diag, cover2);
                }
                if std::env::var("PROBE_LOO").is_ok() {
                    // Leave-one-out local consistency: each guided match against an
                    // affine fitted to its 8 nearest matches in A; residual over
                    // the A keypoint's scale (B's scaled into A's frame via m).
                    let (fa, fb) = (&a.feats, &b.feats);
                    let lin = (v.m[0] * v.m[4] - v.m[1] * v.m[3]).abs().sqrt().max(1e-6);
                    let mut res = Vec::new();
                    let mut px = Vec::new();
                    for (k, &(i, j)) in mt.iter().enumerate() {
                        let ka = &fa.kps[i as usize];
                        let mut nb: Vec<(f32, usize)> = mt.iter().enumerate().filter(|&(o, _)| o != k).map(|(o, &(p, _))| ((fa.kps[p as usize].x - ka.x).powi(2) + (fa.kps[p as usize].y - ka.y).powi(2), o)).collect();
                        if nb.len() < 8 { break; }
                        nb.select_nth_unstable_by(7, |x, y| x.0.partial_cmp(&y.0).unwrap());
                        let mut g = [[0f64; 3]; 3];
                        let (mut tu, mut tv) = ([0f64; 3], [0f64; 3]);
                        for &(_, o) in &nb[..8] {
                            let (p, q) = mt[o];
                            let row = [(fa.kps[p as usize].x - ka.x) as f64, (fa.kps[p as usize].y - ka.y) as f64, 1.0];
                            for s in 0..3 {
                                for t in 0..3 { g[s][t] += row[s] * row[t]; }
                                tu[s] += row[s] * fb.kps[q as usize].x as f64;
                                tv[s] += row[s] * fb.kps[q as usize].y as f64;
                            }
                        }
                        let (Some(su), Some(sv)) = (solve3(g, tu), solve3(g, tv)) else { continue };
                        let kb = &fb.kps[j as usize];
                        let e = (((su[2] as f32) - kb.x).powi(2) + ((sv[2] as f32) - kb.y).powi(2)).sqrt() / lin;
                        px.push(e);
                        res.push(e / ka.sigma);
                    }
                    let q = |v: &mut Vec<f32>, f: f32| { if v.is_empty() { return -1.0 } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[((v.len() - 1) as f32 * f) as usize] };
                    s += &format!("\tO\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}", res.len(), q(&mut res, 0.5), q(&mut res, 0.75), q(&mut res, 0.9), q(&mut px, 0.5), q(&mut px, 0.9));
                }
                if std::env::var("PROBE_LOCAL").is_ok() {
                    let d2: u32 = std::env::var("PROBE_LOCAL").unwrap().parse::<u32>().unwrap().pow(2);
                    let (g, _, gm) = verify::pixel_check_field(&a.thumb, &b.thumb, &v.m, &|x, y| ap(&v.m, x, y), a.feats.w as f32, a.feats.h as f32, b.feats.w as f32, b.feats.h as f32);
                    let fwd = local(&a.feats, &b.feats, &v.m, d2);
                    let inv = verify::invert_affine(&v.m).unwrap_or(v.m);
                    let bwd = local(&b.feats, &a.feats, &inv, d2);
                    let (lf, _, lfm) = verify::pixel_check_field(&a.thumb, &b.thumb, &v.m, &|x, y| fwd.at(x, y), a.feats.w as f32, a.feats.h as f32, b.feats.w as f32, b.feats.h as f32);
                    let (lb, _, lbm) = verify::pixel_check_field(&b.thumb, &a.thumb, &inv, &|x, y| bwd.at(x, y), b.feats.w as f32, b.feats.h as f32, a.feats.w as f32, a.feats.h as f32);
                    s += &format!("\tL\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{}\t{}\t{}\t{}\t{}\t{}", g, gm, lf, lfm, lb, lbm, fwd.n0, fwd.n, fwd.inside, bwd.n0, bwd.n, bwd.inside);
                }
                if std::env::var("PROBE_NULL").is_ok() {
                    // The pair's own null: distances of A's keypoints to B's
                    // keypoints more than 3 tolerances from where A's land. And C,
                    // the window trials guided matching makes (B keypoints within
                    // the tolerance of a landing A keypoint). D_pair = the 1/C
                    // quantile of the null: one coincidental match expected.
                    let (fa, fb) = (&a.feats, &b.feats);
                    let m = if v.n_in >= 3 { v.m } else { [fb.w as f32 / fa.w as f32, 0.0, 0.0, 0.0, fb.h as f32 / fa.h as f32, 0.0] };
                    let tol = (0.015 * ((fb.w * fb.w + fb.h * fb.h) as f32).sqrt()).max(3.0);
                    let mut null = Vec::with_capacity(fa.len() * fb.len());
                    let mut win = Vec::new();
                    let mut c = 0u64;
                    for i in 0..fa.len() {
                        let ka = &fa.kps[i];
                        let (x, y) = (m[0] * ka.x + m[1] * ka.y + m[2], m[3] * ka.x + m[4] * ka.y + m[5]);
                        let da: &[u8; 128] = fa.d(i).try_into().unwrap();
                        let mut best = u32::MAX;
                        for j in 0..fb.len() {
                            let kb = &fb.kps[j];
                            let e = ((x - kb.x).powi(2) + (y - kb.y).powi(2)).sqrt();
                            let d = crate::index::dist2(da, fb.d(j));
                            if e < tol {
                                c += 1;
                                best = best.min(d);
                            } else if e > 3.0 * tol {
                                null.push(d);
                            }
                        }
                        if best != u32::MAX {
                            win.push((best as f32).sqrt());
                        }
                    }
                    null.sort_unstable();
                    let nq = |f: f64| -> f32 { if null.is_empty() { return -1.0 } (null[((null.len() - 1) as f64 * f) as usize] as f32).sqrt() };
                    let q = |v: &mut Vec<f32>, f: f32| { if v.is_empty() { return -1.0 } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[((v.len() - 1) as f32 * f) as usize] };
                    s += &format!("\tN\t{}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}", c, nq(1.0 / c.max(1) as f64), nq(1e-4), nq(1e-3), nq(1e-2), nq(0.5), q(&mut win, 0.1), q(&mut win, 0.5), q(&mut win, 0.9));
                }
                if std::env::var("PROBE_DIST").is_ok() && v.n_in >= 10 {
                    // NN distances of A's keypoints that the transform carries onto their NN.
                    let (mut inl, mut out) = (Vec::new(), Vec::new());
                    let tol = 0.015 * ((b.feats.w * b.feats.w + b.feats.h * b.feats.h) as f32).sqrt();
                    for i in 0..a.feats.len() {
                        let da: &[u8; 128] = a.feats.d(i).try_into().unwrap();
                        let (mut bd, mut bj) = (u32::MAX, 0);
                        for j in 0..b.feats.len() {
                            let d = crate::index::dist2(da, b.feats.d(j));
                            if d < bd { bd = d; bj = j; }
                        }
                        let (ka, kb) = (&a.feats.kps[i], &b.feats.kps[bj]);
                        let (x, y) = (v.m[0] * ka.x + v.m[1] * ka.y + v.m[2], v.m[3] * ka.x + v.m[4] * ka.y + v.m[5]);
                        let e = ((x - kb.x).powi(2) + (y - kb.y).powi(2)).sqrt();
                        if e < tol { inl.push((bd as f32).sqrt()) } else { out.push((bd as f32).sqrt()) }
                    }
                    let q = |v: &mut Vec<f32>, f: f32| { if v.is_empty() { return -1.0 } v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[((v.len() - 1) as f32 * f) as usize] };
                    s += &format!("\tD\t{}\t{:.0}\t{:.0}\t{:.0}\t{:.0}\t{:.0}", inl.len(), q(&mut inl, 0.5), q(&mut inl, 0.9), q(&mut inl, 0.99), q(&mut out, 0.1), q(&mut out, 0.5));
                }
            }
            for &(k, l, w) in &lsh {
                let (la, lb) = (lsh_words(&a.feats, k, l, w), lsh_words(&b.feats, k, l, w));
                let mut cands = Vec::new();
                crate::index::shared(&la, &lb, &mut cands, 1 << 30);
                let v = verify::verify(&p, &cands, Variant::SAME, (3, 0.2), &mut Vec::new(), &mut verify::Scratch::default());
                s += &format!("\t{}\t{}", cands.len(), line(&v));
            }
            s
        })
        .collect();
    let mut f = std::fs::File::create(std::env::var("PROBE_OUT").unwrap()).unwrap();
    let cols = "n_match\tn_in\tov_a\tov_b\tblk\tblk_min\tframed\tcentred";
    let mut head = "kind\ta\tb".to_string();
    if lsh.is_empty() {
        head += &format!("\t{cols}");
    }
    for (i, _) in lsh.iter().enumerate() {
        head += &format!("\tcands{i}\t{}", cols.split('\t').map(|c| format!("{c}{i}")).collect::<Vec<_>>().join("\t"));
    }
    writeln!(f, "{head}").unwrap();
    for l in out {
        writeln!(f, "{l}").unwrap();
    }
}

/// Guided matching's cost per call, single thread, features in the heap:
/// each pair verified once by brute force for a transform, then `guided` timed
/// from that transform at `PROBE_D`.
#[test]
#[ignore]
fn guided_timing() {
    let input = std::fs::read_to_string(std::env::var("PROBE_PAIRS").unwrap()).unwrap();
    let rows: Vec<Vec<String>> = input.lines().filter(|l| !l.is_empty()).map(|l| l.split('\t').map(String::from).collect()).collect();
    let mut files: Vec<String> = rows.iter().flat_map(|r| [r[1].clone(), r[2].clone()]).collect();
    files.sort();
    files.dedup();
    let sp = sift::Params { max_features: FEATURES, ..Default::default() };
    let planes = Planes::default();
    let items: HashMap<String, Item> = files.par_iter().map(|f| (f.clone(), analyse(Path::new(f), 512, &sp, None, &planes).0)).collect();
    let d: u32 = std::env::var("PROBE_D").unwrap().parse().unwrap();
    let jobs: Vec<_> = rows
        .iter()
        .map(|r| {
            let (a, b) = (&items[&r[1]], &items[&r[2]]);
            let p = verify::Pair { fa: &a.feats, fb: &b.feats, ta: &a.thumb, tb: &b.thumb };
            let v = verify::verify(&p, &all_pairs(a.feats.len(), b.feats.len()), Variant::SAME, (3, 0.2), &mut Vec::new(), &mut verify::Scratch::default());
            (r[0].clone(), &a.feats, &b.feats, v.m)
        })
        .collect();
    let mut sc = verify::Scratch::default();
    let mut pairs = Vec::new();
    for kind in ["sib", "true", "other"] {
        let js: Vec<_> = jobs.iter().filter(|j| j.0.starts_with(kind)).collect();
        let t = std::time::Instant::now();
        let mut tot = 0;
        for _ in 0..5 {
            for (_, a, b, m) in &js {
                tot += verify::guided(a, b, *m, b.w as f32, b.h as f32, d * d, &mut pairs, &mut sc).1;
            }
        }
        eprintln!("{kind}: {} pairs, {:.1} us a call, matches {}", js.len(), t.elapsed().as_secs_f64() * 1e6 / (5 * js.len()) as f64, tot / 5);
    }
}

/// Breakdown of one guided round: the grid, the scan of the 3x3 cells, the
/// distances. Single thread, unrelated and true pairs.
#[test]
#[ignore]
fn guided_parts() {
    let input = std::fs::read_to_string(std::env::var("PROBE_PAIRS").unwrap()).unwrap();
    let rows: Vec<Vec<String>> = input.lines().filter(|l| !l.is_empty()).map(|l| l.split('\t').map(String::from).collect()).collect();
    let mut files: Vec<String> = rows.iter().flat_map(|r| [r[1].clone(), r[2].clone()]).collect();
    files.sort();
    files.dedup();
    let sp = sift::Params { max_features: FEATURES, ..Default::default() };
    let planes = Planes::default();
    let items: HashMap<String, Item> = files.par_iter().map(|f| (f.clone(), analyse(Path::new(f), 512, &sp, None, &planes).0)).collect();
    let jobs: Vec<_> = rows
        .iter()
        .map(|r| {
            let (a, b) = (&items[&r[1]], &items[&r[2]]);
            let p = verify::Pair { fa: &a.feats, fb: &b.feats, ta: &a.thumb, tb: &b.thumb };
            let v = verify::verify(&p, &all_pairs(a.feats.len(), b.feats.len()), Variant::SAME, (3, 0.2), &mut Vec::new(), &mut verify::Scratch::default());
            (r[0].clone(), &a.feats, &b.feats, v.m)
        })
        .collect();
    for kind in ["true", "other"] {
        let js: Vec<_> = jobs.iter().filter(|j| j.0.starts_with(kind)).collect();
        for part in 0..3 {
            let t = std::time::Instant::now();
            let (mut land, mut scanned, mut intol, mut acc) = (0u64, 0u64, 0u64, 0u64);
            for _ in 0..5 {
                for (_, a, b, m) in &js {
                    let (bw, bh) = (b.w as f32, b.h as f32);
                    let tol = (0.015 * (bw * bw + bh * bh).sqrt()).max(3.0);
                    let tol2 = tol * tol;
                    let (nx, ny) = ((bw / tol) as usize + 1, (bh / tol) as usize + 1);
                    let mut cell_at = vec![0u32; nx * ny + 1];
                    for k in b.kps.iter() {
                        if k.x >= 0.0 && k.y >= 0.0 && ((k.x / tol) as usize) < nx && ((k.y / tol) as usize) < ny {
                            cell_at[(k.y / tol) as usize * nx + (k.x / tol) as usize + 1] += 1;
                        }
                    }
                    for c in 0..nx * ny {
                        cell_at[c + 1] += cell_at[c];
                    }
                    let mut by_cell = vec![0u32; cell_at[nx * ny] as usize];
                    let mut fill = cell_at.clone();
                    for (j, k) in b.kps.iter().enumerate() {
                        if k.x >= 0.0 && k.y >= 0.0 && ((k.x / tol) as usize) < nx && ((k.y / tol) as usize) < ny {
                            let c = (k.y / tol) as usize * nx + (k.x / tol) as usize;
                            by_cell[fill[c] as usize] = j as u32;
                            fill[c] += 1;
                        }
                    }
                    if part == 0 {
                        acc += by_cell.len() as u64;
                        continue;
                    }
                    for (i, ka) in a.kps.iter().enumerate() {
                        let (x, y) = (m[0] * ka.x + m[1] * ka.y + m[2], m[3] * ka.x + m[4] * ka.y + m[5]);
                        let (cx, cy) = ((x / tol).floor() as isize, (y / tol).floor() as isize);
                        if cx < -1 || cy < -1 || cx > nx as isize || cy > ny as isize {
                            continue;
                        }
                        land += 1;
                        let da: &[u8; 128] = a.d(i).try_into().unwrap();
                        for yy in (cy - 1).max(0)..=(cy + 1).min(ny as isize - 1) {
                            for xx in (cx - 1).max(0)..=(cx + 1).min(nx as isize - 1) {
                                let c = yy as usize * nx + xx as usize;
                                for &j in &by_cell[cell_at[c] as usize..cell_at[c + 1] as usize] {
                                    scanned += 1;
                                    let kb = &b.kps[j as usize];
                                    let (dx, dy) = (x - kb.x, y - kb.y);
                                    if dx * dx + dy * dy >= tol2 {
                                        continue;
                                    }
                                    intol += 1;
                                    if part == 2 {
                                        acc += crate::index::dist2(da, b.d(j as usize)) as u64;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let n = (5 * js.len()) as f64;
            eprintln!("{kind} part {part}: {:.1} us a call; landing {:.0} scanned {:.0} in tol {:.0} ({acc})", t.elapsed().as_secs_f64() * 1e6 / n, land as f64 / n, scanned as f64 / n, intol as f64 / n);
        }
    }
}

fn ap(m: &[f32; 6], x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}

/// A smooth correction to a transform, grown from guided matches: on a 12x12
/// lattice over A, each node's local affine (matches within a sixth of A's
/// longer side, at least 6) gives a displacement from `m`; between nodes it is
/// interpolated. Grown three times: every A keypoint is matched again where
/// the field puts it (nearest descriptor under `d2` within the tolerance), and
/// the field refitted.
struct Field {
    m: [f32; 6],
    w: f32,
    h: f32,
    d: Vec<(f32, f32)>,
    /// Matches at `m` alone, at the end, and A keypoints landing in B.
    n0: usize,
    n: usize,
    inside: usize,
}

const L: usize = 12;

impl Field {
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        let (u, v) = ap(&self.m, x, y);
        let gx = (x / self.w * (L - 1) as f32).clamp(0.0, (L - 1) as f32 - 1e-3);
        let gy = (y / self.h * (L - 1) as f32).clamp(0.0, (L - 1) as f32 - 1e-3);
        let (ix, iy) = (gx as usize, gy as usize);
        let (fx, fy) = (gx - ix as f32, gy - iy as f32);
        let d = |i: usize, j: usize| self.d[j * L + i];
        let (a, b, c, e) = (d(ix, iy), d(ix + 1, iy), d(ix, iy + 1), d(ix + 1, iy + 1));
        let dx = (a.0 * (1.0 - fx) + b.0 * fx) * (1.0 - fy) + (c.0 * (1.0 - fx) + e.0 * fx) * fy;
        let dy = (a.1 * (1.0 - fx) + b.1 * fx) * (1.0 - fy) + (c.1 * (1.0 - fx) + e.1 * fx) * fy;
        (u + dx, v + dy)
    }
}

fn guide(a: &crate::sift::Features, b: &crate::sift::Features, f: &Field, d2: u32) -> Vec<(usize, usize)> {
    let tol = (0.015 * ((b.w * b.w + b.h * b.h) as f32).sqrt()).max(3.0);
    let mut out = Vec::new();
    for i in 0..a.len() {
        let (x, y) = f.at(a.kps[i].x, a.kps[i].y);
        let da: &[u8; 128] = a.d(i).try_into().unwrap();
        let mut best = (d2, usize::MAX);
        for j in 0..b.len() {
            let kb = &b.kps[j];
            if (x - kb.x).powi(2) + (y - kb.y).powi(2) >= tol * tol {
                continue;
            }
            let d = crate::index::dist2(da, b.d(j));
            if d < best.0 {
                best = (d, j);
            }
        }
        if best.1 != usize::MAX {
            out.push((i, best.1));
        }
    }
    out
}

fn local(a: &crate::sift::Features, b: &crate::sift::Features, m: &[f32; 6], d2: u32) -> Field {
    let (w, h) = (a.w as f32, a.h as f32);
    let inside = a.kps.iter().filter(|k| { let (x, y) = ap(m, k.x, k.y); x >= 0.0 && y >= 0.0 && x < b.w as f32 && y < b.h as f32 }).count();
    let mut f = Field { m: *m, w, h, d: vec![(0.0, 0.0); L * L], n0: 0, n: 0, inside };
    let r = w.max(h) / 6.0;
    for round in 0..4 {
        let mt = guide(a, b, &f, d2);
        if round == 0 {
            f.n0 = mt.len();
        }
        f.n = mt.len();
        if round == 3 {
            break;
        }
        let mut d = vec![(0.0, 0.0); L * L];
        for j in 0..L {
            for i in 0..L {
                let (nx, ny) = (i as f32 / (L - 1) as f32 * w, j as f32 / (L - 1) as f32 * h);
                let near: Vec<_> = mt.iter().filter(|&&(p, _)| (a.kps[p].x - nx).powi(2) + (a.kps[p].y - ny).powi(2) < r * r).collect();
                if near.len() < 6 {
                    continue;
                }
                // Least squares [x y 1] -> u, v about the node.
                let mut g = [[0f64; 3]; 3];
                let (mut tu, mut tv) = ([0f64; 3], [0f64; 3]);
                for &&(p, q) in &near {
                    let row = [(a.kps[p].x - nx) as f64, (a.kps[p].y - ny) as f64, 1.0];
                    for s in 0..3 {
                        for t in 0..3 {
                            g[s][t] += row[s] * row[t];
                        }
                        tu[s] += row[s] * b.kps[q].x as f64;
                        tv[s] += row[s] * b.kps[q].y as f64;
                    }
                }
                let (Some(su), Some(sv)) = (solve3(g, tu), solve3(g, tv)) else { continue };
                let (u0, v0) = ap(m, nx, ny);
                d[j * L + i] = ((su[2] as f32) - u0, (sv[2] as f32) - v0);
            }
        }
        f.d = d;
    }
    f
}

fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = det(a);
    if d.abs() < 1e-9 {
        return None;
    }
    let mut out = [0f64; 3];
    for c in 0..3 {
        let mut m = a;
        for r in 0..3 {
            m[r][c] = b[r];
        }
        out[c] = det(m) / d;
    }
    Some(out)
}
