//! HEIC, HEIF and AVIF, through the system's libheif — loaded when the first
//! such file is met, not linked.
//!
//! **Linked, libheif was needed to start at all.** A binary whose ELF header
//! names `libheif.so.1` is refused by the loader on a machine without it, so a
//! folder of JPEGs could not be scanned until a HEIF library was installed,
//! and the version `libheif-rs` was built against (1.17) was the floor for
//! every user whatever they scanned. Loaded here, a machine without libheif
//! runs every other format and reports each HEIF file as one it could not
//! read, naming the library it looked for; and any libheif from 1.12 up
//! serves, since nothing here asks for more than the 2018 API.
//!
//! What is asked of it is what `libheif-rs` asked: the primary image, decoded
//! with libheif's default options — every transformation in the file (`irot`,
//! `imir`, `clap`) applied — to interleaved 8-bit RGB or RGBA.

use anyhow::{anyhow, bail, Result};
use std::ffi::{c_char, c_int, c_void, CStr};
use std::io::{Read, Seek, SeekFrom};
use std::sync::OnceLock;

#[repr(C)]
struct HeifError {
    code: c_int,
    subcode: c_int,
    message: *const c_char,
}

#[repr(C)]
struct Reader {
    reader_api_version: c_int,
    get_position: unsafe extern "C" fn(*mut c_void) -> i64,
    read: unsafe extern "C" fn(*mut c_void, usize, *mut c_void) -> c_int,
    seek: unsafe extern "C" fn(i64, *mut c_void) -> c_int,
    wait_for_file_size: unsafe extern "C" fn(i64, *mut c_void) -> c_int,
}

type Ctx = c_void;
type Handle = c_void;
type Img = c_void;

const COLORSPACE_RGB: c_int = 1;
const CHROMA_INTERLEAVED_RGB: c_int = 10;
const CHROMA_INTERLEAVED_RGBA: c_int = 11;
const CHANNEL_INTERLEAVED: c_int = 10;
const GROW_SIZE_REACHED: c_int = 0;
const GROW_SIZE_BEYOND_EOF: c_int = 2;

/// The functions img-fp calls, looked up once.
struct Api {
    _lib: libloading::Library,
    context_alloc: unsafe extern "C" fn() -> *mut Ctx,
    context_free: unsafe extern "C" fn(*mut Ctx),
    read_from_memory_without_copy: unsafe extern "C" fn(*mut Ctx, *const c_void, usize, *const c_void) -> HeifError,
    read_from_reader: unsafe extern "C" fn(*mut Ctx, *const Reader, *mut c_void, *const c_void) -> HeifError,
    primary_image_handle: unsafe extern "C" fn(*mut Ctx, *mut *mut Handle) -> HeifError,
    handle_release: unsafe extern "C" fn(*const Handle),
    handle_width: unsafe extern "C" fn(*const Handle) -> c_int,
    handle_height: unsafe extern "C" fn(*const Handle) -> c_int,
    handle_has_alpha: unsafe extern "C" fn(*const Handle) -> c_int,
    decode_image: unsafe extern "C" fn(*const Handle, *mut *mut Img, c_int, c_int, *const c_void) -> HeifError,
    image_release: unsafe extern "C" fn(*const Img),
    plane_readonly: unsafe extern "C" fn(*const Img, c_int, *mut c_int) -> *const u8,
    image_width: unsafe extern "C" fn(*const Img, c_int) -> c_int,
    image_height: unsafe extern "C" fn(*const Img, c_int) -> c_int,
}

// SAFETY: the table holds function pointers into a library that is never
// unloaded, and libheif's functions may be called from any thread on objects
// that thread owns.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

/// The names the library goes by: its soname, then the development link a
/// distribution without the runtime package's soname might still have.
#[cfg(not(target_os = "macos"))]
const NAMES: [&str; 2] = ["libheif.so.1", "libheif.so"];
#[cfg(target_os = "macos")]
const NAMES: [&str; 2] = ["libheif.1.dylib", "libheif.dylib"];

/// What a decode says when there is no libheif to ask.
pub fn missing() -> String {
    format!("HEIF and AVIF need libheif, which is not installed ({} was not found)", NAMES[0])
}

