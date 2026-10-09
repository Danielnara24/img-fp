//! Pictures for the results page, decoded off the main thread.
//!
//! The scan keeps no colour picture of anything — its thumbnails are grey and
//! 128 pixels — so a group is shown by decoding its files again, with the
//! library's own decoders (`img_fp::preview`). Two things keep that light:
//!
//! - **Only what is on screen is asked for**, and asking for a new group
//!   drops every request still queued for the old one (`clear_queue`), so
//!   paging quickly through a thousand groups decodes the one it stops at.
//! - **Two decoders at most**, because a decode holds the whole picture for a
//!   moment — 133 MB for a 44-megapixel photograph — and a window that spikes
//!   a gigabyte to draw a grid of thumbnails is not light.
//! - **The desktop's own thumbnail is used when it has one** (`from_cache`):
//!   a card is 200 pixels across, and a file manager that has shown the
//!   folder has usually already made a 256- or 512-pixel copy of every picture
//!   in it. Reading that is a few kilobytes of PNG where the picture itself
//!   was the whole decode. The large view still decodes the file, since no
//!   thumbnail is that large.
//! - **A picture being decoded is not queued again.** Paging away and back
//!   used to start a second decode of a card whose first was still running.
//! - **A picture is made at the size it is shown** (`Size`), and handed to
//!   GTK in the software renderer's own pixel format (premultiplied BGRA).
//!   A texture is converted to that format on every frame that draws it, and
//!   scaled on every frame when its size is not the place's: with a group of
//!   ninety decoded at up to twice their size in RGBA, that was some 12 ms a
//!   frame while pictures arrived, and four times the memory.
//!
//! What is kept is the textures most recently shown, up to a fixed number of
//! bytes, so paging back is instant. Bytes and not a count: a card's texture
//! is a tenth of a megabyte and the large view's is seven or more, and a count
//! of 160 that suited the cards held over a gigabyte once the large view had
//! been paged through.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};

/// Bytes of texture kept, on screen or not, by the group's loader: two or
/// three groups of pictures at the size they are shown, or a few large views.
const KEEP_BYTES: usize = 48 << 20;

/// The place a picture is made for, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    pub w: u32,
    pub h: u32,
    /// Fill the place and cut what overhangs, rather than fit inside it.
    pub cover: bool,
}

impl Size {
    /// No larger than `long` either way.
    pub fn within(long: u32) -> Size {
        Size { w: long, h: long, cover: false }
    }

    /// Exactly `w` x `h`, filled.
    pub fn cover(w: u32, h: u32) -> Size {
        Size { w: w.max(1), h: h.max(1), cover: true }
    }
}

pub type Key = (PathBuf, Size);
type Decoded = Result<(u32, u32, Vec<u8>), String>;

struct Queue {
    jobs: VecDeque<Key>,
    /// Taken by a decoder and not yet `arrived`: asking for one again waits
    /// for that answer rather than starting another decode.
    in_flight: std::collections::HashSet<Key>,
}

/// What a picture widget is waiting for.
pub enum Shown {
    Still(crate::still::Still),
    /// Called with the texture, or with the error.
    Callback(Box<dyn Fn(Result<gdk::Texture, String>)>),
}

pub struct Thumbs {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    done: RefCell<HashMap<Key, Result<gdk::Texture, String>>>,
    /// What `done` holds, oldest first, for dropping.
    order: RefCell<Budget<Key>>,
    /// Each widget waiting, with the ticket its request was given.
    waiting: RefCell<HashMap<Key, Vec<(u64, Shown)>>>,
    tickets: std::cell::Cell<u64>,
}

impl Thumbs {
    pub fn new() -> Rc<Thumbs> {
        Thumbs::with_workers(2, KEEP_BYTES)
    }

