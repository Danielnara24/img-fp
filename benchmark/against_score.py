#!/usr/bin/env python3
"""Score an `img-fp SCAN --against LIBRARY` JSON report against the generated ground truth.

    python3 against_score.py REPORT.json [REPORT2.json ...] [--split IMGS-AGAINST/split.csv]

The corpus is IMGS-ALL split by `corpus/make_against.py`. For each scanned file the ground
truth says one of three things about the library:

  * **holds**: a library file holds all of it (`suggest_score.holds`: same seed, its region
    inside the library file's, at no less resolution, an edit never standing in for the
    original) — the right verdict is `repeat`;
  * **has**: some library file is a positive pair with it (SAME or CROP, either way) but none
    holds it — the right verdict is `better` (or `review` for an edit);
  * **none**: no library file is a positive with it — the right verdict is `new`, or `repeat`
    of another scanned file.

What is measured, per report:

  * **content lost**: a `repeat` that no file left — every library file, and every scanned
    file that is not a repeat — holds, as `suggest_score.py` counts a DELETE. Must be none but
    the content rule's known composites. Beside it, **named keeper does not hold**: the file a
    repeat names is not one that holds it, though another may (the right copy, wrongly named).
  * **false library claims**: a `repeat`, `better` or `review` resting on a library file that
    is not a positive pair with the file — a trap (same seed, scrambled or disjoint) or another
    family. `partial` (a PARTIAL pair, excluded from the pair score too) is counted apart.
  * **found**: of the files the library has (holds + has), how many get a verdict naming a
    library file that really has them; and of those it holds, how many are `repeat`.
  * the full table, truth by verdict.
"""
import collections
import csv
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from suggest_score import build, holds  # noqa: E402

CORPUS = "/home/daniel/Documents/IMGS-ALL"
SPLIT = "/home/daniel/Documents/IMGS-AGAINST/split.csv"


def key(a, b):
    return (a, b) if a <= b else (b, a)


def main():
    args = sys.argv[1:]
    split_path = SPLIT
    if "--split" in args:
        i = args.index("--split")
        split_path = args[i + 1]
        del args[i:i + 2]
    split = {r["link"]: r for r in csv.DictReader(open(split_path))}
    man = {r["path"]: r for r in csv.DictReader(open(os.path.join(CORPUS, "derived", "manifest.csv")))}
    rel = {}
    for p in csv.DictReader(open(os.path.join(CORPUS, "derived", "pairs.csv"))):
        rel[key(p["a"], p["b"])] = p["relation"]
    G = build({p: {"path": p} for p in man}, man)
    library = [r["path"] for r in split.values() if r["side"] == "library"]
    lib_by_seed = collections.defaultdict(list)
    for p in library:
        lib_by_seed[G[p]["seed"]].append(p)

    def relation(a, b):
        if G[a]["sha"] == G[b]["sha"]:
            return "SAME"
        return rel.get(key(a, b), "DIFFERENT")

    def truth(f):
        lib = lib_by_seed[G[f]["seed"]]
        if any(holds(G[k], G[f]) for k in lib):
            return "holds"
        if any(relation(f, k) in ("SAME", "CROP") for k in lib):
            return "has"
        return "none"

    scanned = [r["path"] for r in split.values() if r["side"] == "scan"]
    T = {f: truth(f) for f in scanned}
    print(f"truth over {len(scanned):,} scanned files:", dict(collections.Counter(T.values())))

    for path in args:
        rep = json.load(open(path))
        real = lambda p: split[p]["path"] if p in split else p  # noqa: E731
        rows = {real(r["path"]): r for r in rep["files"]}
        print(f"\n== {path}  ({rep.get('config', {}).get('against')}, {rep['runtime_seconds']:.1f}s)")
        missing = [f for f in scanned if f not in rows]
        if missing:
            print(f"  {len(missing)} scanned files have no row, e.g. {missing[0]}")
        table = collections.Counter()
        lost, false_claims, partial = collections.Counter(), collections.Counter(), 0
        found = holds_repeat = 0
        survivors = collections.defaultdict(list)
        for p in library:
            survivors[G[p]["seed"]].append(p)
        for f in scanned:
            if rows.get(f, {}).get("verdict") != "repeat":
                survivors[G[f]["seed"]].append(f)
        gone = collections.Counter()
        for f in scanned:
            if rows.get(f, {}).get("verdict") == "repeat" and not any(holds(G[k], G[f]) for k in survivors[G[f]["seed"]]):
                gone[G[f]["var"]] += 1
        for f in scanned:
            r = rows.get(f)
            v = r["verdict"] if r else "missing"
            table[(T[f], v)] += 1
            if not r or v == "new":
                continue
            by = real(r["match"]) if r.get("match") else None
            if by is None:
                continue
            on_library = r["match_in"] == "library"
            if v == "repeat" and not holds(G[by], G[f]):
                lost[("library" if on_library else "scan", G[f]["var"])] += 1
            if on_library:
                rr = relation(f, by)
                if rr in ("SAME", "CROP"):
                    found += 1
                elif rr == "PARTIAL":
                    partial += 1
                else:
                    kind = "cross-family" if G[f]["seed"] != G[by]["seed"] else "trap or disjoint"
                    false_claims[kind] += 1
            if v == "repeat" and T[f] == "holds":
                holds_repeat += 1
        n_has = sum(t != "none" for t in T.values())
        n_holds = sum(t == "holds" for t in T.values())
        print(f"  verdicts: {dict(collections.Counter(r['verdict'] for r in rows.values()))}")
        print(f"  found: {found:,} / {n_has:,} files the library has ({found / max(n_has, 1):.2%}) name a library file that has them")
        print(f"  repeat: {holds_repeat:,} / {n_holds:,} files a library file holds ({holds_repeat / max(n_holds, 1):.2%})")
        print(f"  content lost: {sum(gone.values())}  {dict(gone)}")
        by_side = collections.Counter(k[0] for k in lost.elements())
        print(f"  named keeper does not hold: {sum(lost.values())}  {dict(by_side)}")
        print(f"  false library claims: {sum(false_claims.values())} {dict(false_claims)}; partial: {partial}")
        print("  truth \\ verdict   " + "".join(f"{v:>9}" for v in ("repeat", "better", "review", "new", "missing")))
        for t in ("holds", "has", "none"):
            print(f"  {t:17s}" + "".join(f"{table[(t, v)]:>9,}" for v in ("repeat", "better", "review", "new", "missing")))


if __name__ == "__main__":
    main()
