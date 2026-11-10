#!/usr/bin/env python3
"""Run each tool once, cold, alone, and measure what it cost.

    python3 bench.py --out out/v54-field/all

The corpus is IMGS-ALL, all four generated corpora under one root
(`corpus/make_combined.py` builds it). Tools run in the order `build_tools`
lists them: img-fp first, then the rest by how much memory they are expected
to need, so that the ones known to run out of it come last and a crash costs
nobody else's row.

One tool at a time, never two at once, with a cooldown between them. This
laptop thermally throttles — a Ryzen 7 3700U in a chassis that sits at 85 C
under load — so back-to-back runs measure the heatsink, not the tool. The
cooldown waits for a wall-clock minimum *and* for the die to come back near its
own idle temperature, and every run records the temperature and clock it
actually saw, so a throttled number is visible as a throttled number rather
than quietly polluting the baseline.

The ceiling is measured, not assumed. This machine idles at 82-86 C with a load
average of 0.5, so any fixed number like "wait for 60 C" would never be reached
and every cooldown would silently hit its timeout. Instead the harness samples
the die for 15 s before the first run and waits for baseline + 3 C thereafter.
A consequence worth stating: absolute timings from this box are not comparable
to a desktop's, because it is thermally constrained even at rest. Relative
comparisons between tools measured under identical conditions are, and that is
what a baseline is for.

## What is measured, and why it is measured this way

**Wall time** is the whole subprocess tree, start to exit.

**CPU seconds** come from `getrusage(RUSAGE_CHILDREN)` deltas: user + system
across every descendant the runner waited for. Dividing by wall time gives the
parallelism a tool actually achieved — 780% means it kept all eight threads
busy, 100% means it never left one core.

**Peak memory** is sampled from the *whole process tree*, not the parent.
`ru_maxrss` from RUSAGE_CHILDREN reports the high-water mark of the single
largest child, which for anything that forks workers is a fraction of the
truth. This walks /proc at 10 Hz, finds every descendant, and records two
totals:

  * **PSS** — proportional set size, where a page shared by four workers counts
    a quarter to each. This is the honest "how much memory did this need"
    number and the one to compare tools on.
  * **RSS** — the naive sum, which double-counts shared pages. Reported beside
    PSS because it is what most tools claim about themselves.

Sampling can miss a spike shorter than 100 ms. The alternative, ptrace-level
accounting, perturbs the thing being timed more than the error it removes.

## Cold cache

Two kinds of cache would otherwise turn a scan into a database read:

**The tool's own.** img-fp gets `--no-cache` (it keeps the per-image analysis
in ~/.cache/img-fp by default, and a cached run of this corpus is a fifth of a
cold one), czkawka `-H`, imgdupes `--no-cache`, dupeGuru a fresh temp db per
run, SSCD is denied its embeddings file. Without this the second run of a tool
is not a scan at all.

**The kernel's.** The corpus is 1.1 GB and the machine has 5.7 GB of RAM, so
after the first tool the entire corpus sits in the page cache and every later
tool reads from memory. Dropping it needs root, which is not available here —
but `posix_fadvise(POSIX_FADV_DONTNEED)` evicts named files unprivileged, and
is *better* than `drop_caches` for this: it evicts the corpus and nothing else,
so the measurement is not distorted by the OS having to page in unrelated
things it was legitimately holding.
"""
import argparse
import hashlib
import json
import os
import resource
import shlex
import shutil
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
V = os.path.join(HERE, "..", "vendor")
PY = os.path.join(V, "venv", "bin", "python")
PYT = os.path.join(V, "venv-imagededup", "bin", "python")   # the one with torch
R = os.path.join(HERE, "runners")

# Which img-fp is measured. The default is the build the release ships: plain
# x86-64, with the hot loops' x86-64-v3 copies picked at start-up. A build
# with target-cpu=native (which the repo's `.cargo/config.toml` used to ask
# for) is a v3 build with FMA on this machine, and it is *not* the shipped
# binary: jxl-grid rounds JPEG XL differently without FMA, and IMGS3 moves by
# some 1,300 pairs. Build the default with
#
#     RUSTFLAGS="-C target-cpu=x86-64" CARGO_TARGET_DIR=target/plain \
#         cargo build --release --locked --bin img-fp
#
# or pass --imgfp-bin to measure anything else (a downloaded release, a
# native build for an A/B). The binary's path, version and sha256 go into
# metrics.json with the row, so a row always says which build it is.
IMGFP = {
    "bin": os.path.join(HERE, "..", "target", "plain", "release", "img-fp"),
    "args": [],
}


