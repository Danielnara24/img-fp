"""score_disc.py GT PRED.csv [--refs ID_LIST]: DISC21 micro-AP and recall at
90% precision, as the isc2021 evaluation computes them.

PRED.csv rows are `query_id,reference_id,score` (no header). The ground truth
is the isc2021 CSV (`Q00003,R326040`, an empty reference for a query with no
match). With --refs (and --queries), only ground-truth pairs whose reference is in the list
count: a shard is scored against the matches it could have found.
"""
import argparse
import csv


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("gt")
    ap.add_argument("pred")
    ap.add_argument("--refs", help="file of reference ids the run held, one a line")
    ap.add_argument("--queries", help="file of query ids the run scanned, one a line")
    a = ap.parse_args()

    refs = None
    if a.refs:
        refs = {l.strip() for l in open(a.refs) if l.strip()}
    qs = None
    if a.queries:
        qs = {l.strip() for l in open(a.queries) if l.strip()}
    gt = set()
    for row in csv.reader(open(a.gt)):
        if len(row) >= 2 and row[1] and row[0] != "query_id":
            if (refs is None or row[1] in refs) and (qs is None or row[0] in qs):
                gt.add((row[0], row[1]))

    best = {}
    for row in csv.reader(open(a.pred)):
        q, r, s = row[0], row[1], float(row[2])
        if best.get((q, r), -1e30) < s:
            best[(q, r)] = s
    preds = sorted(best.items(), key=lambda kv: -kv[1])

    tp = 0
    ap_sum = 0.0
    r_at_p90 = 0.0
    for k, ((q, r), s) in enumerate(preds, 1):
        if (q, r) in gt:
            tp += 1
            ap_sum += tp / k
        if tp / k >= 0.9:
            r_at_p90 = max(r_at_p90, tp / len(gt))
    n = len(gt)
    print(f"ground-truth pairs {n}, predictions {len(preds)}, true {tp}, false {len(preds) - tp}")
    print(f"micro-AP {ap_sum / n:.4f}  recall {tp / n:.4f}  recall@P90 {r_at_p90:.4f}")


if __name__ == "__main__":
    main()
