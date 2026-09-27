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
//! What is kept is the textures of the last few groups looked at, a quarter of
//! a megabyte each, so paging back is instant.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};

/// Textures kept once nothing on screen shows them.
const KEEP: usize = 160;

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
    /// Oldest first, for dropping.
    order: RefCell<VecDeque<Key>>,
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
            order: RefCell::new(VecDeque::new()),
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
        self.order.borrow_mut().retain(|k| k.0 != path);
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
        self.done.borrow_mut().insert(key.clone(), r);
        let mut order = self.order.borrow_mut();
        order.push_back(key);
        while order.len() > KEEP {
            if let Some(old) = order.pop_front() {
                self.done.borrow_mut().remove(&old);
            }
        }
    }
}

fn deliver(shown: &Shown, r: Result<gdk::Texture, String>) {
    match shown {
        Shown::Picture(p) => match r {
            Ok(t) => p.set_paintable(Some(&t)),
            Err(e) => {
                p.set_paintable(None::<&gdk::Paintable>);
                p.set_alternative_text(Some(&format!("Could not show this image: {e}")));
                p.set_tooltip_text(Some(&format!("Could not show this image: {e}")));
            }
        },
        Shown::Callback(f) => f(r),
    }
}