fn api() -> Result<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        let lib = NAMES.iter().find_map(|n| unsafe { libloading::Library::new(n) }.ok())?;
        let api = unsafe { load(lib) }?;
        Some(api)
    })
    .as_ref()
    .ok_or_else(|| anyhow!(missing()))
}

/// Every symbol, or `None` when the library lacks one — a libheif too old for
/// this, which is treated as no libheif.
unsafe fn load(lib: libloading::Library) -> Option<Api> {
    macro_rules! sym {
        ($name:literal) => {
            *unsafe { lib.get(concat!($name, "\0").as_bytes()) }.ok()?
        };
    }
    // Plugins (the decoders, since 1.14) are found by `heif_init`, which
    // exists from 1.13; before it, the decoders were built in.
    if let Ok(init) = unsafe { lib.get::<unsafe extern "C" fn(*mut c_void) -> HeifError>(b"heif_init\0") } {
        unsafe { init(std::ptr::null_mut()) };
    }
    Some(Api {
        context_alloc: sym!("heif_context_alloc"),
        context_free: sym!("heif_context_free"),
        read_from_memory_without_copy: sym!("heif_context_read_from_memory_without_copy"),
        read_from_reader: sym!("heif_context_read_from_reader"),
        primary_image_handle: sym!("heif_context_get_primary_image_handle"),
        handle_release: sym!("heif_image_handle_release"),
        handle_width: sym!("heif_image_handle_get_width"),
        handle_height: sym!("heif_image_handle_get_height"),
        handle_has_alpha: sym!("heif_image_handle_has_alpha_channel"),
        decode_image: sym!("heif_decode_image"),
        image_release: sym!("heif_image_release"),
        plane_readonly: sym!("heif_image_get_plane_readonly"),
        image_width: sym!("heif_image_get_width"),
        image_height: sym!("heif_image_get_height"),
        _lib: lib,
    })
}

fn check(e: HeifError, what: &str) -> Result<()> {
    if e.code == 0 {
        return Ok(());
    }
    let msg = if e.message.is_null() { String::new() } else { unsafe { CStr::from_ptr(e.message) }.to_string_lossy().into_owned() };
    bail!("heif{what}: {msg}")
}

/// A file's primary image, opened. `'a` is the bytes it was read from, which
/// libheif reads in place for as long as the context lives.
pub struct Primary<'a> {
    api: &'static Api,
    ctx: *mut Ctx,
    handle: *mut Handle,
    /// The reader's state, for a context reading from a file; libheif holds a
    /// pointer to it.
    _source: Option<Box<Source>>,
    _bytes: std::marker::PhantomData<&'a [u8]>,
}

impl Drop for Primary<'_> {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                (self.api.handle_release)(self.handle);
            }
            (self.api.context_free)(self.ctx);
        }
    }
}

impl<'a> Primary<'a> {
    fn finish(api: &'static Api, ctx: *mut Ctx, source: Option<Box<Source>>, read: HeifError) -> Result<Primary<'a>> {
        let mut p = Primary { api, ctx, handle: std::ptr::null_mut(), _source: source, _bytes: std::marker::PhantomData };
        check(read, "")?;
        check(unsafe { (api.primary_image_handle)(ctx, &mut p.handle) }, "")?;
        Ok(p)
    }

    /// The image in a whole file held in memory.
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Primary<'a>> {
        let api = api()?;
        let ctx = unsafe { (api.context_alloc)() };
        if ctx.is_null() {
            bail!("heif: out of memory");
        }
        let read = unsafe { (api.read_from_memory_without_copy)(ctx, bytes.as_ptr() as *const c_void, bytes.len(), std::ptr::null()) };
        Self::finish(api, ctx, None, read)
    }