    /// With at most `most` decoders and `keep` bytes of texture: the group
    /// strip's own loader takes one, so that its pictures never hold up, or
    /// are dropped with, a group's.
    pub fn with_workers(most: usize, keep: usize) -> Rc<Thumbs> {
        let queue = Arc::new((Mutex::new(Queue { jobs: VecDeque::new(), in_flight: Default::default() }), Condvar::new()));
        let (tx, rx) = async_channel::unbounded::<(Key, Decoded)>();
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, most.max(1));
        for _ in 0..workers {
            let (queue, tx) = (queue.clone(), tx.clone());
            let mut dirty = false;
            std::thread::spawn(move || loop {
                let key = {
                    let (m, cv) = &*queue;
                    let mut q = m.lock().unwrap();
                    loop {
                        if let Some(k) = q.jobs.pop_front() {
                            q.in_flight.insert(k.clone());
                            break k;
                        }
                        // Nothing left to do: what the decodes freed goes
                        // back to the system before this waits.
                        if std::mem::take(&mut dirty) {
                            drop(q);
                            release_memory();
                            q = m.lock().unwrap();
                            continue;
                        }
                        q = cv.wait(q).unwrap();
                    }
                };
                dirty = true;
                let Size { w, h, cover } = key.1;
                let got = match thumbnail_dir().and_then(|dir| from_cache(&dir, &key.0, key.1)) {
                    Some(t) => Ok(t),
                    None => match jpeg_scaled(&key.0, key.1) {
                        Some(t) => Ok(t),
                        None => {
                            let _one = heavy(&key.0).then(|| HEAVY.lock().unwrap_or_else(|e| e.into_inner()));
                            img_fp::preview_fit(&key.0, w, h, cover).map_err(|e| format!("{e:#}"))
                        }
                    },
                }
                .map(|(w, h, mut px)| {
                    to_bgra_premultiplied(&mut px);
                    (w, h, px)
                });
                if tx.send_blocking((key, got)).is_err() {
                    return;
                }
            });
        }
        let thumbs = Rc::new(Thumbs {
            queue,
            done: RefCell::new(HashMap::new()),
            order: RefCell::new(Budget::new(keep)),
            waiting: RefCell::new(HashMap::new()),
            tickets: std::cell::Cell::new(0),
        });
        let weak = Rc::downgrade(&thumbs);
        glib::spawn_future_local(async move {
            while let Ok((key, got)) = rx.recv().await {
                let Some(t) = weak.upgrade() else { break };
                t.arrived(key, got);
            }
        });
        thumbs
    }

    /// Drop every request not yet started: the pictures they were for are no
    /// longer on screen.
    pub fn clear_queue(&self) {
        self.queue.0.lock().unwrap().jobs.clear();
        self.waiting.borrow_mut().clear();
    }

    /// Show `path`, made for `size`, in `shown`: now if it is at hand, when
    /// it has been decoded otherwise. `urgent` puts it at the front of the
    /// queue. The ticket withdraws the request (`withdraw`).
    pub fn request(&self, path: &Path, size: Size, shown: Shown, urgent: bool) -> u64 {
        let ticket = self.tickets.get() + 1;
        self.tickets.set(ticket);
        let key = (path.to_path_buf(), size);
        if let Some(r) = self.done.borrow().get(&key) {
            deliver(&shown, r.clone());
            return ticket;
        }
        let mut waiting = self.waiting.borrow_mut();
        let first = !waiting.contains_key(&key);
        waiting.entry(key.clone()).or_default().push((ticket, shown));
        let (m, cv) = &*self.queue;
        let mut q = m.lock().unwrap();
        // Already being decoded: its answer reaches every widget waiting for
        // it, this one included, whenever it comes.
        if q.in_flight.contains(&key) {
            return ticket;
        }
        if first || urgent {
            q.jobs.retain(|k| *k != key);
            if urgent {
                q.jobs.push_front(key);
            } else {
                q.jobs.push_back(key);
            }
            cv.notify_one();
        }
        ticket
    }

    /// Withdraw request `ticket` for `key`: its widget now shows something
    /// else. Nothing else waiting for that picture, it is not decoded at all
    /// unless a decoder already has it.
    pub fn withdraw(&self, key: &Key, ticket: u64) {
        let mut waiting = self.waiting.borrow_mut();
        let Some(list) = waiting.get_mut(key) else { return };
        list.retain(|(t, _)| *t != ticket);
        if list.is_empty() {
            waiting.remove(key);
            self.queue.0.lock().unwrap().jobs.retain(|k| k != key);
        }
    }

    /// Forget a file, which is on its way to the Trash.
    pub fn forget(&self, path: &Path) {
        self.done.borrow_mut().retain(|k, _| k.0 != path);
        self.order.borrow_mut().remove_where(|k| k.0 == path);
    }

    fn arrived(&self, key: Key, got: Decoded) {
        // Here and not on the decoder's thread, so that a request between the
        // decode finishing and its answer arriving waits for this answer.
        self.queue.0.lock().unwrap().in_flight.remove(&key);
        let r = got.map(|(w, h, bgra)| {
            let bytes = glib::Bytes::from_owned(bgra);
            gdk::MemoryTexture::new(w as i32, h as i32, gdk::MemoryFormat::B8g8r8a8Premultiplied, &bytes, w as usize * 4).upcast::<gdk::Texture>()
        });
        // Taken out before delivering, since a delivery may ask for another.
        let list = self.waiting.borrow_mut().remove(&key);
        for (_, s) in list.iter().flatten() {
            deliver(s, r.clone());
        }
        let bytes = r.as_ref().map_or(0, |t| t.width() as usize * t.height() as usize * 4);
        self.done.borrow_mut().insert(key.clone(), r);
        for old in self.order.borrow_mut().insert(key, bytes) {
            self.done.borrow_mut().remove(&old);
        }
    }
}