# ---------------------------------------------------------------- sensors

def cpu_temp():
    """Die temperature in C, from k10temp; None if the sensor is missing."""
    for h in sorted(os.listdir("/sys/class/hwmon")):
        base = f"/sys/class/hwmon/{h}"
        try:
            with open(f"{base}/name") as fh:
                if fh.read().strip() != "k10temp":
                    continue
            with open(f"{base}/temp1_input") as fh:
                return int(fh.read().strip()) / 1000.0
        except OSError:
            continue
    return None


def cpu_mhz():
    try:
        with open("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq") as fh:
            return int(fh.read().strip()) / 1000.0
    except OSError:
        return None


# ---------------------------------------------------------------- cold cache

def evict(root):
    """Drop the corpus from the kernel page cache, without root."""
    n = bytes_ = 0
    for dirpath, _, names in os.walk(root):
        for name in names:
            path = os.path.join(dirpath, name)
            try:
                fd = os.open(path, os.O_RDONLY)
            except OSError:
                continue
            try:
                size = os.fstat(fd).st_size
                os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
                n += 1
                bytes_ += size
            except OSError:
                pass
            finally:
                os.close(fd)
    return n, bytes_


def cached_bytes():
    try:
        with open("/proc/meminfo") as fh:
            for line in fh:
                if line.startswith("Cached:"):
                    return int(line.split()[1]) * 1024
    except OSError:
        pass
    return 0


# ---------------------------------------------------------------- the sampler

def read_ppid(pid):
    try:
        with open(f"/proc/{pid}/stat", "rb") as fh:
            data = fh.read()
        # comm can contain spaces and parentheses; ppid is the field after the
        # last ')' plus the state character.
        rest = data[data.rindex(b")") + 2:].split()
        return int(rest[1])
    except (OSError, ValueError, IndexError):
        return None


def rollup(pid):
    """(rss, pss, swap, anon) in bytes for one process.

    Swap is kept because this machine runs at swappiness 100 with gigabytes
    already swapped out, so a large run is partly paged out while it runs and
    PSS alone reads low -- and reads two builds as closer than they are.

    Anon (anonymous resident pages) is kept because PSS also counts mapped
    file pages, which the kernel can drop and read again: img-fp keeps its
    analysis in an unlinked file read through a mapping (`spill.rs`), so its
    PSS stays near 2.2 GB on IMGS-ALL while what it holds is a quarter of
    that. Anon + swap is what a run cannot give back.
    """
    rss = pss = swap = anon = 0
    try:
        with open(f"/proc/{pid}/smaps_rollup") as fh:
            for line in fh:
                if line.startswith("Rss:"):
                    rss = int(line.split()[1]) * 1024
                elif line.startswith("Pss:"):
                    pss = int(line.split()[1]) * 1024
                elif line.startswith("Swap:"):
                    swap = int(line.split()[1]) * 1024
                elif line.startswith("Anonymous:"):
                    anon = int(line.split()[1]) * 1024
    except OSError:
        try:
            with open(f"/proc/{pid}/statm") as fh:
                rss = int(fh.read().split()[1]) * os.sysconf("SC_PAGE_SIZE")
                pss = rss
        except OSError:
            pass
    return rss, pss, swap, anon


