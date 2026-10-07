//! The scan, as a child process.
//!
//! **Why a process.** Cancel has to be what Ctrl-C is on the command line:
//! immediate, whatever the scan is doing, and keeping every analysis already
//! finished. img-fp's Ctrl-C gets that by not cooperating at all: each record
//! reaches the cache the moment it exists, so the handler only waits for the
//! one append that may be in flight and then ends the process. A scan on a
//! thread could not do that — a thread cannot be stopped from outside, and
//! one inside libheif or a 40-megapixel JPEG decode would finish it first — so
//! the window runs the scan as its own binary with `--worker` and cancels it
//! with the same SIGINT a terminal sends. Its exit also hands back everything
//! it held, which is the whole of a scan's memory; the window keeps only the
//! groups.
//!
//! **What it says.** stdout carries one JSON object a line (see
//! `progress::speak_json` in the library): where the progress bar stands, and
//! each line the run would have printed. stderr is what it always is — the
//! problem summary, and the error of a run that failed. The groups come back
//! as img-fp's own JSON report, in a file the window names and removes.
//!
//! **If the window dies first**, the kernel sends the worker SIGTERM
//! (`PR_SET_PDEATHSIG`), which img-fp treats as Ctrl-C.
//!
//! **Why not `std::process::Command`.** It references `pidfd_spawnp` and
//! `pidfd_getpid` as weak symbols versioned `GLIBC_2.39`, and the loader of
//! any older glibc prints "weak version `GLIBC_2.39' not found" on every
//! start of the window (RHEL 9, Leap 15.6, Fedora 38 and 39), though nothing
//! fails. With a `pre_exec` it forks anyway, so `spawn` below is the fork it
//! made, step for step: the three `dup2`s, SIGPIPE back to its default, the
//! death signal, `execv`. GLib's spawn would do as well, but it takes its
//! arguments as UTF-8 and a path need not be.

use std::ffi::{CString, OsString};
use std::io::{BufRead, Read};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The first argument that makes the binary a scan rather than a window.
pub const WORKER_FLAG: &str = "--worker";

/// img-fp's exit codes; see `EXIT STATUS` in its man page.
pub const EXIT_FATAL: i32 = 1;
pub const EXIT_PROBLEMS: i32 = 2;
pub const EXIT_INTERRUPTED: i32 = 130;

pub enum Event {
    /// Where the bar stands, 0 to 1, and what the scan is doing.
    Progress(f64, String),
    /// A line the scan said about itself.
    Log(String),
    /// A line on its stderr.
    Stderr(String),
    /// It has ended: the exit code, or the signal that ended it.
    Exited { code: Option<i32>, signal: Option<i32> },
}

pub struct Scan {
    /// Which scan this is, so that something waiting on one scan cannot act
    /// on the next.
    pub id: u64,
    pid: i32,
    exited: Arc<AtomicBool>,
    pub result: PathBuf,
}

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
    fn fork() -> i32;
    fn dup2(old: i32, new: i32) -> i32;
    fn signal(sig: i32, handler: usize) -> usize;
    fn execv(path: *const std::ffi::c_char, argv: *const *const std::ffi::c_char) -> i32;
    fn write(fd: i32, buf: *const u8, n: usize) -> isize;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn __errno_location() -> *mut i32;
    fn _exit(code: i32) -> !;
}
const SIGINT: i32 = 2;
const SIGKILL: i32 = 9;
const SIGPIPE: i32 = 13;
const SIGTERM: i32 = 15;
const SIG_DFL: usize = 0;
const PR_SET_PDEATHSIG: i32 = 1;
const EINTR: i32 = 4;

/// What a started worker hands back: its pid and the read ends of its stdout
/// and stderr.
struct Child {
    pid: i32,
    stdout: std::io::PipeReader,
    stderr: std::io::PipeReader,
}

