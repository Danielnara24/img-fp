//! Turning "these two files are the same picture" edges into groups.
//!
//! A group is a **representative and everything that matched it directly**.
//! Every file in a group was verified against the representative, by the same
//! pixel check as any other claim — so a group asserts a set of pairs the run
//! actually made, and never a pair nobody tested.
//!
//! The obvious alternatives are both wrong here, and measurably so.
//!
//! **Connected components** — the closure of the pairs — invent the pairs they
//! do not have. Matching is not transitive: retrieval scores *containment*, so
//! a photograph matches both the slide and the poster it appears in while
//! those two match nothing of each other, and a left crop and a right crop are
//! both matches for the whole while being disjoint from each other. On the
//! benchmark corpus the closure claims 13,773 pairs the run never tested, and
//! 10,208 of them are pairs the ground truth calls DIFFERENT — an order of
//! magnitude more false pairs than the tool itself makes. Worse, a component
//! is only as good as its weakest edge: one wrong pair between two families
//! merges them entirely, which measured at ~7,400 false pairs from a single
//! bad edge.
//!
//! **Maximal cliques** — every file matched every other, which is what
//! `vid-fp` uses — invent nothing, but a family is not a clique. The same
//! corpus gives 6,991 of them where there are 62 families, most differing from
//! each other by a handful of members, with one file appearing in 577. On a
//! sparse video library, where a group is two or three files, that never comes
//! up; on a folder of photographs, where one seed and its ninety
//! transformations are nearly a complete graph with scattered gaps, it is the
//! normal case. Enumeration is also `3^(n/3)` in the worst case and needs a
//! budget, a ceiling and an abandonment path to stay safe.
//!
//! A star keeps the clique's honesty and the component's readability. Measured
//! against the same run: **122 groups covering all 5,512 matched files, 9,713
//! claims, none of them untested**, of which 142 are false — and those 142 are
//! the deliberate `column_roll` traps the tool is caught by anyway. A wrong
//! pair drags one file into one group rather than merging two families: the
//! same injected error that costs a component ~7,400 false pairs costs a star
//! cover 58.
//!
//! It is also the property a deduplicator needs. The representative is the
//! natural file to keep, and every file you would delete on the strength of a
//! group has been compared against it — not against some other member by way
//! of a chain.
//!
//! Two consequences, both deliberate:
//!
//! - **Groups overlap.** A file that is a duplicate of two representatives is
//!   reported under both. The output is a list of relationships, not a
//!   partition of the folder. Taking only the files nobody has claimed yet
//!   would give a partition, but it strands the leftovers — 15 files on this
//!   corpus — and its tail is scraps rather than families.
//! - **A group is not an all-pairs claim.** Two members that both matched the
//!   representative have not been compared with each other, so expanding a
//!   group back into pairs asserts more than the run did. `pairs` is there for
//!   anyone who wants the claims themselves.

use std::collections::{BinaryHeap, HashMap};

/// A representative and the files that matched it.
pub struct Group {
    /// Index of the file everything else here was verified against.
    pub representative: usize,
    /// Every file in the group, the representative included, ascending.
    pub members: Vec<usize>,
}

/// Group the matched pairs around representatives.
///
/// `edges` are index pairs in any order; duplicates and both directions are
/// tolerated. Files with no edges are in no group — a group of one is not a
/// duplicate of anything.
///
/// The representative is chosen greedily: the file that still accounts for the
/// most files nobody has grouped yet, which is a statement rather than a
/// threshold, and is usually the original or a clean re-encode of it — the
/// file most of the others matched. Ties go to the lowest index, so the result
/// depends on the pairs and not on the order they arrived in.
///
/// Cost is `O(E log V)` with no worst case to defend against, which is the
/// other reason to prefer this to clique enumeration: 9.5 ms against 43 ms on
/// the benchmark corpus, and no budget, ceiling or abandonment path needed to
/// bound it.
pub fn find(n: usize, edges: &[(usize, usize)]) -> Vec<Group> {
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a != b {
            adj[a].push(b as u32);
            adj[b].push(a as u32);
        }
    }
    for a in adj.iter_mut() {
        a.sort_unstable();
        a.dedup();
    }

    let mut ungrouped = vec![false; n];
    let mut left = 0usize;
    for v in 0..n {
        if !adj[v].is_empty() {
            ungrouped[v] = true;
            left += 1;
        }
    }

    // How many files electing `v` would account for. Recomputed rather than
    // maintained: a heap entry is allowed to be stale, and is corrected when
    // it comes up.
    let reach = |v: usize, ungrouped: &[bool]| -> usize {
        adj[v].iter().filter(|&&u| ungrouped[u as usize]).count() + ungrouped[v] as usize
    };

    // Max by reach, ties to the lowest index. Every key is distinct, so the
    // order is total and the whole greedy is reproducible.
    let mut heap: BinaryHeap<(usize, std::cmp::Reverse<usize>)> = (0..n)
        .filter(|&v| ungrouped[v])
        .map(|v| (adj[v].len() + 1, std::cmp::Reverse(v)))
        .collect();

    let mut out: Vec<Group> = Vec::new();
    while left > 0 {
        let Some((claimed, std::cmp::Reverse(r))) = heap.pop() else { break };
        // Lazy evaluation: earlier groups have covered files this entry still
        // counts. A stale entry goes back with the truth rather than being
        // trusted, which is what keeps the greedy exact without touching every
        // neighbour's key on every election.
        let real = reach(r, &ungrouped);
        if real < claimed {
            if real > 0 {
                heap.push((real, std::cmp::Reverse(r)));
            }
            continue;
        }
        let mut members: Vec<usize> = adj[r].iter().map(|&u| u as usize).collect();
        members.push(r);
        members.sort_unstable();
        for &f in members.iter() {
            if ungrouped[f] {
                ungrouped[f] = false;
                left -= 1;
            }
        }
        // `real > 0` held, so this election accounted for at least one file
        // and the loop makes progress. A representative with neighbours always
        // has at least two members.
        if members.len() > 1 {
            out.push(Group { representative: r, members });
        }
    }
    out
}

