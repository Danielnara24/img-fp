//! From verified matches to what a run claims: which files its anchors put in
//! one cluster (`Clusters`), and the pairs it states inside each.
//!
//! A direct verdict's `verify::Standing` says what it may do. A clean anchor
//! joins two clusters, unless it is the lone link between two groups of files
//! (`demote_bridges`); an anchor adds a file to a cluster, and joins two only
//! when every file of the smaller vouches for it (`admit`). Inside the
//! clusters those make, transforms composed along the anchors are put to the
//! pixels (`propagate`), and the weaker direct matches are corroborated:
//! each a `Claim`, and `How` it was made. Nothing after the anchors joins two
//! clusters.

use crate::verify::{self, Affine, Variant, Verdict};
use crate::Item;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A verdict between the files at two indices, from the first file's working
/// image to the second's: what every pass finds and every later stage reads.
///
/// The verdict is all of it. Its transform is the edge's (a mirror folded in
/// as a negative determinant), and its variant's inversion is the edge's.
/// Edges used to be `(usize, usize, Affine, bool, Verdict)`, the transform
/// and the inversion kept a second time beside the verdict holding them, at
/// 112 bytes against 76 for each edge of the lists held through the peak.
#[derive(Clone, Copy)]
pub(crate) struct Edge {
    i: u32,
    j: u32,
    pub(crate) v: Verdict,
}

impl Edge {
    pub(crate) fn new(a: usize, b: usize, v: Verdict) -> Edge {
        Edge { i: a as u32, j: b as u32, v }
    }

    pub(crate) fn a(&self) -> usize {
        self.i as usize
    }

    pub(crate) fn b(&self) -> usize {
        self.j as usize
    }

    pub(crate) fn ends(&self) -> (usize, usize) {
        (self.a(), self.b())
    }

    /// Whether one file is the other's negative.
    pub(crate) fn inverted(&self) -> bool {
        self.v.variant.invert
    }
}

/// How a pair came to be claimed: the report's `relation` for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum How {
    /// An anchor, believed on its own evidence.
    Direct,
    /// A transform composed through other claims and put to the pixels
    /// (`propagate`); no keypoint was matched for it.
    Propagated,
    /// A direct match that stands only between two files an anchor already
    /// put in one cluster (`Standing::Corroborating`).
    Corroborated,
}

/// A pair the run states, and how it came to.
#[derive(Clone, Copy)]
pub(crate) struct Claim {
    pub(crate) e: Edge,
    pub(crate) how: How,
}

/// The first edge for each pair of files, in the order given.
pub(crate) fn unique_pairs(edges: impl IntoIterator<Item = Edge>) -> Vec<Edge> {
    let mut seen = HashSet::new();
    edges.into_iter().filter(|e| seen.insert((e.a().min(e.b()), e.a().max(e.b())))).collect()
}

/// Files in clusters: each file's cluster, each cluster's files, and whether
/// a cluster's clean anchors close a cycle (`admit`).
///
/// A cluster is named by one of its files, and joining moves the smaller's
/// files into the larger, so a file's cluster is read without a search and a
/// file moves at most log n times.
pub(crate) struct Clusters {
    id: Vec<usize>,
    members: Vec<Vec<usize>>,
    cycle: Vec<bool>,
}

impl Clusters {
    /// `n` files, each alone.
    pub(crate) fn new(n: usize) -> Clusters {
        Clusters { id: (0..n).collect(), members: (0..n).map(|i| vec![i]).collect(), cycle: vec![false; n] }
    }

    /// `n` files, joined by `edges`.
    pub(crate) fn joined(n: usize, edges: impl IntoIterator<Item = Edge>) -> Clusters {
        let mut c = Clusters::new(n);
        for e in edges {
            c.join(e.a(), e.b());
        }
        c
    }

    /// The cluster file `x` is in.
    pub(crate) fn id(&self, x: usize) -> usize {
        self.id[x]
    }