fn c_string(s: &std::ffi::OsStr) -> std::io::Result<CString> {
    CString::new(s.as_bytes()).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "an argument holds a NUL byte"))
}

/// Run `program` with `argv` (its first entry is the child's `argv[0]`),
/// stdin from `/dev/null` and stdout and stderr piped, as
/// `std::process::Command` does with a `pre_exec`; see the module's note.
fn spawn(program: &Path, argv: &[OsString]) -> std::io::Result<Child> {
    // Everything the child touches is made before the fork: between fork and
    // exec only async-signal-safe calls are allowed, so no allocation.
    let program = c_string(program.as_os_str())?;
    let argv = argv.iter().map(|a| c_string(a)).collect::<std::io::Result<Vec<_>>>()?;
    let mut argp: Vec<*const std::ffi::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    argp.push(std::ptr::null());
    // Every one of these is opened close-on-exec, and `dup2` clears that on
    // the copy it makes, so the child keeps its stdio and nothing else.
    let null: OwnedFd = std::fs::File::open("/dev/null")?.into();
    let (out_r, out_w) = std::io::pipe()?;
    let (err_r, err_w) = std::io::pipe()?;
    // How the child says exec failed: its errno, or nothing once exec has
    // closed the pipe.
    let (mut fail_r, fail_w) = std::io::pipe()?;
    let (null_fd, out_fd, err_fd, fail_fd) = (null.as_raw_fd(), out_w.as_raw_fd(), err_w.as_raw_fd(), fail_w.as_raw_fd());

    // SAFETY: the child runs only async-signal-safe calls on memory made
    // before the fork, and leaves by `execv` or `_exit`.
    let pid = unsafe { fork() };
    if pid == 0 {
        unsafe {
            if dup2(null_fd, 0) >= 0 && dup2(out_fd, 1) >= 0 && dup2(err_fd, 2) >= 0 {
                signal(SIGPIPE, SIG_DFL);
                prctl(PR_SET_PDEATHSIG, SIGTERM as u64, 0, 0, 0);
                execv(program.as_ptr(), argp.as_ptr());
            }
            let errno = (*__errno_location()).to_ne_bytes();
            write(fail_fd, errno.as_ptr(), errno.len());
            _exit(127);
        }
    }
    if pid < 0 {
        return Err(std::io::Error::last_os_error());
    }
    drop((null, out_w, err_w, fail_w));
    let mut errno = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match fail_r.read(&mut errno[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    if got == 4 {
        let _ = wait(pid);
        return Err(std::io::Error::from_raw_os_error(i32::from_ne_bytes(errno)));
    }
    Ok(Child { pid, stdout: out_r, stderr: err_r })
}

/// Wait for `pid` to end and reap it.
fn wait(pid: i32) -> std::io::Result<std::process::ExitStatus> {
    let mut status = 0;
    loop {
        if unsafe { waitpid(pid, &mut status, 0) } == pid {
            return Ok(std::process::ExitStatus::from_raw(status));
        }
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() != Some(EINTR) {
            return Err(e);
        }
    }
}

impl Scan {
    /// Start a scan of `argv`, an img-fp command line, and a channel of what
    /// it says. The channel ends with `Exited`.
    pub fn start(argv: &[OsString]) -> std::io::Result<(Scan, async_channel::Receiver<Event>)> {
        let result = result_path()?;
        // The running binary, through `/proc/self/exe` rather than the path
        // `current_exe` reads out of it. A package upgrade replaces the file
        // while the window is open, and the path then reads
        // `/usr/bin/img-fp-gui (deleted)`, which names nothing: every scan
        // failed to start with "No such file or directory" until the window
        // was restarted. The link itself still opens the binary this window
        // is, which is also the worker it should run.
        let exe = std::env::current_exe()?;
        let proc_exe = Path::new("/proc/self/exe");
        let program = if proc_exe.exists() { proc_exe } else { exe.as_path() };
        let mut args = vec![exe.clone().into_os_string(), WORKER_FLAG.into(), result.clone().into_os_string()];
        args.extend(argv.iter().cloned());
        let child = match spawn(program, &args) {
            Ok(c) => c,
            Err(e) => {
                let _ = result.parent().map(std::fs::remove_dir);
                return Err(e);
            }
        };
        let pid = child.pid;
        let (tx, rx) = async_channel::unbounded();
        let (out, err) = (child.stdout, child.stderr);
        let readers = [
            {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for line in std::io::BufReader::new(out).lines() {
                        let Ok(line) = line else { break };
                        if let Some(ev) = parse(&line) {
                            if tx.send_blocking(ev).is_err() {
                                break;
                            }
                        }
                    }
                })
            },
            {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for line in std::io::BufReader::new(err).lines() {
                        let Ok(line) = line else { break };
                        if tx.send_blocking(Event::Stderr(line)).is_err() {
                            break;
                        }
                    }
                })
            },
        ];
        let exited = Arc::new(AtomicBool::new(false));
        {
            let exited = exited.clone();
            std::thread::spawn(move || {
                let status = wait(pid);
                exited.store(true, Ordering::SeqCst);
                // Everything it wrote before it ended arrives before this does.
                for r in readers {
                    let _ = r.join();
                }
                let (code, signal) = match status {
                    Ok(s) => (s.code(), s.signal()),
                    Err(_) => (None, None),
                };
                let _ = tx.send_blocking(Event::Exited { code, signal });
            });
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        Ok((Scan { id, pid, exited, result }, rx))
    }

    /// Stop it, as Ctrl-C would. It answers at once and keeps what it has
    /// finished in the cache.
    pub fn interrupt(&self) {
        self.signal(SIGINT);
    }

    /// Stop it without asking: for a scan that has not answered `interrupt`,
    /// which means a disk that has stopped answering it.
    pub fn kill(&self) {
        self.signal(SIGKILL);
    }

    pub fn running(&self) -> bool {
        !self.exited.load(Ordering::SeqCst)
    }

    fn signal(&self, sig: i32) {
        // Once it has been reaped its pid may belong to something else.
        if self.running() {
            unsafe {
                kill(self.pid, sig);
            }
        }
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.result);
        // And the private folder it was written in, which is this scan's alone.
        if let Some(dir) = self.result.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

