//! The analysis, kept in a scratch file rather than in the heap.
//!
//! **What a run holds is mostly its analysis.** Each picture's descriptors,
//! keypoints and thumbnail are some 70 KB, and every one of them is held from
//! the moment it is made to the end of matching, because any pair may be
//! verified: on 27,000 pictures that was 2 GB of a 2.3 GB peak, all of it
//! anonymous memory, which the kernel can only swap. A library of a hundred
//! thousand photographs did not fit a machine with eight gigabytes.
//!
//! So each record's arrays are written, as they are made, to a file nobody
//! else can see — created unlinked, in the cache's folder — and read back
//! through a read-only mapping of it. The bytes are the bytes the heap held,
//! at the same alignment, so everything that reads them reads what it read
//! before and the output is the same to the byte. What changes is what the
//! memory *is*: pages of a file, which the kernel writes out and drops when it
//! needs the room and reads in again when they are touched. On a machine with
//! room to spare nothing is dropped and the only cost is one write a record;
//! on one without, the cost is reading back what was dropped, where the heap
//! would have swapped or been killed.
//!
//! Nothing here is required. A folder that cannot hold the file, a disk that
//! fills, a platform without the calls: each record that cannot be written
//! stays in the heap, as every record did before.

use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

/// A type whose values are their bytes: no padding, no pointers, and any byte
/// pattern a value. Only these are written to the file.
///
/// # Safety
/// Implementors must be `#[repr(C)]` (or primitive) plain data with no padding.
pub unsafe trait Plain: Copy + Send + Sync + 'static {}
unsafe impl Plain for u8 {}

/// An array that lives in the heap or in the scratch file, read as a slice
/// either way.
pub struct Shared<T: Plain> {
    ptr: *const T,
    len: usize,
    owner: Owner<T>,
}