class TreeSampler(threading.Thread):
    def __init__(self, root_pid, interval=0.1):
        super().__init__(daemon=True)
        self.root = root_pid
        self.interval = interval
        self.stop_flag = threading.Event()
        self.peak_rss = self.peak_pss = self.peak_pss_swap = self.peak_swap = 0
        self.peak_anon_swap = 0
        self.samples = 0
        self.max_procs = 0
        self.mhz = []

    def tree(self):
        parents = {}
        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            pid = int(entry)
            ppid = read_ppid(pid)
            if ppid is not None:
                parents[pid] = ppid
        members, frontier = {self.root}, [self.root]
        children = {}
        for pid, ppid in parents.items():
            children.setdefault(ppid, []).append(pid)
        while frontier:
            pid = frontier.pop()
            for kid in children.get(pid, ()):
                if kid not in members:
                    members.add(kid)
                    frontier.append(kid)
        return members

    def run(self):
        while not self.stop_flag.is_set():
            rss = pss = swap = anon = 0
            members = self.tree()
            for pid in members:
                r, p, w, a = rollup(pid)
                rss += r
                pss += p
                swap += w
                anon += a
            if rss:
                self.peak_rss = max(self.peak_rss, rss)
                self.peak_pss = max(self.peak_pss, pss)
                if pss + swap > self.peak_pss_swap:
                    self.peak_pss_swap, self.peak_swap = pss + swap, swap
                self.peak_anon_swap = max(self.peak_anon_swap, anon + swap)
                self.max_procs = max(self.max_procs, len(members))
                self.samples += 1
                f = cpu_mhz()
                if f:
                    self.mhz.append(f)
            self.stop_flag.wait(self.interval)


# Exit codes that mean the tool did its job. Zero for everything whose wrapper
# normalises the code (czkawka's 11-when-it-finds-duplicates never reaches
# here), and img-fp's 2 as well: it means the run finished and wrote its output
# but could not read something on the way, which on this corpus is the one
# truncated PNG and is true of every run. `exit_code` is recorded either way,
# so a row is never quietly relabelled -- this only decides whether a run is
# counted as having happened.
OK_EXIT = {"imgfp": {0, 2}}


# ---------------------------------------------------------------- one run

# `--timeout`, in seconds; None waits for ever.
TIMEOUT = {"seconds": None}


def _first_to_go():
    """In the child, before exec: be the out-of-memory killer's first choice."""
    try:
        with open("/proc/self/oom_score_adj", "w") as fh:
            fh.write("1000")
    except OSError:
        pass


def run_one(name, cmd, out_dir, corpus, cooldown, temp_ceiling, log):
    out_json = os.path.join(out_dir, f"{name}.json")
    log_path = os.path.join(out_dir, f"{name}.log")

    print(f"\n=== {name}")
    print(f"    {' '.join(str(c) for c in cmd)}")

    # 1. cool down
    waited, cooled = wait_cool(cooldown, temp_ceiling)
    if not cooled:
        print(f"    WARNING: still above {temp_ceiling:.1f} C after {waited:.0f}s")

    # 2. cold cache
    files, byts = evict(corpus)
    print(f"    evicted {files:,} files ({byts/2**30:.2f} GiB) from page cache; "
          f"cooled {waited:.0f}s")

    t_start, f_start = cpu_temp(), cpu_mhz()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)

    with open(log_path, "wb") as logfh:
        started = time.time()
        # The tool, and everything it starts, is the kernel's first choice
        # when memory runs out (`oom_score_adj` 1000, which anyone may raise
        # for their own processes): a tool that runs the machine dry then
        # costs its own row, not the desktop session beside it. In a session
        # of its own, so a run past `--timeout` is ended whole.
        proc = subprocess.Popen(cmd, stdout=logfh, stderr=subprocess.STDOUT, start_new_session=True,
                                preexec_fn=_first_to_go)
        sampler = TreeSampler(proc.pid)
        sampler.start()
        try:
            rc = proc.wait(timeout=TIMEOUT["seconds"])
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, 9)
            rc = proc.wait()
            timed_out = True
        else:
            timed_out = False
        wall = time.time() - started
        sampler.stop_flag.set()
        sampler.join(timeout=2)

    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = ((after.ru_utime - before.ru_utime)
           + (after.ru_stime - before.ru_stime))
    t_end = cpu_temp()
    mhz = sampler.mhz

    ok = rc in OK_EXIT.get(name, {0}) and os.path.exists(out_json) and os.path.getsize(out_json) > 0
    m = {
        "tool": name,
        "command": [str(c) for c in cmd],
        "ok": ok,
        "exit_code": rc,
        "wall_seconds": round(wall, 2),
        "cpu_seconds": round(cpu, 2),
        "cpu_percent": round(100 * cpu / wall, 1) if wall else 0.0,
        "peak_pss_mb": round(sampler.peak_pss / 2**20, 1),
        "peak_rss_mb": round(sampler.peak_rss / 2**20, 1),
        "peak_pss_swap_mb": round(sampler.peak_pss_swap / 2**20, 1),
        "swap_at_peak_mb": round(sampler.peak_swap / 2**20, 1),
        "peak_anon_swap_mb": round(sampler.peak_anon_swap / 2**20, 1),
        "max_processes": sampler.max_procs,
        "mem_samples": sampler.samples,
        "temp_start_c": t_start, "temp_end_c": t_end,
        "mhz_start": round(f_start) if f_start else None,
        "mhz_mean": round(sum(mhz) / len(mhz)) if mhz else None,
        "mhz_min": round(min(mhz)) if mhz else None,
        "cooldown_seconds": round(waited, 1),
        "cooled_to_ceiling": cooled,
        "page_cache_evicted_files": files,
    }
    if timed_out:
        m["timed_out_after_s"] = TIMEOUT["seconds"]
    if rc == -9 and not timed_out:
        m["killed"] = "SIGKILL: the kernel's out-of-memory killer, most likely"
    if name == "imgfp":
        m["binary"] = describe_binary(cmd[0])
    if not ok:
        with open(log_path) as fh:
            tail = fh.read()[-400:].strip()
        m["error"] = tail
        print(f"    FAILED (exit {rc}) in {wall:.1f}s")
        print(f"      {tail.splitlines()[-1][:150] if tail else ''}")
    else:
        print(f"    {wall:.1f}s wall, {cpu:.1f}s cpu ({m['cpu_percent']:.0f}%), "
              f"{m['peak_pss_mb']:.0f} MB pss ({m['peak_pss_swap_mb']:.0f} with swap, {m['peak_anon_swap_mb']:.0f} anon+swap)"
              f" / {m['peak_rss_mb']:.0f} MB rss, "
              f"{t_start:.0f}->{t_end:.0f} C")
    log.append(m)
    save_metrics(out_dir, log)
    return m


