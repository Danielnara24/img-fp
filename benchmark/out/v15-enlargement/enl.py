#!/usr/bin/env python3
"""The enlargement sweep, on the build where a picture the working size shrank
is never enlarged. usage: enl.py rules | small"""
import sys, os, re, json, subprocess, time
E = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(E))
import sweep

sweep.BIN = f"{E}/img-fp-fix"
sweep.RES = f"{E}/results.jsonl"
sweep.S = E
os.makedirs(f"{E}/out", exist_ok=True)
os.makedirs(f"{E}/logs", exist_ok=True)
os.makedirs(f"{E}/cache", exist_ok=True)
sweep.done = set()
if os.path.exists(sweep.RES):
    for line in open(sweep.RES):
        sweep.done.add(json.loads(line)["key"])

# name: (upsample_below, max factor); None leaves the build's default
RULES = {
    "t512": ("512", None),          # the shipped rule, on the fixed build
    "none": ("0", None),
    "t256": ("256", None),
    "t384": ("384", None),
    "t768": ("768", None),
    "t1024": ("1024", None),
    "t512c2": ("512", "2"),
    "t512c4": ("512", "4"),
    "t1024c2": ("1024", "2"),
    "t1024c4": ("1024", "4"),
    "tws": ("ws", None),             # enlarge towards the working size
    "dbl": ("1000000000", "2"),      # Lowe: double every picture once
}


def env(rule):
    below, cap = RULES[rule]
    os.environ["IMGFP_UPSAMPLE_BELOW"] = below
    if cap:
        os.environ["IMGFP_MAX_UPSAMPLE"] = cap
    else:
        os.environ.pop("IMGFP_MAX_UPSAMPLE", None)


def found(rule):
    key = f"enl_{rule}_found_384"
    if key in sweep.done:
        return
    log = f"{E}/logs/{key}.log"
    cmd = ["/usr/bin/time", "-f", "TIME %e %U %S %M", sweep.BIN, "/home/daniel/Downloads/archive",
           "-r", "-x", "*", "--no-cache", "-o", "/dev/null", "--format", "json"]
    with open(log, "w") as fh:
        rc = subprocess.run(cmd, stdout=fh, stderr=subprocess.STDOUT).returncode
    txt = open(log).read()
    m = re.search(r"(\d+) groups, (\d+) pairs over (\d+) images", txt)
    tm = re.search(r"TIME ([\d.]+) ([\d.]+) ([\d.]+) (\d+)", txt)
    row = dict(key=key, corpus="found", ws=384, rule=rule, rc=rc,
               groups=int(m[1]) if m else None, pairs=int(m[2]) if m else None,
               wall=float(tm[1]), cpu=float(tm[2]) + float(tm[3]), maxrss_mb=int(tm[4]) // 1024)
    with open(sweep.RES, "a") as fh:
        fh.write(json.dumps(row) + "\n")
    sweep.done.add(key)
    print(f"{time.strftime('%H:%M:%S')} {key:<34} pairs {row['pairs']} groups {row['groups']} "
          f"| {row['wall']:.0f}s {row['cpu']:.0f}cpu", flush=True)


def block(tag, ws, rule):
    cache = f"{E}/cache/c_{tag}_{ws}.bin"
    for c in ("imgs", "imgs2", "imgs3"):
        sweep.run(f"{tag}_{c}_{ws}_cold", c, ws, cache, {})
    sweep.run(f"{tag}_all_{ws}", "all", ws, cache, {})
    if os.path.exists(cache):
        os.remove(cache)


if __name__ == "__main__":
    phase = sys.argv[1]
    if phase == "rules":
        for rule in RULES:
            env(rule)
            found(rule)
            for ws in (384, 640):
                block(f"enl_{rule}", ws, rule)
    elif phase == "small":
        env("t512")
        for ws in (16, 32, 48, 64, 96, 128, 192, 224, 240, 256, 288, 320):
            block("fix", ws, "t512")
    print(f"PHASE {phase} DONE", flush=True)