/// Where the worker writes its report: a file in a folder only this user can
/// open.
///
/// The folder is made by this process, mode 0700, and a name already taken is
/// skipped rather than used. `$XDG_RUNTIME_DIR` is private already, but the
/// fallback is `/tmp`, where the report used to go straight in under a name
/// anyone could predict — and the worker writes it with an ordinary create,
/// which follows a symlink planted there first.
fn result_path() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|d| d.is_dir()).unwrap_or_else(std::env::temp_dir);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    for _ in 0..100 {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!("img-fp-gui-{}-{n}-{nanos:x}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir.join("result.json")),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no private folder could be made for the scan's results"))
}

fn parse(line: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if let Some(l) = v.get("l").and_then(|l| l.as_str()) {
        return Some(Event::Log(l.to_string()));
    }
    let p = v.get("p")?.as_f64()?;
    let m = v.get("m").and_then(|m| m.as_str()).unwrap_or("").to_string();
    Some(Event::Progress(p, m))
}

// ---------------------------------------------------------------- the report

/// One file of one group, as img-fp's JSON report states it.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Member {
    pub path: PathBuf,
    /// The path's own bytes, which the report carries beside `path` when the
    /// name is not UTF-8 and `path` is only its readable form; see
    /// `read_report`.
    #[serde(default)]
    path_bytes: Option<Vec<u8>>,
    pub role: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
    /// How this file came to be in its group: `identical`, `same_pixels`, `direct`,
    /// `corroborated` or `propagated`; `None` for the representative.
    #[serde(default)]
    pub relation: Option<String>,
    /// How much of one of this file and the representative lies inside the
    /// other: the larger of the two ways round.
    #[serde(default)]
    pub frame_overlap: Option<f32>,
    #[serde(default)]
    pub pixel_correlation: Option<f32>,
    #[serde(default)]
    pub mirrored: Option<bool>,
    #[serde(default)]
    pub inverted: Option<bool>,
    /// The file decoded only in part: it is cut off.
    #[serde(default)]
    pub damaged: bool,
}