enum Owner<T> {
    Heap(#[allow(dead_code)] Vec<T>),
    Mapped(#[allow(dead_code)] Arc<Chunk>),
}

// SAFETY: the slice is immutable for the whole life of the value, and what
// keeps it alive (a `Vec` or a mapping) is itself `Send` and `Sync`.
unsafe impl<T: Plain> Send for Shared<T> {}
unsafe impl<T: Plain> Sync for Shared<T> {}

impl<T: Plain> std::ops::Deref for Shared<T> {
    type Target = [T];
    #[inline(always)]
    fn deref(&self) -> &[T] {
        // SAFETY: `ptr` points at `len` values owned by `owner`: a `Vec`'s
        // buffer, which does not move when the `Vec` does, or a mapping held
        // open by the `Arc`.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl<'a, T: Plain> IntoIterator for &'a Shared<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: Plain> From<Vec<T>> for Shared<T> {
    fn from(v: Vec<T>) -> Self {
        Shared { ptr: v.as_ptr(), len: v.len(), owner: Owner::Heap(v) }
    }
}

impl<T: Plain> FromIterator<T> for Shared<T> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Self {
        it.into_iter().collect::<Vec<T>>().into()
    }
}

impl<T: Plain + PartialEq> PartialEq<Shared<T>> for Vec<T> {
    fn eq(&self, other: &Shared<T>) -> bool {
        self[..] == **other
    }
}

impl<T: Plain> Default for Shared<T> {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl<T: Plain> Clone for Shared<T> {
    fn clone(&self) -> Self {
        match &self.owner {
            Owner::Heap(v) => v.clone().into(),
            Owner::Mapped(c) => Shared { ptr: self.ptr, len: self.len, owner: Owner::Mapped(c.clone()) },
        }
    }
}

impl<T: Plain + std::fmt::Debug> std::fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: Plain + PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: Plain + PartialEq> PartialEq<Vec<T>> for Shared<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        **self == other[..]
    }
}

impl<T: Plain> Shared<T> {
    /// Whether the values are in the scratch file rather than the heap.
    #[cfg(test)]
    pub fn is_mapped(&self) -> bool {
        matches!(self.owner, Owner::Mapped(_))
    }
}

// ---------------------------------------------------------------- the file

unsafe extern "C" {
    fn mmap(addr: *mut std::ffi::c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut std::ffi::c_void;
    fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
}
const PROT_READ: i32 = 1;
const MAP_SHARED: i32 = 1;

/// A read-only mapping of one stretch of the file, unmapped when the last
/// array in it goes.
struct Chunk {
    base: *mut u8,
    len: usize,
}

// SAFETY: a read-only mapping, never written through.
unsafe impl Send for Chunk {}
unsafe impl Sync for Chunk {}

impl Drop for Chunk {
    fn drop(&mut self) {
        // SAFETY: the mapping `mmap` made, and nothing refers to it any more.
        unsafe {
            munmap(self.base as *mut _, self.len);
        }
    }
}

/// How much of the file one mapping covers. Large enough that a library of a
/// million pictures is a few thousand mappings, far under the kernel's limit;
/// small enough that the end of the last one is not worth thinking about.
const CHUNK: usize = 64 << 20;

/// Where a record starts in its chunk: a cache line, which is more than any
/// `Plain` type needs and what the heap gave the descriptors anyway.
const ALIGN: usize = 64;

/// One scratch file and the chunk being filled.
struct Store {
    file: File,
    state: Mutex<Fill>,
}

struct Fill {
    chunk: Option<Arc<Chunk>>,
    /// The chunk's offset in the file, and how much of it is taken.
    at: u64,
    used: usize,
    /// The file's length: where the next chunk starts.
    end: u64,
}

impl Store {
    fn create(dir: &Path) -> std::io::Result<Store> {
        Ok(Store { file: unlinked_file(dir)?, state: Mutex::new(Fill { chunk: None, at: 0, used: 0, end: 0 }) })
    }

    /// `bytes` written to the file, and where the mapping holds them; `None`
    /// when they could not be, and they stay where they are.
    fn put(&self, bytes: &[u8]) -> Option<(Arc<Chunk>, *const u8)> {
        use std::os::unix::fs::FileExt;
        use std::os::fd::AsRawFd;
        let n = bytes.len();
        let (chunk, offset, file_at) = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let fits = s.chunk.as_ref().is_some_and(|c| s.used + n <= c.len);
            if !fits {
                let len = n.div_ceil(CHUNK).max(1) * CHUNK;
                let at = s.end;
                self.file.set_len(at + len as u64).ok()?;
                // SAFETY: a fresh read-only shared mapping of a range the file
                // now covers; `at` is a multiple of `CHUNK`, so of the page.
                let base = unsafe { mmap(std::ptr::null_mut(), len, PROT_READ, MAP_SHARED, self.file.as_raw_fd(), at as i64) };
                if base as isize == -1 {
                    return None;
                }
                s.chunk = Some(Arc::new(Chunk { base: base as *mut u8, len }));
                s.at = at;
                s.used = 0;
                s.end = at + len as u64;
            }
            let chunk = s.chunk.clone()?;
            let offset = s.used;
            s.used = (s.used + n).next_multiple_of(ALIGN);
            (chunk, offset, s.at + offset as u64)
        };
        // Written through the file, never the mapping: a disk that fills is an
        // error here, where through a mapping it would be a SIGBUS.
        self.file.write_all_at(bytes, file_at).ok()?;
        // SAFETY: `offset + n` is inside the chunk, by `fits` or by its size.
        let ptr = unsafe { chunk.base.add(offset) } as *const u8;
        Some((chunk, ptr))
    }
}

/// A file in `dir` that no other process can open and that is gone, space
/// and all, once this one closes it and unmaps the last of it — however the
/// process ends.
fn unlinked_file(dir: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    #[cfg(target_os = "linux")]
    {
        // O_TMPFILE: never has a name at all.
        const O_TMPFILE: i32 = 0o20000000 | 0o200000;
        if let Ok(f) = std::fs::OpenOptions::new().read(true).write(true).mode(0o600).custom_flags(O_TMPFILE).open(dir) {
            return Ok(f);
        }
    }
    // A filesystem without it: a name nobody else holds, removed at once.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    for _ in 0..100 {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = dir.join(format!(".img-fp-scratch-{}-{n}", std::process::id()));
        match std::fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(&path) {
            Ok(f) => {
                let _ = std::fs::remove_file(&path);
                return Ok(f);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no scratch file could be made"))
}

// ---------------------------------------------------------------- the run's

/// What an array is, which decides how long it lives: the features are let go
/// once matching is done, and the thumbnails are read to the end. Kept in
/// files of their own, so that the features' pages go with them rather than
/// sharing mappings with thumbnails that stay.
#[derive(Clone, Copy)]
pub enum Kind {
    Features = 0,
    Thumbs = 1,
}

static STORES: RwLock<[Option<Arc<Store>>; 2]> = RwLock::new([None, None]);

/// Start keeping arrays in scratch files in `dir`.
pub fn begin(dir: &Path) -> std::io::Result<()> {
    let made = [Arc::new(Store::create(dir)?), Arc::new(Store::create(dir)?)];
    *STORES.write().unwrap_or_else(|e| e.into_inner()) = made.map(Some);
    Ok(())
}

/// Stop: arrays already written stay readable for as long as they are held.
pub fn end() {
    *STORES.write().unwrap_or_else(|e| e.into_inner()) = [None, None];
}

/// `v`, moved to the scratch file when there is one and it takes it, else as
/// it is.
pub fn share<T: Plain>(kind: Kind, v: Shared<T>) -> Shared<T> {
    if v.is_empty() || matches!(v.owner, Owner::Mapped(_)) {
        return v;
    }
    let Some(store) = STORES.read().unwrap_or_else(|e| e.into_inner())[kind as usize].clone() else { return v };
    // SAFETY: `T: Plain` is its bytes.
    let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(&v[..])) };
    match store.put(bytes) {
        Some((chunk, ptr)) => Shared { ptr: ptr as *const T, len: v.len(), owner: Owner::Mapped(chunk) },
        None => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What goes in comes back, from the file, however many chunks it takes;
    /// and without a store, or for nothing at all, it stays in the heap.
    #[test]
    fn arrays_come_back_from_the_file_as_they_went_in() {
        let dir = std::env::temp_dir().join(format!("img-fp-spill-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::create(&dir).unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "the file has no name");
        let put = |v: Vec<u8>| -> Shared<u8> {
            let (chunk, ptr) = store.put(&v).unwrap();
            assert_eq!(ptr as usize % ALIGN, 0);
            Shared { ptr, len: v.len(), owner: Owner::Mapped(chunk) }
        };
        let small: Vec<u8> = (0..1000).map(|i| (i * 7) as u8).collect();
        let big: Vec<u8> = (0..CHUNK + 5).map(|i| (i % 251) as u8).collect();
        let (a, b, c) = (put(small.clone()), put(big.clone()), put(small.clone()));
        assert!(a.is_mapped() && b.is_mapped());
        assert_eq!((a, b.clone(), c), (small.clone().into(), big.into(), small.into()));
        drop(store);
        assert_eq!(b[CHUNK + 4], ((CHUNK + 4) % 251) as u8, "readable after the store is gone");
        assert!(!share(Kind::Thumbs, Vec::<u8>::new().into()).is_mapped());
        std::fs::remove_dir_all(&dir).ok();
    }
}