/// RGBA to what cairo draws without converting: premultiplied, and BGRA in
/// memory, which is `CAIRO_FORMAT_ARGB32` on a little-endian machine. GTK's
/// software renderer converts any other format each time it draws one.
fn to_bgra_premultiplied(px: &mut [u8]) {
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        let (r, b) = (p[0], p[2]);
        if a == 255 {
            p[0] = b;
            p[2] = r;
        } else {
            let mul = |c: u8| ((c as u32 * a + 127) / 255) as u8;
            p[0] = mul(b);
            p[1] = mul(p[1]);
            p[2] = mul(r);
        }
    }
}

/// Held while a picture `heavy` says is heavy is decoded, by every loader.
static HEAVY: Mutex<()> = Mutex::new(());

/// Whether `path` is a HEIF, an AVIF or a JPEG XL, whose decoders hold many
/// times the picture while they work: libaom took 124 MB for one AVIF of
/// IMGS-ALL, and three decoders meeting three of them is what the window's
/// peaks were made of. One of these is decoded at a time.
fn heavy(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 12];
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    if f.read_exact(&mut head).is_err() {
        return false;
    }
    &head[4..8] == b"ftyp" || head.starts_with(&[0xFF, 0x0A]) || head.starts_with(b"\0\0\0\x0cJXL ")
}

/// Hand the heap's free pages back to the system. glibc keeps what a thread
/// frees in that thread's arena, and a decode frees most of a picture.
fn release_memory() {
    #[cfg(target_env = "gnu")]
    {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        // SAFETY: takes and releases the allocator's own locks; nothing
        // else is asked of the caller.
        unsafe {
            malloc_trim(0);
        }
    }
}