    /// The image in a file, read only as far as libheif seeks: for its size,
    /// that is the boxes ahead of the pixels.
    pub fn from_file(file: std::fs::File) -> Result<Primary<'static>> {
        let api = api()?;
        let size = file.metadata()?.len() as i64;
        let mut source = Box::new(Source { r: std::io::BufReader::new(file), size });
        let ctx = unsafe { (api.context_alloc)() };
        if ctx.is_null() {
            bail!("heif: out of memory");
        }
        static READER: Reader = Reader {
            reader_api_version: 1,
            get_position: source_position,
            read: source_read,
            seek: source_seek,
            wait_for_file_size: source_wait,
        };
        let user = &mut *source as *mut Source as *mut c_void;
        let read = unsafe { (api.read_from_reader)(ctx, &READER, user, std::ptr::null()) };
        Primary::finish(api, ctx, Some(source), read)
    }

    pub fn width(&self) -> u32 {
        unsafe { (self.api.handle_width)(self.handle) }.max(0) as u32
    }

    pub fn height(&self) -> u32 {
        unsafe { (self.api.handle_height)(self.handle) }.max(0) as u32
    }

    pub fn has_alpha(&self) -> bool {
        unsafe { (self.api.handle_has_alpha)(self.handle) != 0 }
    }

    /// Decoded to interleaved 8-bit RGB, or RGBA when it has alpha.
    pub fn decode(&self) -> Result<Decoded> {
        let chroma = if self.has_alpha() { CHROMA_INTERLEAVED_RGBA } else { CHROMA_INTERLEAVED_RGB };
        let mut img: *mut Img = std::ptr::null_mut();
        check(unsafe { (self.api.decode_image)(self.handle, &mut img, COLORSPACE_RGB, chroma, std::ptr::null()) }, " decode")?;
        let ch = if self.has_alpha() { 4 } else { 3 };
        let d = Decoded { api: self.api, img, ch };
        let mut stride: c_int = 0;
        let data = unsafe { (self.api.plane_readonly)(img, CHANNEL_INTERLEAVED, &mut stride) };
        let (w, h) = unsafe { ((self.api.image_width)(img, CHANNEL_INTERLEAVED), (self.api.image_height)(img, CHANNEL_INTERLEAVED)) };
        if data.is_null() || w <= 0 || h <= 0 || (stride as i64) < w as i64 * ch as i64 {
            bail!("heif: no interleaved plane");
        }
        Ok(d)
    }
}

/// A decoded image, released when dropped.
pub struct Decoded {
    api: &'static Api,
    img: *mut Img,
    /// Bytes a pixel: 3, or 4 with alpha.
    ch: usize,
}

impl Drop for Decoded {
    fn drop(&mut self) {
        unsafe { (self.api.image_release)(self.img) }
    }
}

impl Decoded {
    /// The interleaved plane: its bytes, its stride, its width and height.
    /// `decode` has checked that it is there and that a row fits its stride.
    pub fn plane(&self) -> (&[u8], usize, usize, usize) {
        let mut stride: c_int = 0;
        unsafe {
            let data = (self.api.plane_readonly)(self.img, CHANNEL_INTERLEAVED, &mut stride);
            let w = (self.api.image_width)(self.img, CHANNEL_INTERLEAVED) as usize;
            let h = (self.api.image_height)(self.img, CHANNEL_INTERLEAVED) as usize;
            let stride = stride as usize;
            // Only as far as the last pixel: the last row is not promised its
            // padding.
            let len = stride * (h - 1) + w * self.ch;
            (std::slice::from_raw_parts(data, len), stride, w, h)
        }
    }
}

/// What libheif reads a file through.
struct Source {
    r: std::io::BufReader<std::fs::File>,
    size: i64,
}

unsafe extern "C" fn source_position(user: *mut c_void) -> i64 {
    let s = unsafe { &mut *(user as *mut Source) };
    s.r.stream_position().map_or(-1, |p| p as i64)
}

unsafe extern "C" fn source_read(data: *mut c_void, size: usize, user: *mut c_void) -> c_int {
    let s = unsafe { &mut *(user as *mut Source) };
    let buf = unsafe { std::slice::from_raw_parts_mut(data as *mut u8, size) };
    if s.r.read_exact(buf).is_ok() { 0 } else { 1 }
}

unsafe extern "C" fn source_seek(position: i64, user: *mut c_void) -> c_int {
    let s = unsafe { &mut *(user as *mut Source) };
    if position < 0 || s.r.seek(SeekFrom::Start(position as u64)).is_err() { 1 } else { 0 }
}

unsafe extern "C" fn source_wait(target: i64, user: *mut c_void) -> c_int {
    let s = unsafe { &*(user as *const Source) };
    if target > s.size { GROW_SIZE_BEYOND_EOF } else { GROW_SIZE_REACHED }
}