def describe_binary(path):
    """Path, `--version` and sha256 of the img-fp that made a row."""
    path = os.path.abspath(path)
    with open(path, "rb") as fh:
        digest = hashlib.sha256(fh.read()).hexdigest()
    try:
        version = subprocess.run([path, "--version"], capture_output=True,
                                 text=True, timeout=30).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        version = None
    return {"path": path, "version": version, "sha256": digest}


def save_metrics(out_dir, log):
    """Merge into whatever is already there.

    Re-running one tool after a fix must not wipe the other ten's numbers, and
    a rerun of a tool replaces its old row rather than sitting beside it.
    """
    path = os.path.join(out_dir, "metrics.json")
    existing = []
    if os.path.exists(path):
        try:
            with open(path) as fh:
                existing = json.load(fh)
        except (OSError, json.JSONDecodeError):
            existing = []
    merged, seen = [], {m["tool"] for m in log}
    for m in existing:
        if m["tool"] not in seen:
            merged.append(m)
    merged.extend(log)
    order = {name: i for i, (name, _) in enumerate(build_tools("", ""))}
    merged.sort(key=lambda m: order.get(m["tool"], 99))
    with open(path, "w") as fh:
        json.dump(merged, fh, indent=1)


def baseline_temp(seconds=15.0, interval=1.5):
    """The machine's own idle temperature, sampled rather than assumed."""
    samples = []
    end = time.time() + seconds
    while time.time() < end:
        t = cpu_temp()
        if t is not None:
            samples.append(t)
        time.sleep(interval)
    if not samples:
        return None
    return sum(samples) / len(samples)


def wait_cool(minimum, ceiling, cap=1200):
    """Sleep the minimum, then keep waiting while the die is above `ceiling`."""
    started = time.time()
    time.sleep(minimum)
    while time.time() - started < cap:
        t = cpu_temp()
        if t is None or t <= ceiling:
            return time.time() - started, True
        time.sleep(5)
    return time.time() - started, False


# ---------------------------------------------------------------- the pool