    pub(crate) fn same(&self, a: usize, b: usize) -> bool {
        self.id[a] == self.id[b]
    }

    /// The files of cluster `c`.
    pub(crate) fn members(&self, c: usize) -> &[usize] {
        &self.members[c]
    }

    /// Every cluster, as its files.
    pub(crate) fn all(&self) -> impl Iterator<Item = &[usize]> {
        self.members.iter().filter(|m| !m.is_empty()).map(Vec::as_slice)
    }

    /// One cluster of the clusters of `a` and `b`, named for the larger (for
    /// `b`'s when they are the size); a link inside one cluster closes a cycle.
    pub(crate) fn join(&mut self, a: usize, b: usize) {
        let (mut ra, mut rb) = (self.id[a], self.id[b]);
        if ra == rb {
            self.cycle[ra] = true;
            return;
        }
        if self.members[ra].len() > self.members[rb].len() {
            std::mem::swap(&mut ra, &mut rb);
        }
        let moved = std::mem::take(&mut self.members[ra]);
        for &x in &moved {
            self.id[x] = rb;
        }
        self.members[rb].extend(moved);
        self.cycle[rb] |= self.cycle[ra];
    }
}

/// Take from the clean anchors (`clean`, by position in `edges`) every one
/// that is the only thing joining two groups of files; it stays an anchor. How
/// many were taken.
///
/// A wrong pair does not cost one pair. If it is the *only* edge between two
/// groups of files, everything on one side becomes a claimed duplicate of
/// everything on the other, and propagation will dutifully manufacture the
/// evidence: on this corpus one weak match between a dark galaxy photograph
/// and a beach photograph produced 62 false pairs downstream, and an earlier
/// one produced 354.
///
/// So an edge that is a *bridge* in the graph theory sense — its removal
/// disconnects the component — and which separates two non-trivial groups is
/// held to a much higher standard than an ordinary match, because it is
/// carrying all of them. A bridge to a single isolated file is left alone:
/// that is the ordinary case of a file matched exactly once, and it claims
/// nothing beyond itself.
///
/// It is not held to a higher bar, because "higher" was three more numbers
/// fitted to one corpus (three times the inliers, fifteen points more
/// agreement, under two octaves of gap) and a match strong enough to pass them
/// is still one match. The pairs are not lost if the two sides really are one
/// family: any second link between them stops the edge being a bridge at all.
/// And it is not thrown away: as an anchor it may still add a file to a
/// cluster and counts in the cover test, and it joins no two clusters of its
/// own. Without the test, at `--min-frame-overlap` 0.6 two photographs of
/// the docks merge for 1,545 false pairs.
pub(crate) fn demote_bridges(edges: &[Edge], clean: &mut [bool], n: usize) -> usize {
    if rule_off("bridge") {
        return 0;
    }
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n]; // (neighbour, edge index)
    for (k, e) in edges.iter().enumerate().filter(|&(k, _)| clean[k]) {
        adj[e.a()].push((e.b(), k));
        adj[e.b()].push((e.a(), k));
    }
    // Iterative Tarjan: discovery time, low-link, and the subtree size needed
    // to know how much sits on the far side of a bridge.
    let mut disc = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut size = vec![1usize; n];
    let mut timer = 0usize;
    let mut bridges: Vec<(usize, usize)> = Vec::new(); // (edge index, files below it)
    let mut stack: Vec<(usize, usize, usize)> = Vec::new(); // (node, parent edge, next neighbour)
    let mut demoted = 0;
    for s in 0..n {
        if disc[s] != usize::MAX || adj[s].is_empty() {
            continue;
        }
        disc[s] = timer;
        low[s] = timer;
        timer += 1;
        stack.push((s, usize::MAX, 0));
        while let Some(&mut (u, pe, ref mut k)) = stack.last_mut() {
            if *k < adj[u].len() {
                let (v, e) = adj[u][*k];
                *k += 1;
                if e == pe {
                    continue;
                }
                if disc[v] == usize::MAX {
                    disc[v] = timer;
                    low[v] = timer;
                    timer += 1;
                    stack.push((v, e, 0));
                } else {
                    low[u] = low[u].min(disc[v]);
                }
            } else {
                stack.pop();
                if let Some(&mut (p, _, _)) = stack.last_mut() {
                    low[p] = low[p].min(low[u]);
                    size[p] += size[u];
                    if low[u] > disc[p] {
                        bridges.push((pe, size[u]));
                    }
                }
            }
        }
        // The root's subtree is the component, so the far side of each of
        // its bridges is now known: the smaller of the two sides.
        for (e, below) in bridges.drain(..) {
            if below.min(size[s] - below) >= 2 {
                clean[e] = false;
                demoted += 1;
            }
        }
    }
    demoted
}