impl Member {
    /// The file every other member of its group was matched against.
    pub fn is_representative(&self) -> bool {
        self.role == "representative"
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct Group {
    pub files: Vec<Member>,
}

#[derive(serde::Deserialize)]
struct Report {
    groups: Vec<Group>,
    files_analysed: usize,
    // `pairs` and the rest are not read.
}

pub struct Found {
    pub groups: Vec<Group>,
    pub analysed: usize,
}

/// The groups from a finished scan's report.
///
/// No file is treated as a scan that found no images at all. img-fp writes a
/// report for that case too, so this is for a worker that ended before
/// writing one.
///
/// A member whose name is not UTF-8 takes its path from its bytes. The
/// readable form names no file, and the Trash, asked to move a file that is
/// not there, answers "not found" — which the results page counts as already
/// gone. So the window reported the file moved and left it where it was.
pub fn read_report(path: &Path) -> Result<Found, String> {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Found { groups: Vec::new(), analysed: 0 }),
        Err(e) => return Err(format!("could not read the scan's results: {e}")),
    };
    let r: Report = serde_json::from_reader(std::io::BufReader::new(f)).map_err(|e| format!("could not read the scan's results: {e}"))?;
    let mut groups = r.groups;
    for m in groups.iter_mut().flat_map(|g| g.files.iter_mut()) {
        if let Some(bytes) = m.path_bytes.take() {
            use std::os::unix::ffi::OsStringExt;
            m.path = PathBuf::from(OsString::from_vec(bytes));
        }
    }
    Ok(Found { groups, analysed: r.files_analysed })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each scan's report goes in a folder of its own that only this user can
    /// open, never straight into a shared one under a name known in advance.
    #[test]
    fn a_result_is_written_in_a_private_folder_of_its_own() {
        use std::os::unix::fs::PermissionsExt;
        let (a, b) = (result_path().unwrap(), result_path().unwrap());
        assert_ne!(a.parent(), b.parent());
        for r in [&a, &b] {
            let dir = r.parent().unwrap();
            assert_eq!(std::fs::metadata(dir).unwrap().permissions().mode() & 0o777, 0o700);
            assert!(!r.exists());
            std::fs::remove_dir(dir).unwrap();
        }
    }

    /// The window reads the report img-fp writes, and a name that is not
    /// UTF-8 comes back as the file's own name rather than its readable form.
    #[test]
    fn a_member_is_the_file_itself_whatever_its_name() {
        use std::os::unix::ffi::OsStrExt;
        let dir = std::env::temp_dir().join(format!("img-fp-gui-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let report = dir.join("r.json");
        std::fs::write(
            &report,
            r#"{"files_analysed": 2, "groups": [{"files": [
                {"path": "/p/a.jpg", "role": "representative", "width": 4, "height": 3, "size_bytes": 10},
                {"path": "/p/b\ufffd.jpg", "path_bytes": [47,112,47,98,255,46,106,112,103], "role": "match"}]}]}"#,
        )
        .unwrap();
        let found = read_report(&report).unwrap();
        let files = &found.groups[0].files;
        assert_eq!(files[0].path, PathBuf::from("/p/a.jpg"));
        assert_eq!(files[1].path.as_os_str().as_bytes(), b"/p/b\xff.jpg");
        assert_eq!(found.analysed, 2);
        assert_eq!(files[1].relation, None, "a field the report leaves out is not needed");
        std::fs::remove_dir_all(&dir).ok();
    }
}