def build_tools(corpus, out_dir):
    """Every tool at its shipped defaults, plus czkawka's documented-better one."""
    fclones = os.path.join(V, "bin", "fclones")
    tools = [
        # img-fp emits the canonical shape itself, so it is invoked directly:
        # a wrapper would only add Python start-up to its measured time.
        #
        # --no-cache because img-fp caches the per-image analysis in
        # ~/.cache/img-fp by default, and every cost figure here is a cold run:
        # without it the second benchmark of the day would measure a cache
        # read and quietly report a tenth of the real time. It is the same
        # reason the page cache is evicted before each tool.
        #
        # -x '*' because one seed, `beach`, has no extension, and a walk takes
        # extensionless files only when asked to (as vid-fp's does). Every
        # figure before the -x option existed was taken with it included.
        #
        # --format json although the name already says it: the extension
        # picks the layout only by default, and the scorer needs this one.
        #
        # Which binary, and any extra options (--detail 1024, say), come
        # from --imgfp-bin and --imgfp-args; see IMGFP above.
        ("imgfp", [IMGFP["bin"], corpus,
                   "-r", "-x", "*", "--no-cache", "-o", f"{out_dir}/imgfp.json",
                   "--format", "json", *IMGFP["args"]]),
        ("fclones", [PY, os.path.join(R, "run_fclones.py"), corpus,
                     "-o", f"{out_dir}/fclones.json", "--bin", fclones]),
        ("czkawka_default", [PY, os.path.join(R, "run_czkawka.py"), corpus,
                             "-o", f"{out_dir}/czkawka_default.json"]),
        ("czkawka_tuned", [PY, os.path.join(R, "run_czkawka.py"), corpus,
                           "-z", "Lanczos3",
                           "--geometric-invariance", "mirror-flip-rotate90",
                           "-o", f"{out_dir}/czkawka_tuned.json"]),
        ("pdq", [PY, os.path.join(R, "run_pdq.py"), corpus, "-d", "31",
                 "-o", f"{out_dir}/pdq.json"]),
        ("imagededup_phash", [PYT, os.path.join(R, "run_imagededup.py"), corpus,
                              "--method", "phash", "-d", "10",
                              "-o", f"{out_dir}/imagededup_phash.json"]),
        ("imagededup_cnn", [PYT, os.path.join(R, "run_imagededup.py"), corpus,
                            "--method", "cnn", "-s", "0.9",
                            "-o", f"{out_dir}/imagededup_cnn.json"]),
        ("imgdupes_phash", [PY, os.path.join(R, "run_imgdupes.py"), corpus,
                            "--method", "phash", "-d", "4",
                            "-o", f"{out_dir}/imgdupes_phash.json"]),
        ("difpy", [PY, os.path.join(R, "run_difpy.py"), corpus,
                   "-o", f"{out_dir}/difpy.json"]),
        ("dupeguru", [PY, os.path.join(R, "run_dupeguru.py"), corpus,
                      "-t", "95", "-o", f"{out_dir}/dupeguru.json"]),
        ("findimagedupes", [PY, os.path.join(R, "run_findimagedupes.py"), corpus,
                            "-t", "90%",
                            "-o", f"{out_dir}/findimagedupes.json"]),
        # No --embeddings: the cache would make this a matrix multiply, not a run.
        ("sscd", [PYT, os.path.join(R, "run_sscd.py"), corpus, "-t", "0.6",
                  "-o", f"{out_dir}/sscd.json"]),
    ]
    by_name = dict(tools)
    return [(name, by_name[name]) for name in RUN_ORDER]


