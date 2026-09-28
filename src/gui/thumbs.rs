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

/// Bytes of texture kept once nothing on screen shows them: some six hundred
/// cards, or a dozen large views, or any mixture.
const KEEP_BYTES: usize = 96 << 20;

type Key = (PathBuf, u32);
type Decoded = Result<(u32, u32, Vec<u8>), String>;

struct Queue {
    jobs: VecDeque<Key>,
}

/// What a picture widget is waiting for.
pub enum Shown {
    Picture(gtk::Picture),
    /// Called with the texture, or with the error.
    Callback(Box<dyn Fn(Result<gdk::Texture, String>)>),
}

pub struct Thumbs {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    done: RefCell<HashMap<Key, Result<gdk::Texture, String>>>,
    /// What `done` holds, oldest first, for dropping.
    order: RefCell<Budget<Key>>,
    waiting: RefCell<HashMap<Key, Vec<Shown>>>,
}

impl Thumbs {
    pub fn new() -> Rc<Thumbs> {
        let queue = Arc::new((Mutex::new(Queue { jobs: VecDeque::new() }), Condvar::new()));
        let (tx, rx) = async_channel::unbounded::<(Key, Decoded)>();
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 2);
        for _ in 0..workers {
            let (queue, tx) = (queue.clone(), tx.clone());
            std::thread::spawn(move || loop {
                let key = {
                    let (m, cv) = &*queue;
                    let mut q = m.lock().unwrap();
                    loop {
                        if let Some(k) = q.jobs.pop_front() {
                            break k;
                        }
                        q = cv.wait(q).unwrap();
                    }
                };
                let got = img_fp::preview(&key.0, key.1).map_err(|e| format!("{e:#}"));
                if tx.send_blocking((key, got)).is_err() {
                    return;
                }
            });
        }
        let thumbs = Rc::new(Thumbs {
            queue,
            done: RefCell::new(HashMap::new()),
            order: RefCell::new(Budget::new(KEEP_BYTES)),
            waiting: RefCell::new(HashMap::new()),
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

    /// Show `path` at `long` pixels in `shown`: now if it is at hand, when it
    /// has been decoded otherwise. `urgent` puts it at the front of the queue.
    pub fn request(&self, path: &Path, long: u32, shown: Shown, urgent: bool) {
        let key = (path.to_path_buf(), long);
        if let Some(r) = self.done.borrow().get(&key) {
            deliver(&shown, r.clone());
            return;
        }
        let mut waiting = self.waiting.borrow_mut();
        let first = !waiting.contains_key(&key);
        waiting.entry(key.clone()).or_default().push(shown);
        let (m, cv) = &*self.queue;
        let mut q = m.lock().unwrap();
        if first || urgent {
            q.jobs.retain(|k| *k != key);
            if urgent {
                q.jobs.push_front(key);
            } else {
                q.jobs.push_back(key);
            }
            cv.notify_one();
        }
    }

    /// Forget a file, which is on its way to the Trash.
    pub fn forget(&self, path: &Path) {
        self.done.borrow_mut().retain(|k, _| k.0 != path);
        self.order.borrow_mut().remove_where(|k| k.0 == path);
    }

    fn arrived(&self, key: Key, got: Decoded) {
        let r = got.map(|(w, h, rgba)| {
            let bytes = glib::Bytes::from_owned(rgba);
            gdk::MemoryTexture::new(w as i32, h as i32, gdk::MemoryFormat::R8g8b8a8, &bytes, w as usize * 4).upcast::<gdk::Texture>()
        });
        if let Some(list) = self.waiting.borrow_mut().remove(&key) {
            for s in &list {
                deliver(s, r.clone());
            }
        }
        let bytes = r.as_ref().map_or(0, |t| t.width() as usize * t.height() as usize * 4);
        self.done.borrow_mut().insert(key.clone(), r);
        for old in self.order.borrow_mut().insert(key, bytes) {
            self.done.borrow_mut().remove(&old);
        }
    }
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
        Shown::Picture(p) => show_on(p, r),
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