/// A JPEG made for `size`, decoded by libjpeg at the smallest of its own
/// scales (a half, a quarter, an eighth) that still covers the place, through
/// gdk-pixbuf, which GTK already loads. `None` for anything else, or for any
/// failure, which the library's own decoder then meets.
///
/// The library's JPEG decoder has no scaled decode, so a 35-megapixel
/// photograph shown in a 250-pixel tile was 105 MB of RGB for a moment, and
/// most of a second; at an eighth it is a sixty-fourth of both, less the
/// entropy decoding, which no scale skips.
fn jpeg_scaled(path: &Path, size: Size) -> Option<(u32, u32, Vec<u8>)> {
    use gtk::gdk_pixbuf::prelude::*;
    let bytes = std::fs::read(path).ok()?;
    if !bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return None;
    }
    let loader = gtk::gdk_pixbuf::PixbufLoader::with_type("jpeg").ok()?;
    loader.connect_size_prepared(move |loader, w, h| {
        if w <= 0 || h <= 0 {
            return;
        }
        // The orientation is not known yet, so enough for the place either
        // way round.
        let (w, h) = (w as f64, h as f64);
        let (pw, ph) = (size.w as f64, size.h as f64);
        let need = |a: f64, b: f64| if size.cover { (pw / a).max(ph / b) } else { (pw / a).min(ph / b) };
        let s = need(w, h).max(need(h, w));
        // libjpeg's own size at each scale, so that gdk-pixbuf has nothing
        // left to resample.
        let mut d = 1.0;
        while d < 8.0 && s * 2.0 * d <= 1.0 {
            d *= 2.0;
        }
        if d > 1.0 {
            loader.set_size((w / d).ceil() as i32, (h / d).ceil() as i32);
        }
    });
    let wrote = loader.write(&bytes).is_ok();
    drop(bytes);
    let closed = loader.close().is_ok();
    if !(wrote && closed) {
        return None;
    }
    let pixbuf = loader.pixbuf()?;
    let pixbuf = pixbuf.apply_embedded_orientation().unwrap_or(pixbuf);
    if pixbuf.bits_per_sample() != 8 {
        return None;
    }
    let (w, h, n, stride) = (pixbuf.width() as usize, pixbuf.height() as usize, pixbuf.n_channels() as usize, pixbuf.rowstride() as usize);
    let bytes = pixbuf.read_pixel_bytes();
    let mut packed = Vec::with_capacity(w * h * n);
    for y in 0..h {
        packed.extend_from_slice(bytes.get(y * stride..y * stride + w * n)?);
    }
    let img = match n {
        3 => image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w as u32, h as u32, packed)?),
        4 => image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(w as u32, h as u32, packed)?),
        _ => return None,
    };
    Some(img_fp::fit_preview(img, size.w, size.h, size.cover))
}

/// The desktop's thumbnail cache: `$XDG_CACHE_HOME/thumbnails`, or
/// `~/.cache/thumbnails`.
fn thumbnail_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("thumbnails"))
}

/// A picture from the freedesktop thumbnail cache under `dir`, made for
/// `size`, as RGBA — when there is one that is current and at least that
/// large.
///
/// The cache is the one the file managers share: a thumbnail is
/// `<size>/<md5 of the file's URI>.png`, and it is current when its
/// `Thumb::MTime` text is the file's modification time in seconds, which is
/// the spec's own test. Only the sizes at least as large as what is asked for
/// are looked in, smallest first, so a card is never drawn from a thumbnail
/// smaller than itself.
fn from_cache(dir: &Path, path: &Path, size: Size) -> Option<(u32, u32, Vec<u8>)> {
    let long = size.w.max(size.h);
    const SIZES: [(&str, u32); 4] = [("normal", 128), ("large", 256), ("x-large", 512), ("xx-large", 1024)];
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    // The URI the file manager would have named it by, which is the path it
    // was opened under: the one the report gives, and failing that the real
    // one, for a report that reached it through a link.
    let mut names = vec![path.to_path_buf()];
    if let Ok(real) = std::fs::canonicalize(path) {
        if real != path {
            names.push(real);
        }
    }
    for name in names {
        let uri = gtk::gio::File::for_path(&name).uri();
        let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, uri.as_str())?;
        for &(sub, folder) in SIZES.iter().filter(|(_, s)| *s >= long) {
            let thumb = dir.join(sub).join(format!("{md5}.png"));
            if !thumb.is_file() {
                continue;
            }
            let Some((tw, th)) = current(&thumb, uri.as_str(), mtime) else { continue };
            // At least `long` on its long side, or smaller than its folder's
            // size and so the whole picture (a thumbnailer shrinks and never
            // enlarges): either way what a full decode would have given. To
            // fill a place it must also be as large as the place both ways,
            // since a wide picture shrunk to the folder's size is short.
            let shrunk = tw.max(th) >= folder;
            if shrunk && size.cover && (tw < size.w || th < size.h) {
                continue;
            }
            if let Ok(t) = img_fp::preview_fit(&thumb, size.w, size.h, size.cover) {
                return Some(t);
            }
        }
    }
    None
}