# The default order: img-fp first, then the others by expected peak memory,
# imagededup's CNN mode last (an n x n float16 similarity matrix: 12.4 GB of
# anonymous memory plus swap at 33,136 files). The 2026-10-28 baseline passed
# `--only` with the memory-heavy tools first instead, while the machine was
# freshest; with `oom_score_adj` 1000 either order costs a failing tool only
# its own row.
RUN_ORDER = ["imgfp", "fclones", "findimagedupes", "dupeguru", "imgdupes_phash",
             "czkawka_default", "imagededup_phash", "sscd", "difpy",
             "czkawka_tuned", "pdq", "imagededup_cnn"]


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--corpus", default="/home/daniel/Documents/IMGS-ALL")
    ap.add_argument("--out", default=os.path.join(HERE, "out", "v54-field", "all"))
    ap.add_argument("--cooldown", type=float, default=60.0,
                    help="minimum seconds between runs (default 60)")
    ap.add_argument("--temp-ceiling", type=float, default=None,
                    help="wait until the die is at or below this, C "
                         "(default: measured idle baseline + 3)")
    ap.add_argument("--temp-margin", type=float, default=3.0,
                    help="how far above measured idle counts as cool enough")
    ap.add_argument("--only", default=None,
                    help="comma-separated tool names, run in the order given")
    ap.add_argument("--timeout", type=float, default=None,
                    help="hours a tool may run before it is ended (default: no limit)")
    ap.add_argument("--imgfp-bin", default=IMGFP["bin"],
                    help="the img-fp binary to measure (default: the plain "
                         "x86-64 build the release ships, target/plain)")
    ap.add_argument("--imgfp-args", default="",
                    help="extra img-fp options, one shell-quoted string, "
                         "e.g. '--detail 1024'")
    args = ap.parse_args()

    IMGFP["bin"] = os.path.abspath(args.imgfp_bin)
    IMGFP["args"] = shlex.split(args.imgfp_args)

    corpus = os.path.abspath(args.corpus)
    out_dir = os.path.abspath(args.out)
    os.makedirs(out_dir, exist_ok=True)

    tools = build_tools(corpus, out_dir)
    if args.only:
        # In the order given: the tools that may run the machine out of
        # memory can be put first, while it is freshest.
        by_name = dict(tools)
        tools = [(n, by_name[n]) for n in args.only.split(",")]
    if args.timeout:
        TIMEOUT["seconds"] = args.timeout * 3600
    if any(name == "imgfp" for name, _ in tools) and not os.access(IMGFP["bin"], os.X_OK):
        sys.exit(f"no img-fp binary at {IMGFP['bin']}: build it with\n"
                 '    RUSTFLAGS="-C target-cpu=x86-64" CARGO_TARGET_DIR=target/plain '
                 "cargo build --release --locked --bin img-fp\n"
                 "or pass --imgfp-bin")

    n_files = sum(len(f) for _, _, f in os.walk(corpus))
    print(f"corpus: {corpus}, {n_files:,} files")
    print("measuring idle temperature for 15s ...")
    base = baseline_temp()
    ceiling = args.temp_ceiling
    if ceiling is None:
        ceiling = (base + args.temp_margin) if base is not None else 1e9
    print(f"idle baseline {base:.1f} C, clock {cpu_mhz():.0f} MHz"
          if base is not None else "no temperature sensor")
    print(f"{len(tools)} tools, one at a time, never two at once, "
          f"{args.cooldown:.0f}s minimum cooldown, then waiting for <= "
          f"{ceiling:.1f} C")

    log = []
    meta = {"idle_baseline_c": round(base, 1) if base else None,
            "temp_ceiling_c": round(ceiling, 1),
            "cooldown_minimum_s": args.cooldown,
            "corpus": corpus, "corpus_files": n_files,
            "cpu": "AMD Ryzen 7 3700U", "cores": os.cpu_count()}
    with open(os.path.join(out_dir, "run_meta.json"), "w") as fh:
        json.dump(meta, fh, indent=1)
    for name, cmd in tools:
        run_one(name, cmd, out_dir, corpus, args.cooldown, ceiling, log)

    print("\n" + "=" * 92)
    head = (f"{'tool':<20} {'wall':>9} {'cpu s':>9} {'cpu%':>7} "
            f"{'pss MB':>9} {'rss MB':>9} {'procs':>6} {'MHz':>6} {'temp':>10}")
    print(head)
    print("-" * len(head))
    for m in log:
        if not m["ok"]:
            print(f"{m['tool']:<20} {'FAILED':>9}  exit {m['exit_code']}")
            continue
        print(f"{m['tool']:<20} {m['wall_seconds']:>8.1f}s "
              f"{m['cpu_seconds']:>8.1f}s {m['cpu_percent']:>6.0f}% "
              f"{m['peak_pss_mb']:>9.0f} {m['peak_rss_mb']:>9.0f} "
              f"{m['max_processes']:>6} {m['mhz_mean'] or 0:>6} "
              f"{m['temp_start_c']:>4.0f}->{m['temp_end_c']:<4.0f}")
    print(f"\nmetrics -> {out_dir}/metrics.json")


if __name__ == "__main__":
    main()
