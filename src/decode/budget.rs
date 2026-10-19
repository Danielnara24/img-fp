//! How much decoding the workers may hold at once (the decode budget), and
//! the most one decoder may allocate (`max_alloc`).

use super::reduce::working_bytes;
use crate::timed;
use super::{Kind, Probe};
use anyhow::{bail, Result};
use image::ImageFormat;
use std::path::Path;

/// How much decoded image the workers may hold between them.
///
/// A decoder's output is much the largest thing this program allocates: a
/// 44-megapixel photograph is 133 MB of RGB against the 1.2 MB of working
/// image the analysis keeps of it, and the buffer lives only long enough to be
/// reduced. With one worker per thread all reaching for one at once, the peak
/// of a run was decided by how many large files happened to be adjacent in
/// directory order — an accident of the corpus's layout rather than anything
/// about the corpus.
///
/// So the decoders share a budget and a decode waits for room in it. This
/// bounds a transient; it does not limit the work. A file too large for the
/// whole budget still decodes, alone, and no file is skipped, resized or
/// treated differently for it — the output of a run cannot depend on this.
/// The figure is a fraction of what the machine reports free, because how much
/// scratch it is reasonable to hold is a fact about the machine and not about
/// the pictures.
fn decode_budget() -> usize {
    match mem_available() {
        Some(bytes) => (bytes / 8).max(64 << 20),
        // No /proc to ask: enough for several large photographs at once.
        None => 256 << 20,
    }
}

/// What the machine reports it can still hand out, in bytes: `MemAvailable`,
/// or less where the process's control group has less left (`cgroup_room`).
///
/// **`MemAvailable` is the host's**, inside a container, a `systemd-run -p
/// MemoryMax=` scope or a batch job alike, and it is not what such a process
/// may use: under a 300 MB limit on a machine with 2.2 GB free, one 1.5 MB
/// TIFF of 12000x12000 16-bit pixels passed `fits_in_memory`, and the kernel
/// killed the whole run (exit 137, no report) where the same picture should
/// have been one file's problem.
fn mem_available() -> Option<usize> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = s.lines().find(|l| l.starts_with("MemAvailable:"))?;
    let kb = line.split_whitespace().nth(1)?.parse::<usize>().ok()?;
    let host = kb.saturating_mul(1024);
    Some(cgroup_room().map_or(host, |c| c.min(host)))
}

/// How much more the control groups this process is in will let it use: the
/// least, over the group and every group above it that has a memory limit,
/// of the limit less what the group uses, its reclaimable page cache
/// (`inactive_file`) counted as free, as `MemAvailable` counts it. `None`
/// when no group has a limit, which is the ordinary desktop session.
///
/// cgroup v2 (`memory.max`, `memory.current`), and v1's memory controller
/// (`memory.limit_in_bytes`, `memory.usage_in_bytes`), whose "no limit" is a
/// number near `i64::MAX`. Inside a container the group's path reads `/`,
/// and `/sys/fs/cgroup` is the container's own group.
fn cgroup_room() -> Option<usize> {
    let own = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let read = |p: &Path| std::fs::read_to_string(p).ok().and_then(|s| s.trim().parse::<u64>().ok());
    let inactive = |dir: &Path| {
        std::fs::read_to_string(dir.join("memory.stat")).ok().and_then(|s| {
            s.lines().find_map(|l| l.strip_prefix("inactive_file ").or_else(|| l.strip_prefix("total_inactive_file ")).and_then(|v| v.trim().parse::<u64>().ok()))
        })
    };
    let mut least: Option<u64> = None;
    for line in own.lines() {
        // `hierarchy:controllers:path`.
        let mut parts = line.splitn(3, ':');
        let (Some(_), Some(controllers), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let (root, limit, usage) = if controllers.is_empty() {
            (std::path::PathBuf::from("/sys/fs/cgroup"), "memory.max", "memory.current")
        } else if controllers.split(',').any(|c| c == "memory") {
            (std::path::PathBuf::from("/sys/fs/cgroup/memory"), "memory.limit_in_bytes", "memory.usage_in_bytes")
        } else {
            continue;
        };
        let mut dir = root.join(path.trim_start_matches('/'));
        loop {
            // A v1 "unlimited" is page-rounded `i64::MAX`; nothing real is
            // within a factor of two of it.
            if let Some(max) = read(&dir.join(limit)).filter(|&m| m < u64::MAX / 4) {
                let used = read(&dir.join(usage)).unwrap_or(0).saturating_sub(inactive(&dir).unwrap_or(0));
                let room = max.saturating_sub(used);
                least = Some(least.map_or(room, |l| l.min(room)));
            }
            if dir == root || !dir.pop() || !dir.starts_with(&root) {
                break;
            }
        }
    }
    least.map(|b| b as usize)
}

/// The most a single decoder may allocate: what the machine has available
/// when the run starts.
///
/// **This replaces the `image` crate's own 512 MiB**, which was nobody's
/// decision here and refused real pictures: a 15000x12000 scan saved as TIFF
/// is 540 MB of RGB, and it was "Memory limit exceeded" — a problem, exit 2 —
/// while the same picture as JPEG or PNG decoded. The decode budget already
/// decides how much decoding happens at once, so what is left for a limit to
/// say is the one thing the budget cannot: that a picture larger than the
/// memory there is will not decode, and should be refused with a message
/// rather than end the process. A header claiming a gigapixel is exactly that
/// case on a laptop and not on a workstation, which is why it is measured and
/// not written down.
///
/// Taken once, like the budget. Without `/proc` there is nothing to measure
/// and the crate's own default stands.
pub(super) fn max_alloc() -> u64 {
    // The floor is the crate's own default, and no floor stands above what a
    // control group has left: there it is the kernel that refuses, by
    // killing the run.
    static MAX: std::sync::LazyLock<u64> = std::sync::LazyLock::new(|| {
        let floor = mem_available().map_or(512 << 20, |b| (b as u64).max(512 << 20));
        cgroup_room().map_or(floor, |c| floor.min(c as u64))
    });
    *MAX
}

/// The `image` crate's limits, with `max_alloc` in place of its own.
pub(super) fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_alloc = Some(max_alloc());
    l
}