/// A thumbnail PNG's size, when it says it is of `uri` as it was at `mtime`.
fn current(thumb: &Path, uri: &str, mtime: u64) -> Option<(u32, u32)> {
    let f = std::fs::File::open(thumb).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_ignore_text_chunk(false);
    let reader = dec.read_info().ok()?;
    let text = |key: &str| {
        let info = reader.info();
        info.uncompressed_latin1_text
            .iter()
            .find(|t| t.keyword == key)
            .map(|t| t.text.clone())
            .or_else(|| info.utf8_text.iter().find(|t| t.keyword == key).and_then(|t| t.get_text().ok()))
    };
    let uri_ok = text("Thumb::URI").is_none_or(|u| u == uri);
    let fresh = uri_ok && text("Thumb::MTime").and_then(|m| m.trim().parse::<u64>().ok()) == Some(mtime);
    fresh.then(|| (reader.info().width, reader.info().height))
}

/// Keys oldest first, each with its size, held to a total.
///
/// Inserting a key it already holds moves it to the newest end rather than
/// holding it twice — which a picture decoded again after its queue was
/// cleared used to do, and the older copy's eviction then dropped the newer
/// texture. The newest key is always kept, however large.
pub struct Budget<K> {
    order: VecDeque<(K, usize)>,
    held: usize,
    limit: usize,
}

impl<K: PartialEq> Budget<K> {
    pub fn new(limit: usize) -> Self {
        Budget { order: VecDeque::new(), held: 0, limit }
    }

    /// Hold `key`, and hand back the keys dropped to make room for it.
    pub fn insert(&mut self, key: K, bytes: usize) -> Vec<K> {
        self.remove_where(|k| *k == key);
        self.order.push_back((key, bytes));
        self.held += bytes;
        let mut dropped = Vec::new();
        while self.held > self.limit && self.order.len() > 1 {
            let (k, b) = self.order.pop_front().expect("more than one");
            self.held -= b;
            dropped.push(k);
        }
        dropped
    }

    pub fn remove_where(&mut self, gone: impl Fn(&K) -> bool) {
        let held = &mut self.held;
        self.order.retain(|(k, b)| {
            let drop = gone(k);
            if drop {
                *held -= *b;
            }
            !drop
        });
    }
}

/// Which of several requests for one widget is the one it should show.
///
/// The large view asks for a picture each time it moves, and every answer is
/// for the same widget. Two decoders finish in whatever order they finish, so
/// a slow picture the view has already moved past could arrive after the one
/// it moved to, and be shown under the other's title and mark. Each request
/// takes a ticket; an answer is shown only while its ticket is the latest.
#[derive(Clone, Default)]
pub struct Latest(Rc<std::cell::Cell<u64>>);

impl Latest {
    /// A new request, which makes every earlier one stale. The closure says
    /// whether this one still is the latest.
    pub fn next(&self) -> impl Fn() -> bool + 'static {
        let t = self.0.get() + 1;
        self.0.set(t);
        let now = self.0.clone();
        move || now.get() == t
    }
}