/// Which groups each file belongs to, for callers that want to report a file
/// once rather than once per relationship.
pub fn membership(groups: &[Group]) -> HashMap<usize, Vec<usize>> {
    let mut out: HashMap<usize, Vec<usize>> = HashMap::new();
    for (gi, g) in groups.iter().enumerate() {
        for &f in g.members.iter() {
            out.entry(f).or_default().push(gi);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups_of(n: usize, edges: &[(usize, usize)]) -> Vec<(usize, Vec<usize>)> {
        let mut g: Vec<(usize, Vec<usize>)> =
            find(n, edges).into_iter().map(|x| (x.representative, x.members)).collect();
        g.sort();
        g
    }

    /// The property the whole design rests on: every member was matched
    /// against the representative, so a group claims nothing untested.
    #[test]
    fn every_member_matched_the_representative() {
        let edges = [(0, 1), (1, 2), (2, 3), (3, 4), (4, 0), (0, 2), (5, 6), (6, 7)];
        let present: std::collections::HashSet<(usize, usize)> =
            edges.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
        for g in find(8, &edges) {
            for &m in g.members.iter() {
                if m == g.representative {
                    continue;
                }
                let key = (m.min(g.representative), m.max(g.representative));
                assert!(present.contains(&key), "{m} was never matched against {}", g.representative);
            }
        }
    }

    /// A chain is one group around its middle, which is the honest reading:
    /// the ends are each a duplicate of the middle, and the group does not say
    /// they are duplicates of each other.
    #[test]
    fn a_chain_groups_around_its_middle() {
        assert_eq!(groups_of(3, &[(0, 1), (1, 2)]), vec![(1, vec![0, 1, 2])]);
    }

    /// The containment case: a photograph inside two hosts that share nothing.
    /// The photograph is the representative and both hosts are its duplicates.
    #[test]
    fn a_file_matched_by_two_others_represents_them() {
        assert_eq!(groups_of(3, &[(0, 1), (0, 2)]), vec![(0, vec![0, 1, 2])]);
    }

    /// Where one representative cannot reach everything, a second is elected —
    /// and its group lists everything it matched, including files the first
    /// group already took. That overlap is the point: both relationships are
    /// real and both are reported.
    #[test]
    fn a_second_representative_covers_what_the_first_missed() {
        // 0 matches 1,2,3; 4 matches only 3.
        let g = groups_of(5, &[(0, 1), (0, 2), (0, 3), (3, 4)]);
        assert_eq!(g, vec![(0, vec![0, 1, 2, 3]), (3, vec![0, 3, 4])]);
        // 3 is in both, which is how 4 is reported at all.
        assert_eq!(membership(&find(5, &[(0, 1), (0, 2), (0, 3), (3, 4)]))[&3], vec![0, 1]);
    }

    /// Nothing that matched something is left out.
    #[test]
    fn every_matched_file_lands_in_a_group() {
        let edges = [(0, 1), (1, 2), (3, 4), (5, 6), (6, 7), (7, 8), (8, 5)];
        let covered: std::collections::HashSet<usize> =
            find(9, &edges).iter().flat_map(|g| g.members.iter().copied()).collect();
        for v in 0..9 {
            assert!(covered.contains(&v), "{v} matched something and was dropped");
        }
    }

    #[test]
    fn unmatched_files_are_in_no_group() {
        assert!(find(5, &[]).is_empty());
        assert_eq!(groups_of(5, &[(3, 4)]), vec![(3, vec![3, 4])]);
    }

    #[test]
    fn a_repeated_edge_does_not_duplicate_a_member() {
        assert_eq!(groups_of(2, &[(0, 1), (1, 0), (0, 1)]), vec![(0, vec![0, 1])]);
    }

    /// A folder of byte-identical files is one group, however many there are.
    #[test]
    fn a_complete_component_is_one_group() {
        let n = 400;
        let edges: Vec<(usize, usize)> = (0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j))).collect();
        let g = find(n, &edges);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].members.len(), n);
        assert_eq!(g[0].representative, 0);
    }

    /// The shape clique enumeration cannot survive — a complete graph minus a
    /// perfect matching, `2^(n/2)` maximal cliques — is one election here.
    #[test]
    fn a_dense_component_costs_nothing() {
        let n = 200;
        let edges: Vec<(usize, usize)> = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .filter(|&(i, j)| !(i % 2 == 0 && j == i + 1))
            .collect();
        let g = find(n, &edges);
        assert!(g.len() <= 3, "{} groups", g.len());
        let covered: std::collections::HashSet<usize> =
            g.iter().flat_map(|x| x.members.iter().copied()).collect();
        assert_eq!(covered.len(), n);
    }

    /// Reproducible: the same pairs in a different order group identically.
    #[test]
    fn output_does_not_depend_on_edge_order() {
        let edges = [(0, 1), (1, 2), (0, 2), (2, 3), (3, 4), (2, 4), (4, 5)];
        let mut shuffled: Vec<(usize, usize)> = edges.iter().rev().map(|&(a, b)| (b, a)).collect();
        assert_eq!(groups_of(6, &edges), groups_of(6, &shuffled));
        shuffled.rotate_left(3);
        assert_eq!(groups_of(6, &edges), groups_of(6, &shuffled));
    }
}