/// Claims are served in the order they are made, so that a large one cannot be
/// starved by a stream of small ones slipping past it. Without that, a worker
/// holding a forty-megapixel photograph could wait indefinitely on a machine
/// whose budget is tight, while its seven neighbours took turns.
struct Budget {
    state: std::sync::Mutex<Queue>,
    room: std::sync::Condvar,
    limit: usize,
}

struct Queue {
    held: usize,
    issued: u64,
    serving: u64,
}

static BUDGET: std::sync::LazyLock<Budget> = std::sync::LazyLock::new(|| Budget {
    state: std::sync::Mutex::new(Queue { held: 0, issued: 0, serving: 0 }),
    room: std::sync::Condvar::new(),
    limit: decode_budget(),
});

thread_local! {
    /// How many claims this thread is already holding.
    ///
    /// It is not bookkeeping for its own sake: a worker can re-enter the
    /// budget while holding it, and a thread that waits for room only it can
    /// give back waits forever. See `reserve_inner`.
    static HELD_HERE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// A claim on the decode budget, given back when the buffers it covers die.
pub(super) struct Permit(usize);

impl Drop for Permit {
    fn drop(&mut self) {
        let mut q = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
        q.held -= self.0;
        drop(q);
        HELD_HERE.with(|h| h.set(h.get() - 1));
        BUDGET.room.notify_all();
    }
}

/// Wait for room for `bytes` of decoded image. A request larger than the whole
/// budget waits for the workers to empty and then proceeds: the alternative is
/// refusing to read a file for being big.
fn reserve(bytes: u64) -> Permit {
    timed!(29, reserve_inner(bytes))
}

thread_local! {
    /// How much of the decode under way the `Claim` it began with covers.
    static COVERED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// What a decode claims before it reads its file: the file's bytes and what
/// the header says the decoder will hold, waited for in the queue like any
/// claim.
///
/// **The file is read after the claim, not before it.** A decoder knows
/// exactly what it will hold only once it has the bytes, and the claim used
/// to be made there — so every worker waiting for room was already holding a
/// whole file. Six 75 MB BMPs peaked at 176 MB on one thread and 535 MB on
/// six, against a budget of 168: five files held while waiting for room to
/// decode them. The estimate comes from the header probe the progress bar has
/// already taken; whatever the decoder finds it needs beyond it, it takes
/// through `cover` without queueing, since this thread is holding a claim.
pub(super) struct Claim {
    _permit: Permit,
    outer: u64,
}

impl Claim {
    pub fn new(bytes: u64) -> Claim {
        let permit = reserve(bytes);
        let outer = COVERED.with(|c| c.replace(bytes));
        Claim { _permit: permit, outer }
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        COVERED.with(|c| c.set(self.outer));
    }
}

/// What a decoder will hold, now that it knows: the part its decode's `Claim`
/// did not already cover, taken at once.
pub(super) fn cover(bytes: u64) -> Permit {
    let have = COVERED.with(|c| c.replace(c.get().max(bytes)));
    reserve(bytes.saturating_sub(have))
}

/// What a decoder will hold for a picture of this header, beyond the file's
/// own bytes: the decoded buffer as each decoder lays it out, and the working
/// plane. A guess on the low side of each layout, since a guess too high holds
/// back other decodes for memory nobody takes, and one too low is made up by
/// `cover` — a PNG is read a row at a time, so its buffer is nothing unless
/// it turns out to need the general path.
pub(super) fn decode_estimate(p: &Probe, work: usize) -> u64 {
    let per_px: u64 = match p.kind {
        Kind::Image(ImageFormat::Png) => 0,
        // The luma plane, one byte a pixel (`jpeg::luma`); a JPEG with
        // no Y plane takes the rest of what it needs through `cover`.
        Kind::Image(ImageFormat::Jpeg) => 1,
        Kind::Heif => 6,
        Kind::Jxl => 24,
        Kind::Image(_) => 3,
    };
    p.w as u64 * p.h as u64 * per_px + working_bytes(p.w as usize, p.h as usize, work)
}

fn reserve_inner(bytes: u64) -> Permit {
    let want = bytes.min(isize::MAX as u64) as usize;
    // A thread already holding a claim takes what it asks for and does not
    // queue. It cannot safely wait: the room it would be waiting for is room
    // it is itself holding, and every later ticket queues behind it, so the
    // whole pool stops. That is not hypothetical — a decoder that runs its own
    // work on the global rayon pool leaves its thread free to steal another
    // image's task, and that task decodes, and so claims. The JPEG XL decode was
    // exactly this until it was given a pool of its own, and the shape is
    // one library away from coming back.
    //
    // The overshoot is bounded by the nesting depth and costs nothing that
    // matters: this bounds a transient, and a budget that hangs is worse than
    // a budget briefly exceeded.
    if HELD_HERE.with(|h| h.get()) > 0 {
        let mut q = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
        q.held += want;
        drop(q);
        HELD_HERE.with(|h| h.set(h.get() + 1));
        return Permit(want);
    }
    let mut q = BUDGET.state.lock().unwrap_or_else(|e| e.into_inner());
    let ticket = q.issued;
    q.issued += 1;
    loop {
        if q.serving == ticket && (q.held == 0 || q.held + want <= BUDGET.limit) {
            q.held += want;
            q.serving += 1;
            break;
        }
        q = BUDGET.room.wait(q).unwrap_or_else(|e| e.into_inner());
    }
    drop(q);
    HELD_HERE.with(|h| h.set(h.get() + 1));
    // The next in line may now fit in what is left.
    BUDGET.room.notify_all();
    Permit(want)
}

/// Refuse a picture whose decode would need more than `limit` bytes, at
/// `per_px` bytes a pixel — the check the image crate makes from a header
/// before it allocates, for the decoders that do not make it themselves.
pub(super) fn too_large(what: &str, w: u32, h: u32, per_px: u64, limit: u64) -> Result<()> {
    let need = (w as u64).saturating_mul(h as u64).saturating_mul(per_px);
    if need > limit {
        bail!(
            "{what}: a {w}x{h} picture needs {} MB to decode, more than the {} MB this machine has available",
            need >> 20,
            limit >> 20
        );
    }
    Ok(())
}

/// Refuse a picture whose decoded buffer would be larger than `max_alloc`,
/// before anything allocates it.
///
/// **`into_decoder` and `from_decoder` do not ask.** The crate's own
/// `ImageReader::decode` reserves the decoder's `total_bytes` against the
/// limits first (its source carries a FIXME saying the check belongs in
/// `from_decoder`); taking the decoder out to read its orientation and size
/// skipped that, so `max_alloc` was never applied to a BMP or a TIFF. An
/// 18 KB BMP whose header claimed 96 x 788 million pixels asked the
/// allocator for 227 GB, and a failed allocation aborts the process: the
/// whole scan ended, exit 134, with no file named, and every rescan ended at
/// the same file. Three of 3,000 randomly damaged small pictures did it.
pub(super) fn fits_in_memory(decoder: &impl image::ImageDecoder) -> Result<()> {
    let (w, h) = decoder.dimensions();
    let (need, limit) = (decoder.total_bytes(), max_alloc());
    if need > limit {
        bail!("a {w}x{h} picture needs {} MB to decode, more than the {} MB this machine has available", need >> 20, limit >> 20);
    }
    Ok(())
}
