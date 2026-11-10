import json, csv, sys, glob, os
seed = {}
for c in ["/home/daniel/Documents/IMGS5", "/home/daniel/Documents/IMGS-ALL"]:
    for r in csv.DictReader(open(f"{c}/derived/manifest.csv")): seed[r["path"]] = (r["seed"], r["variant"])
for f in sys.argv[1:]:
    d = json.load(open(f)); n = bad = rev = 0; ex = []
    for g in d["groups"]:
        for m in g["files"]:
            if m["action"] == "DELETE":
                n += 1; k = m.get("kept_copy")
                if k and m["path"] in seed and k in seed and seed[k][0] != seed[m["path"]][0]:
                    bad += 1; ex.append((seed[m["path"]], seed[k]))
    # each path counted once
    print(f"{os.path.basename(f):<20} DELETE rows {n:>6}  kept copy another photo: {bad}", "; ".join(f"{a[0]}:{a[1]} -> {b[0]}:{b[1]}" for a, b in ex[:4]))