/// Show a decoded picture, or why there is none, in `p`.
pub fn show_on(p: &gtk::Picture, r: Result<gdk::Texture, String>) {
    match r {
        Ok(t) => p.set_paintable(Some(&t)),
        Err(e) => {
            p.set_paintable(None::<&gdk::Paintable>);
            p.set_alternative_text(Some(&format!("Could not show this image: {e}")));
            p.set_tooltip_text(Some(&format!("Could not show this image: {e}")));
        }
    }
}

fn deliver(shown: &Shown, r: Result<gdk::Texture, String>) {
    match shown {
        Shown::Still(p) => match r {
            Ok(t) => p.set_texture(Some(&t)),
            Err(e) => {
                p.set_texture(None);
                p.set_tooltip_text(Some(&format!("Could not show this image: {e}")));
            }
        },
        Shown::Callback(f) => f(r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Held to bytes, oldest out first, and never two entries for one key.
    #[test]
    fn the_cache_is_held_to_bytes_not_to_a_count() {
        let mut b = Budget::new(100);
        assert!(b.insert("card1", 10).is_empty());
        assert!(b.insert("card2", 10).is_empty());
        // A large view evicts the oldest until the whole fits.
        assert_eq!(b.insert("large1", 85), vec!["card1"]);
        assert_eq!(b.held, 95);
        // The same key again moves, rather than doubling.
        assert!(b.insert("card2", 10).is_empty());
        assert_eq!(b.order.iter().map(|e| e.0).collect::<Vec<_>>(), ["large1", "card2"]);
        assert_eq!(b.held, 95);
        // Something larger than the whole budget is kept on its own.
        assert_eq!(b.insert("huge", 500), vec!["large1", "card2"]);
        assert_eq!(b.held, 500);
        b.remove_where(|k| *k == "huge");
        assert_eq!(b.held, 0);
    }

    /// A PNG of `w` x `h` with the two texts the thumbnail spec asks for.
    fn thumbnail_png(at: &Path, w: u32, h: u32, uri: &str, mtime: u64) {
        let f = std::fs::File::create(at).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.add_text_chunk("Thumb::URI".into(), uri.into()).unwrap();
        enc.add_text_chunk("Thumb::MTime".into(), mtime.to_string()).unwrap();
        let mut w8 = enc.write_header().unwrap();
        w8.write_image_data(&vec![200u8; (w * h * 4) as usize]).unwrap();
    }

    /// A card is drawn from the desktop's thumbnail when one is current and
    /// large enough, and from the picture otherwise.
    #[test]
    fn a_current_thumbnail_large_enough_is_used() {
        let dir = std::env::temp_dir().join(format!("img-fp-thumbs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = dir.join("thumbnails");
        std::fs::create_dir_all(cache.join("large")).unwrap();
        std::fs::create_dir_all(cache.join("normal")).unwrap();
        let pic = dir.join("pic.png");
        std::fs::write(&pic, b"not read when the thumbnail answers").unwrap();
        let mtime = std::fs::metadata(&pic).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let uri = gtk::gio::File::for_path(&pic).uri();
        let md5 = glib::compute_checksum_for_string(glib::ChecksumType::Md5, uri.as_str()).unwrap();

        // None at all: the picture is decoded.
        let card = Size::within(200);
        assert!(from_cache(&cache, &pic, card).is_none());
        // A 128-pixel one is too small for a 200-pixel card.
        thumbnail_png(&cache.join("normal").join(format!("{md5}.png")), 128, 96, &uri, mtime);
        assert!(from_cache(&cache, &pic, card).is_none());
        // A 256-pixel one is used, shrunk to the card.
        let large = cache.join("large").join(format!("{md5}.png"));
        thumbnail_png(&large, 256, 192, &uri, mtime);
        let (w, h, rgba) = from_cache(&cache, &pic, card).expect("a current large thumbnail");
        assert_eq!((w, h, rgba.len()), (200, 150, 200 * 150 * 4));
        // Or filling a place exactly, cut to it: from the large one, since
        // the 128-pixel one is only 96 high.
        let (w, h, _) = from_cache(&cache, &pic, Size::cover(120, 120)).expect("a current large thumbnail");
        assert_eq!((w, h), (120, 120));
        // And the 128-pixel one does fill a place it covers.
        std::fs::remove_file(&large).unwrap();
        let (w, h, _) = from_cache(&cache, &pic, Size::cover(96, 72)).expect("a current normal thumbnail");
        assert_eq!((w, h), (96, 72));
        assert!(from_cache(&cache, &pic, Size::cover(120, 120)).is_none());
        // But not one made before the file last changed, nor one of another file.
        thumbnail_png(&large, 256, 192, &uri, mtime - 1);
        assert!(from_cache(&cache, &pic, card).is_none());
        thumbnail_png(&large, 256, 192, "file:///elsewhere.png", mtime);
        assert!(from_cache(&cache, &pic, card).is_none());
        // And never for the large view: no thumbnail is that large.
        thumbnail_png(&large, 256, 192, &uri, mtime);
        assert!(from_cache(&cache, &pic, Size::within(1600)).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A JPEG with `orientation` in its EXIF, `w` x `h` as stored.
    fn oriented_jpeg(at: &Path, w: u32, h: u32, orientation: u8) {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 90]));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&img).unwrap();
        let mut exif = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0".to_vec();
        exif.extend([orientation, 0, 0, 0, 0, 0, 0]);
        let mut out = jpeg[..2].to_vec();
        out.extend([0xFF, 0xE1]);
        out.extend(((exif.len() + 2) as u16).to_be_bytes());
        out.extend(exif);
        out.extend(&jpeg[2..]);
        std::fs::write(at, out).unwrap();
    }

    /// The scaled JPEG decode gives what the library's decoder gives: the
    /// right way up, at the size asked for, and alike to look at.
    #[test]
    fn a_scaled_jpeg_is_the_picture_shrunk() {
        let dir = std::env::temp_dir().join(format!("img-fp-jpeg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("turned.jpg");
        oriented_jpeg(&path, 1600, 800, 6);
        for size in [Size::within(100), Size::cover(60, 90), Size::cover(400, 300)] {
            let (w, h, px) = jpeg_scaled(&path, size).expect("a JPEG decodes through gdk-pixbuf");
            let (lw, lh, lpx) = img_fp::preview_fit(&path, size.w, size.h, size.cover).unwrap();
            assert_eq!((w, h), (lw, lh), "{size:?}");
            let worst = px.iter().zip(&lpx).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
            assert!(worst <= 24, "{size:?}: the two decodes differ by {worst}");
        }
        // Turned a quarter: stored wide, shown tall.
        assert_eq!(jpeg_scaled(&path, Size::within(100)).map(|t| (t.0, t.1)), Some((50, 100)));
        // Anything that is not a JPEG is left to the library.
        std::fs::write(&path, b"not a picture").unwrap();
        assert!(jpeg_scaled(&path, Size::within(100)).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Premultiplied and swapped, and an opaque pixel only swapped.
    #[test]
    fn rgba_becomes_premultiplied_bgra() {
        let mut px = [10, 20, 30, 255, 200, 100, 50, 128, 255, 255, 255, 0];
        to_bgra_premultiplied(&mut px);
        assert_eq!(px, [30, 20, 10, 255, 25, 50, 100, 128, 0, 0, 0, 0]);
    }

    /// Only the latest request's answer is shown, whatever order the answers
    /// arrive in.
    #[test]
    fn only_the_latest_request_is_shown() {
        let latest = Latest::default();
        let first = latest.next();
        assert!(first());
        let second = latest.next();
        assert!(!first(), "a later request makes the earlier one stale");
        assert!(second());
    }
}
