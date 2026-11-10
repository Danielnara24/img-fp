#!/usr/bin/env python3
"""score1.py REPORT CORPUS -> one JSON object on stdout.

F1, precision, recall, false pairs split into traps and cross-family pairs, the
merges those imply, the seed pairs carrying them, both seed halves, perfect
transformations and the per-transform hits; for IMGS-ALL the same per corpus
(without the per-transform table) and the pairs crossing corpora. Lifted from
out/v14-fullsweep/sweep.py.
"""
import json, os, sys
from collections import defaultdict

sys.path.insert(0, "/home/daniel/Documents/Vscode_repositories/img-fp/benchmark")
import analyse

D = "/home/daniel/Documents"
CORP = {"imgs": f"{D}/IMGS", "imgs2": f"{D}/IMGS2", "imgs3": f"{D}/IMGS3",
        "imgs4": f"{D}/IMGS4", "all": f"{D}/IMGS-ALL"}


def score_set(found, t, keep=lambda s: True):
    known, seed_of, variant_of, positives, skipped, prim = t
    pos = {k for k in positives if keep(seed_of[k[0]]) and keep(seed_of[k[1]])}
    fnd = {k for k in found if keep(seed_of[k[0]]) and keep(seed_of[k[1]])}
    m = analyse.metrics(fnd, pos)
    fp = fnd - pos
    trap = sum(1 for k in fp if seed_of[k[0]] == seed_of[k[1]])
    by = defaultdict(int)
    for a, b in fp:
        if seed_of[a] != seed_of[b]:
            by[analyse.key(seed_of[a], seed_of[b])] += 1
    m.update(trap=trap, cross=sum(by.values()), merges=analyse.merge_count(set(by)),
             seedpairs={f"{a}|{b}": n for (a, b), n in sorted(by.items(), key=lambda kv: -kv[1])})
    hs = {}
    seeds = {s for s in set(seed_of.values()) if keep(s)}
    for name, half in analyse.halves(seeds):
        ph = {k for k in pos if seed_of[k[0]] in half and seed_of[k[1]] in half}
        fh = {k for k in fnd if seed_of[k[0]] in half and seed_of[k[1]] in half}
        hs[name] = round(analyse.metrics(fh, ph)["f1"], 5)
    m["halves"] = hs
    per = defaultdict(lambda: [0, 0])
    for variant, k in prim:
        if not keep(seed_of[k[0]]):
            continue
        per[variant][1] += 1
        if k in fnd:
            per[variant][0] += 1
    m["perfect"] = sum(1 for h, tt in per.values() if h == tt)
    m["per_transform"] = dict(per)
    return m


def main():
    path, corpus = sys.argv[1], sys.argv[2]
    analyse.CORPORA = [CORP[corpus]]
    m, p = analyse.load_truth()
    t = analyse.build(m, p)
    known, seed_of, _, _, skipped, _ = t
    data = json.load(open(path))
    found = {analyse.key(p["a"], p["b"]) for p in data.get("pairs", [])}
    groups = len(data.get("groups", []))
    del data
    unknown = {k for k in found if k[0] not in known or k[1] not in known}
    found = found - unknown - skipped
    out = score_set(found, t)
    out["groups"], out["unknown"] = groups, len(unknown)
    if corpus == "all":
        sub = {}
        for tag in ("IMGS", "IMGS2", "IMGS3", "IMGS4"):
            s = score_set(found, t, keep=lambda s, tag=tag: s.split("/", 1)[0] == tag)
            s.pop("per_transform")
            sub[tag] = s
        out["sub"] = sub
        out["cross_corpus"] = sum(1 for a, b in found
                                  if seed_of[a].split("/", 1)[0] != seed_of[b].split("/", 1)[0])
    json.dump(out, sys.stdout)


main()