/// SCAFFOLDING (v46): a claim-graph rule switched off by `IMGFP_CLAIMS_OFF`
/// (comma-separated: `bridge`, `cover`, `cycle`, `clean`, which makes every
/// anchor clean).
pub(crate) fn rule_off(name: &str) -> bool {
    static OFF: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    OFF.get_or_init(|| std::env::var("IMGFP_CLAIMS_OFF").unwrap_or_default().split(',').map(str::to_owned).collect()).iter().any(|r| r == name)
}

/// The clusters the clean anchors make (`clean`, by position in `edges`),
/// with the other anchors added to them. Two clusters join on those only when
/// **every file of the smaller one has an anchor into the larger**. The
/// clusters; the edges inside them, the clean in the order given and then the
/// rest strongest first; and how many anchors were refused.
///
/// **This is the bridge test's argument, carried from one edge to several.**
/// A family is held together by many clean matches — the re-encodes, the
/// resizes, the crops of one photograph agree with each other everywhere —
/// and two families of photographs of one scene, taken a moment or a step
/// apart, touch only through weak ones: a dozen aligned points and a patch of
/// the overlap that does not agree, where someone moved or the background
/// shifted with the viewpoint. Each such match is one of hundreds between the
/// two families' copies, so there are always several, none is a bridge, and
/// the bridge test kept them all. On IMGS2 four families merged that way at
/// the shipped settings, for 21,458 false pairs.
///
/// **Why every file, and not "never join two clusters".** The stricter rule
/// was tried first and cut families apart: two variants of one photograph
/// that match each other cleanly and the family only weakly — a perspective
/// warp and a photo of a screen, a tiny embed and a contact sheet — make a
/// cluster of two, and a low-contrast family breaks into many. It cost IMGS
/// four points of recall and 43 of 45 perfect transformations. A fragment of
/// a family is matched, file by file, by the family it came from; a family of
/// ninety files is not matched file by file by the photograph taken a moment
/// later. And a lone file is the case of one: its one file has an anchor, so
/// it joins, strongest first, the cluster it matches best — after which an
/// anchor into a second cluster is a join of two clusters like any other.
/// The cover test asked of every join, clean anchors' too, was tried and is
/// worse: families were held whole by the clean ones joining unasked, and
/// without that their fragments hopped (Segovia merged, 1,059 false pairs).
///
/// **And never two clusters that each close a cycle of clean anchors.** That
/// a family is not matched file by file by its sibling was an expectation,
/// and two photographs of one scene in Segovia broke it: eighty files
/// of one, every one with a weak anchor into the other's eighty-five, and
/// sixty-six of those pointing back. Coverage cannot tell that from a
/// fragment joining its family; what can is the fragment's own evidence. A
/// fragment's clean anchors are a pair, a chain or a star, each link the only
/// one; a family's reach a file by two paths, which is the corroboration the
/// bridge test asks of a single link. Two clusters that each have it are two
/// families, whatever the weak anchors between them say — unless **one file of
/// the larger has a weak anchor to every file of the smaller**. A greyscale, a
/// duotone and a halftone of one photograph agree with each other cleanly and
/// close a triangle, and the original matches all three; no photograph of
/// Segovia was matched by all eighty of the other's. It is the cover test
/// once more, asked of a single file: the smaller cluster is then renderings
/// of something the larger holds.
pub(crate) fn admit(edges: Vec<Edge>, clean: &[bool], n: usize) -> (Clusters, Vec<Edge>, usize) {
    let mut c = Clusters::new(n);
    let (mut out, mut weak) = (Vec::new(), Vec::new());
    for (e, &is_clean) in edges.into_iter().zip(clean) {
        if is_clean {
            c.join(e.a(), e.b());
            out.push(e);
        } else {
            weak.push(e);
        }
    }
    weak.sort_by(|x, y| y.v.n_in.cmp(&x.v.n_in).then(y.v.blk.total_cmp(&x.v.blk)).then(x.ends().cmp(&y.ends())));
    let mut near: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in weak.iter() {
        near[e.a()].push(e.b());
        near[e.b()].push(e.a());
    }
    // Clusters only grow, so a join refused now can be granted after another
    // has made one side larger; the pass repeats until nothing joins.
    // Cluster pairs already found wanting, forgotten at every join since a
    // join is what can change the answer.
    let mut refused_pairs: HashSet<(usize, usize)> = HashSet::new();
    loop {
        let mut joined = false;
        for e in weak.iter() {
            let (ra, rb) = (c.id(e.a()), c.id(e.b()));
            if ra == rb {
                continue;
            }
            let (small, big) = if c.members(ra).len() <= c.members(rb).len() { (ra, rb) } else { (rb, ra) };
            if refused_pairs.contains(&(small, big)) {
                continue;
            }
            let covered = rule_off("cover") || c.members(small).iter().all(|&x| near[x].iter().any(|&y| c.id(y) == big));
            // Two clusters with clean cycles join only when one file of the
            // larger has a weak anchor to every file of the smaller: the
            // smaller is then renderings of a photograph the larger holds.
            // A weak edge is listed once, so a count is a cover.
            let admitted = covered
                && (rule_off("cycle") || !(c.cycle[ra] && c.cycle[rb]) || {
                    let k = c.members(small).len();
                    c.members(big).iter().any(|&y| near[y].iter().filter(|&&z| c.id(z) == small).count() == k)
                });
            if admitted {
                c.join(small, big);
                refused_pairs.clear();
                joined = true;
            } else {
                refused_pairs.insert((small, big));
            }
        }
        if !joined {
            break;
        }
    }
    let mut refused = 0;
    for e in weak {
        if c.same(e.a(), e.b()) {
            out.push(e);
        } else {
            refused += 1;
        }
    }
    (c, out, refused)
}

/// What a round of propagation proposed, and how.
pub(crate) struct Propagation {
    /// Every composed hypothesis that cleared the overlap floor, verdict and all.
    pub(crate) found: Vec<Edge>,
    /// The clusters the round could not afford to compare pair by pair,
    /// and compared with their root alone, each as its lowest file and its
    /// size; see `propagate`.
    pub(crate) starred: Vec<(usize, usize)>,
    /// The lowest file of each cluster the round compared pair by pair.
    pub(crate) full: Vec<usize>,
    /// Composed hypotheses put to the pixels.
    pub(crate) proposed: usize,
}

/// Within each cluster, test the pairs direct matching missed by composing
/// transforms along the edges that were found.
///
/// This is not closure expansion. A composed transform is a *hypothesis* about
/// a pair nobody matched, and it is put through the same pixel test as any
/// other, at a stricter threshold because no features vouch for it. When A is
/// a crop of B and B is a crop of C, the A-C transform is known exactly and
/// the only question is whether the pixels agree — which is cheap to answer
/// and wrong to assume. A composed pair joins two files already in one
/// cluster, so the clusters are the ones `admit` made, round after round.
///
/// **What a round may spend is `budget` hypotheses**, and the caller makes that
/// the number of candidate pairs the direct pass verified. Every pair inside a
/// cluster is quadratic in it, and everything else in a run is linear in
/// files times `-k`; a budget in those units keeps propagation the same order
/// of cost as the matching it extends, whatever one family's size. Clusters
/// are taken cheapest first, so small families — nearly all of them — are
/// never touched by it, and one that does not fit is not skipped but
/// **starred**: each of its files is compared with the root alone, which is
/// linear in the cluster. That is what a group is anyway — a
/// representative and what matched it, and the root is the best-connected
/// file, which is what `group::find` elects — so a starred family still comes
/// out as one group; what it gives up is the pairs between two non-root
/// members that direct matching missed.
///
/// It replaces a cap of two thousand files, in the first commit and never
/// derived, past which a cluster was not propagated at all. A family of
/// 2,100 variants of one photograph came out at a tenth of its pairs and as 26
/// overlapping groups, where 2,000 would have had every pair.
///
/// **The work is spread by row, not by cluster.** A cluster used to be one
/// task, so one large family ran on one thread while the rest sat idle — the
/// same 2,100 files took 76 CPU-seconds in 70 seconds of wall. A unit of work
/// is now one row of a cluster's pair triangle, or one pair of a star, and
/// the units come back in cluster order and row order, which is the order
/// the per-cluster loop produced them in.
///
/// `dirty`, when given, limits the work to the clusters holding a file it
/// marks; see the round loop in `pipeline::cluster`.
pub(crate) fn propagate(
    items: &[Item],
    edges: &[Edge],
    clusters: &Clusters,
    min_ov: f32,
    dirty: Option<&[bool]>,
    budget: usize,
) -> Propagation {
    let mut adj: Vec<Vec<(usize, Affine, bool, u32)>> = vec![Vec::new(); items.len()];
    for e in edges.iter() {
        adj[e.a()].push((e.b(), e.v.m, e.inverted(), e.v.n_in));
        if let Some(mi) = verify::invert_affine(&e.v.m) {
            adj[e.b()].push((e.a(), mi, e.inverted(), e.v.n_in));
        }
    }
    let known: HashSet<(usize, usize)> = edges.iter().map(Edge::ends).collect();
    // Pairs each cluster already holds, so that what a cluster would cost
    // is the pairs it would actually propose.
    let mut inside: HashMap<usize, usize> = HashMap::new();
    for &(a, _) in known.iter() {
        *inside.entry(clusters.id(a)).or_default() += 1;
    }

    // Cheapest first, ties to the lowest file.
    let mut costed: Vec<(usize, Vec<usize>)> = clusters
        .all()
        .filter(|c| c.len() > 2)
        .filter(|c| dirty.is_none_or(|d| c.iter().any(|&i| d[i])))
        .map(|c| {
            let k = c.len();
            let cost = k * (k - 1) / 2 - inside.get(&clusters.id(c[0])).copied().unwrap_or(0).min(k * (k - 1) / 2);
            let mut c = c.to_vec();
            c.sort_unstable();
            (cost, c)
        })
        .collect();
    costed.sort_unstable_by(|x, y| x.0.cmp(&y.0).then(x.1[0].cmp(&y.1[0])));
    let mut spent = 0usize;
    let (mut starred, mut full) = (Vec::new(), Vec::new());
    let plan: Vec<(Vec<usize>, bool)> = costed
        .into_iter()
        .map(|(cost, c)| {
            let every_pair = spent + cost <= budget;
            if every_pair {
                spent += cost;
                full.push(c[0]);
            } else {
                starred.push((c[0], c.len()));
            }
            (c, every_pair)
        })
        .collect();

    // Pose of every member relative to its cluster's root, by position in
    // the cluster.
    //
    // Breadth-first, from the best-connected member. Every hop composes
    // another transform and carries its error into the result, so the thing
    // to minimise is the number of hops, not the quality of each one: growing
    // the tree best-edge-first was tried and is measurably worse, because it
    // trades short paths for slightly better links and ends up composing more
    // of them. The root is the file most others matched, which is usually the
    // original or a clean re-encode of it.
    let posed: Vec<(usize, Vec<Option<(Affine, bool)>>)> = plan
        .par_iter()
        .map(|(comp, _)| {
            let root = *comp.iter().max_by_key(|&&i| (adj[i].len(), std::cmp::Reverse(i))).unwrap();
            let at = |v: usize| comp.binary_search(&v).ok();
            let mut pose: Vec<Option<(Affine, bool)>> = vec![None; comp.len()];
            pose[at(root).unwrap()] = Some(([1.0, 0.0, 0.0, 0.0, 1.0, 0.0], false));
            let mut queue = std::collections::VecDeque::from([root]);
            while let Some(u) = queue.pop_front() {
                let (mu, iu) = pose[at(u).unwrap()].unwrap();
                for &(v, m, inv, _) in adj[u].iter() {
                    let Some(pv) = at(v) else { continue };
                    if pose[pv].is_some() {
                        continue;
                    }
                    pose[pv] = Some((verify::compose(&mu, &m), iu ^ inv));
                    queue.push_back(v);
                }
            }
            (root, pose)
        })
        .collect();

    // One unit per row of a cluster pair by pair, one per member of a star.
    let units: Vec<(u32, u32)> =
        plan.iter().enumerate().flat_map(|(ci, (c, _))| (0..c.len() as u32).map(move |ai| (ci as u32, ai))).collect();
    // `a -> root -> b`, put to the pixels, and kept against the floor the
    // caller asked for — `--min-frame-overlap`, or a dump's lower one — and
    // no other. This used to be a fixed 0.5, which quietly overruled a
    // `--min-frame-overlap` below it for every composed pair, and threw away
    // the dump's hypotheses between 0.2 and 0.5.
    let test = |comp: &[usize], pose: &[Option<(Affine, bool)>], x: usize, y: usize| {
        let (a, b) = (comp[x], comp[y]);
        let ((ma, ia), (mb, ib)) = (pose[x]?, pose[y]?);
        let m = verify::compose(&verify::invert_affine(&ma)?, &mb);
        let var = Variant { mirror: false, invert: ia ^ ib };
        let p = verify::Pair { fa: &items[a].feats, fb: &items[b].feats, ta: &items[a].thumb, tb: &items[b].thumb };
        let v = verify::verify_transform(&p, &m, var, min_ov);
        (v.ov_a.max(v.ov_b) >= min_ov).then_some(Edge::new(a, b, v))
    };
    let proposed = AtomicUsize::new(0);
    let found = units
        .par_iter()
        .flat_map_iter(|&(ci, x)| {
            let ((comp, every_pair), (root, pose)) = (&plan[ci as usize], &posed[ci as usize]);
            let x = x as usize;
            let mut out = Vec::new();
            let mut asked = 0;
            if *every_pair {
                for y in x + 1..comp.len() {
                    if !known.contains(&(comp[x], comp[y])) {
                        asked += 1;
                        out.extend(test(comp, pose, x, y));
                    }
                }
            } else if comp[x] != *root {
                let r = comp.binary_search(root).unwrap();
                let (lo, hi) = (x.min(r), x.max(r));
                if !known.contains(&(comp[lo], comp[hi])) {
                    asked += 1;
                    out.extend(test(comp, pose, lo, hi));
                }
            }
            proposed.fetch_add(asked, Ordering::Relaxed);
            out
        })
        .collect();
    Propagation { found, starred, full, proposed: proposed.into_inner() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lone link between two clusters is a bridge however many times the
    /// run found it. Two passes verifying the same pair used to be two links,
    /// and the bridge test then kept a match it exists to drop.
    #[test]
    fn a_pair_found_twice_is_still_one_bridge() {
        let direct = Verdict { n_in: 40, ..Default::default() };
        let mirrored = Verdict { n_in: 12, ..Default::default() };
        // Two triangles {0,1,2} and {3,4,5}, joined only by 2-3: once from
        // the direct pass and once more, the other way round, from the second
        // look.
        let mut edges: Vec<Edge> =
            [(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5), (2, 3)].iter().map(|&(a, b)| Edge::new(a, b, direct)).collect();
        edges.push(Edge::new(3, 2, mirrored));
        let unique = unique_pairs(edges);
        assert_eq!(unique.len(), 7);
        assert_eq!(unique[6].v.n_in, 40, "the first verdict for a pair is the one kept");
        let mut clean = vec![true; 7];
        assert_eq!(demote_bridges(&unique, &mut clean, 6), 1);
        assert!(!clean[6], "the lone link is no longer clean");
    }

    /// Admitted with the edges of each kind given, the pairs kept, sorted.
    fn admitted(clean: &[Edge], weak: &[Edge], n: usize) -> (Vec<(usize, usize)>, usize) {
        let flags: Vec<bool> = clean.iter().map(|_| true).chain(weak.iter().map(|_| false)).collect();
        let (_, kept, refused) = admit([clean, weak].concat(), &flags, n);
        let mut pairs: Vec<(usize, usize)> = kept.iter().map(Edge::ends).collect();
        pairs.sort_unstable();
        (pairs, refused)
    }

    /// Weak anchors join a cluster only when every file of the smaller side has
    /// one into the larger: a lone file and a family's fragment join, a group
    /// with a file that does not vouch for the join does not.
    #[test]
    fn weak_anchors_join_clusters_only_when_every_file_vouches() {
        let e = |a: usize, b: usize, n_in: u32| Edge::new(a, b, Verdict { n_in, ..Default::default() });
        // Two clean clusters, {0,1,2,7} and {3,4,5}.
        let clean = vec![e(0, 1, 50), e(1, 2, 50), e(2, 7, 50), e(3, 4, 50), e(4, 5, 50)];
        // Weak links between them from three files of each, a lone file 6
        // matching both (the stronger into the second), a weak link inside
        // the first, and a fragment {8,9} of the second family, clean between
        // its two files and weakly matched by the family from both.
        let weak = vec![e(2, 3, 12), e(1, 4, 11), e(0, 6, 11), e(3, 6, 15), e(0, 2, 10), e(3, 8, 11), e(4, 9, 11)];
        let clean = [clean, vec![e(8, 9, 50)]].concat();
        let (pairs, refused) = admitted(&clean, &weak, 10);
        // 7 has no weak anchor into the second cluster, so the first cluster
        // never joins it however many of its other files do.
        assert_eq!(pairs, [(0, 1), (0, 2), (1, 2), (2, 7), (3, 4), (3, 6), (3, 8), (4, 5), (4, 9), (8, 9)]);
        assert_eq!(refused, 3);
    }

    /// Two clusters whose clean anchors each close a cycle are two families,
    /// and weak anchors do not join them even file for file in both
    /// directions — unless one file vouches for the whole of the smaller; a
    /// chain matched the same way still joins.
    #[test]
    fn weak_anchors_join_two_clusters_with_clean_cycles_only_through_one_file() {
        let e = |a: usize, b: usize, n_in: u32| Edge::new(a, b, Verdict { n_in, ..Default::default() });
        // Triangles {0,1,2} and {3,4,5}; a chain {6,7,8}.
        let clean = vec![e(0, 1, 50), e(1, 2, 50), e(0, 2, 50), e(3, 4, 50), e(4, 5, 50), e(3, 5, 50), e(6, 7, 50), e(7, 8, 50)];
        // Every file of each triangle has a weak anchor into the other, and
        // every file of the chain one into the first triangle.
        let weak = vec![e(0, 3, 12), e(1, 4, 12), e(2, 5, 12), e(6, 0, 11), e(7, 1, 11), e(8, 2, 11)];
        let (pairs, refused) = admitted(&clean, &weak, 9);
        assert_eq!(pairs, [(0, 1), (0, 2), (1, 2), (3, 4), (3, 5), (4, 5), (6, 0), (6, 7), (7, 1), (7, 8), (8, 2)]);
        assert_eq!(refused, 3);
        // One file of the first triangle matching every file of the second,
        // as an original matches its renderings, does join them.
        // The first is given a fourth file so that it is the larger.
        let clean = vec![e(0, 1, 50), e(1, 2, 50), e(0, 2, 50), e(2, 6, 50), e(3, 4, 50), e(4, 5, 50), e(3, 5, 50)];
        let weak = vec![e(0, 3, 12), e(0, 4, 12), e(0, 5, 12)];
        let (pairs, refused) = admitted(&clean, &weak, 7);
        assert_eq!((pairs.len(), refused), (10, 0));
    }

    /// A round proposes every unmatched pair of a cluster it can afford,
    /// cheapest cluster first, and compares the files of one it cannot with
    /// the root alone — it never passes one over.
    #[test]
    fn a_cluster_over_the_budget_is_starred_not_skipped() {
        // A chain over `files`, which leaves every pair but the links to ask.
        let chain = |files: std::ops::Range<usize>| files.clone().skip(1).map(|b| Edge::new(b - 1, b, Verdict::default())).collect::<Vec<_>>();
        let n = 30;
        let items: Vec<Item> = (0..n).map(|_| Item::default()).collect();
        // {0..10}: 45 pairs, 9 known, 36 to ask. {10..30}: 190 pairs, 19 known, 171.
        let edges = [chain(0..10), chain(10..30)].concat();
        let clusters = Clusters::joined(n, edges.iter().copied());
        let p = propagate(&items, &edges, &clusters, 0.0, None, 1_000);
        assert_eq!((p.proposed, p.found.len(), p.starred.len()), (36 + 171, 36 + 171, 0));
        // A budget for the small one alone: the large one is starred — its
        // root is the second file of the chain, the first of the best
        // connected, and it is asked about the 18 files it has no link to.
        let p = propagate(&items, &edges, &clusters, 0.0, None, 100);
        assert_eq!(p.starred, vec![(10, 20)]);
        assert_eq!(p.full, vec![0]);
        assert_eq!(p.proposed, 36 + 17);
        assert!(p.found.iter().filter(|e| e.a() >= 10).all(|e| e.a() == 11 || e.b() == 11), "a star is all through its root");
        // No budget at all: both are starred, and still asked.
        let p = propagate(&items, &edges, &clusters, 0.0, None, 0);
        assert_eq!((p.starred.len(), p.proposed), (2, 7 + 17));
        // Starred in one round, and alone in the next, it fits the budget and
        // is compared pair by pair: which is why the run's note is decided by
        // a cluster's last round and not its first.
        let p1 = propagate(&items, &edges, &clusters, 0.0, None, 180);
        assert_eq!(p1.starred, vec![(10, 20)]);
        let mut dirty = vec![false; n];
        for e in &p1.found {
            dirty[e.a()] = true;
            dirty[e.b()] = true;
        }
        let pool = [edges.clone(), p1.found.clone()].concat();
        let p2 = propagate(&items, &pool, &clusters, 0.0, Some(&dirty), 180);
        assert!(p2.starred.is_empty() && p2.full.contains(&10), "{:?} {:?}", p2.starred, p2.full);
        // The order is the plan's, whatever order the work finished in.
        let again = propagate(&items, &edges, &clusters, 0.0, None, 1_000);
        let pairs = |p: &Propagation| p.found.iter().map(Edge::ends).collect::<Vec<_>>();
        assert_eq!(pairs(&again), pairs(&propagate(&items, &edges, &clusters, 0.0, None, 1_000)));
        let mut sorted = pairs(&again);
        sorted.sort_unstable();
        assert_eq!(pairs(&again), sorted, "cluster by cluster, row by row");
    }
}
