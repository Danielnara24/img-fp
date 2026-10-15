//! The images of a group, as rows that fill the page.
//!
//! Each picture keeps its own shape, and the rows are as tall as they can be
//! with the whole group still fitting the space the page has: two pictures
//! fill it, eleven share it. Only a group too large for that, where a row
//! would fall under `MIN_ROW` pixels, is laid out at `ROW` pixels a row and
//! scrolls.
//!
//! Only the pictures on or near the part of the page shown are on a tile:
//! the tiles are a pool, each told in turn which picture it shows, so a
//! page of thousands costs what a screenful does.
//!
//! **Zoom** (`zoom`) picks the row height instead: rows that tall, flowing
//! down a page that scrolls — unless the whole group fits the page with rows
//! at least that tall, when it is fitted as before, so a height chosen for a
//! group of hundreds does not leave a group of three small. Each step is a
//! factor of √2, which doubles or halves every picture's area. The limits are
//! the page's: a row no taller than the part of the page shown, since a
//! taller one cannot be seen whole, and no shorter than the controls a tile
//! draws over its picture need (`connect_least_row`); and no step down while every picture is already on screen,
//! since a smaller one would show nothing more.
//!
//! A picture is never cropped to fit its place: the place has the picture's
//! own shape. Cropping would make a crop and its original look alike, which
//! is the difference a person deciding what to delete most needs to see.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// The shortest row the page fits a group into before it scrolls instead.
const MIN_ROW: f32 = 150.0;
/// A row's height when the group scrolls.
const ROW: f32 = 200.0;
/// Between pictures, both ways.
const GAP: f32 = 6.0;
/// Around them all, so that the outline of an image at the edge, which is
/// drawn outside it, is not cut off.
const PAD: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Where each of pictures of aspect `a` (width over height) goes in a page
/// `w` wide whose visible part is `h` high, and how tall it all is. `row` is
/// the row height chosen by zooming, `None` for the page's own fit.
pub fn layout(a: &[f32], w: f32, h: f32, row: Option<f32>) -> (Vec<Rect>, f32) {
    if a.is_empty() || w <= 0.0 {
        return (Vec::new(), 0.0);
    }
    // Never taller than the page shows: see the head of the file.
    let row = row.map(|r| r.min(h).max(1.0));
    match fit(a, w, h, row.unwrap_or(MIN_ROW)) {
        Some(r) => (r, h),
        None => flow(a, w, row.unwrap_or(ROW), h),
    }
}

/// Every picture on one screen, in as many rows as gives them the most area,
/// or `None` when that would make a row shorter than `least`.
fn fit(a: &[f32], w: f32, h: f32, least: f32) -> Option<Vec<Rect>> {
    let n = a.len();
    let total: f32 = a.iter().sum();
    // k rows each at least `least` tall need k * `least` of height.
    let most = (((h + GAP) / (least + GAP)).floor() as usize).min(n);
    // (area, rows, row heights before scaling, scale)
    type Candidate = (f32, Vec<Vec<usize>>, Vec<f32>, f32);
    let mut best: Option<Candidate> = None;
    for k in 1..=most {
        let rows = split(a, total, k);
        if rows.len() != k {
            continue;
        }
        let hs: Vec<f32> = rows.iter().map(|r| (w - GAP * (r.len() - 1) as f32) / r.iter().map(|&i| a[i]).sum::<f32>()).collect();
        // A row whose gaps alone are wider than the page has a negative
        // height, and two negatives made a scale that passed: a folder of
        // four hundred laid out as one row far off the page.
        if hs.iter().any(|&x| x <= 0.0) {
            continue;
        }
        let tall = hs.iter().sum::<f32>() + GAP * (k - 1) as f32;
        let scale = (h / tall).min(1.0);
        if hs.iter().any(|&x| x * scale < least) {
            continue;
        }
        let area = scale * scale * w * tall;
        if best.as_ref().is_none_or(|b| area > b.0) {
            best = Some((area, rows, hs, scale));
        }
    }
    let (_, rows, hs, scale) = best?;
    let tall = hs.iter().sum::<f32>() * scale + GAP * (rows.len() - 1) as f32;
    let mut out = vec![Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }; n];
    let mut y = ((h - tall) / 2.0).max(0.0);
    for (r, rh) in rows.iter().zip(&hs) {
        let rh = rh * scale;
        let wide = r.iter().map(|&i| a[i] * rh).sum::<f32>() + GAP * (r.len() - 1) as f32;
        let mut x = (w - wide) / 2.0;
        for &i in r {
            out[i] = Rect { x, y, w: a[i] * rh, h: rh };
            x += a[i] * rh + GAP;
        }
        y += rh + GAP;
    }
    Some(out)
}

/// `a` in order, cut into `k` rows of about equal aspect sum.
fn split(a: &[f32], total: f32, k: usize) -> Vec<Vec<usize>> {
    let n = a.len();
    let mut rows: Vec<Vec<usize>> = Vec::with_capacity(k);
    let mut cur = Vec::new();
    let (mut acc, mut used) = (0.0, 0.0);
    let mut target = total / k as f32;
    for (i, &ai) in a.iter().enumerate() {
        cur.push(i);
        acc += ai;
        used += ai;
        let left = n - i - 1;
        let rows_left = k - rows.len() - 1;
        if rows_left > 0 && ((acc >= target * 0.92 && left >= rows_left) || left == rows_left) {
            rows.push(std::mem::take(&mut cur));
            acc = 0.0;
            target = (total - used) / (k - rows.len()) as f32;
        }
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    rows
}

/// Rows about `row` tall, each stretched or shrunk to the width but the
/// last, for a group that cannot fit one screen.
///
/// **Each row ends where its height comes nearest `row`**: before the picture
/// that overflows the width, stretching the row, or after it, shrinking it,
/// whichever is the smaller factor away. Ending always after it was right for
/// rows a sixth of the page wide and wrong for zoomed ones a third or half of
/// it, where shrinking to the width made one row half the height of the
/// next. A stretched row is never taller than `most`, the page's height.
fn flow(a: &[f32], w: f32, row: f32, most: f32) -> (Vec<Rect>, f32) {
    let mut out = Vec::with_capacity(a.len());
    let mut y = 0.0;
    let mut start = 0;
    let height = |sum: f32, n: usize| (w - GAP * (n - 1) as f32) / sum;
    while start < a.len() {
        let mut end = start;
        let mut sum = 0.0;
        while end < a.len() {
            sum += a[end];
            end += 1;
            if sum * row + GAP * (end - start - 1) as f32 >= w {
                break;
            }
        }
        let full = sum * row + GAP * (end - start - 1) as f32 >= w;
        let mut rh = if full { height(sum, end - start) } else { row };
        // One picture fewer, stretched to the width, when that is nearer.
        if full && end - start > 1 {
            let short = height(sum - a[end - 1], end - start - 1);
            if short <= most && (short / row).ln().abs() < (row / rh).ln().abs() {
                end -= 1;
                rh = short;
            }
        }
        let mut x = 0.0;
        for &ai in &a[start..end] {
            out.push(Rect { x, y, w: ai * rh, h: rh });
            x += ai * rh + GAP;
        }
        y += rh + GAP;
        start = end;
    }
    (out, (y - GAP).max(0.0))
}

/// The tiles of pictures of aspect `a` in a mosaic `width` wide that shows
/// `h` of its height, `PAD` included, at zoom `row`.
fn placed(a: &[f32], width: i32, h: f32, row: Option<f32>) -> Vec<Rect> {
    let (mut rects, _) = layout(a, width as f32 - 2.0 * PAD, (h - 2.0 * PAD).max(1.0), row);
    for r in rects.iter_mut() {
        r.x += PAD;
        r.y += PAD;
    }
    rects
}

/// A tile's place in whole pixels: edges rounded, not sizes, so that the
/// gaps stay even.
fn pixels(r: &Rect) -> (i32, i32, i32, i32) {
    let (x0, y0) = (r.x.round() as i32, r.y.round() as i32);
    let (x1, y1) = ((r.x + r.w).round() as i32, (r.y + r.h).round() as i32);
    (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
}

fn aspects(sizes: &[(u32, u32)]) -> Vec<f32> {
    sizes.iter().map(|&(w, h)| if w > 0 && h > 0 { (w as f32 / h as f32).clamp(0.1, 10.0) } else { 4.0 / 3.0 }).collect()
}

/// The tile above or below tile `i`, nearest to it across: `down` says which.
pub fn neighbour(rects: &[Rect], i: usize, down: bool) -> Option<usize> {
    let me = rects.get(i)?;
    let cx = me.x + me.w / 2.0;
    let row_y = rects
        .iter()
        .map(|r| r.y)
        .filter(|&y| if down { y > me.y + 0.5 } else { y < me.y - 0.5 })
        .fold(None, |best: Option<f32>, y| match best {
            None => Some(y),
            Some(b) => Some(if down { b.min(y) } else { b.max(y) }),
        })?;
    rects
        .iter()
        .enumerate()
        .filter(|(_, r)| (r.y - row_y).abs() < 0.5)
        .min_by(|(_, p), (_, q)| (p.x + p.w / 2.0 - cx).abs().total_cmp(&(q.x + q.w / 2.0 - cx).abs()))
        .map(|(j, _)| j)
}

/// The pictures whose tiles lie within `margin` of the part of the page shown,
/// `top` to `top + h`: a range, since the rows run down the page in order.
pub fn visible(rects: &[Rect], top: f32, h: f32, margin: f32) -> std::ops::Range<usize> {
    let (lo, hi) = (top - margin, top + h + margin);
    let start = rects.partition_point(|r| r.y + r.h < lo);
    let end = start + rects[start..].partition_point(|r| r.y <= hi);
    start..end
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;

    /// Told that tile `.0` now shows picture `.1`, or none.
    pub type Bind = Rc<dyn Fn(usize, Option<usize>)>;

    #[derive(Default)]
    pub struct Mosaic {
        /// The tiles made so far: a pool, each showing one picture near the
        /// part of the page on screen, or none. A folder of the tree view can
        /// hold every image of a scan, and a tile for each was a widget, a
        /// decode and a texture for each of 27,000.
        pub tiles: RefCell<Vec<gtk::Widget>>,
        /// The picture each tile shows, and the tile each picture is on.
        pub slot_item: RefCell<Vec<Option<usize>>>,
        pub item_slot: RefCell<HashMap<usize, usize>>,
        /// The shape of every picture.
        pub aspects: RefCell<Vec<f32>>,
        /// Where they all go, at `laid_for`'s width and viewport.
        pub rects: RefCell<Vec<Rect>>,
        pub laid_for: Cell<(i32, f32)>,
        /// The row height zooming chose, `None` for the page's own fit.
        pub row: Cell<Option<f32>>,
        /// Told whether a step in and a step out would each change anything,
        /// when that changes.
        pub on_zoomable: RefCell<Option<Rc<dyn Fn(bool, bool)>>>,
        /// Asked for the shortest row a tile can take (`connect_least_row`).
        pub least: RefCell<Option<Rc<dyn Fn() -> f32>>>,
        pub zoomable: Cell<Option<(bool, bool)>>,
        /// The height the page shows of it, from the scrolled window, and
        /// how far down it is scrolled.
        pub viewport: Cell<f32>,
        pub offset: Cell<f32>,
        /// Popovers parented here, which a parent must present itself.
        pub popovers: RefCell<Vec<gtk::Popover>>,
        /// Told when the tiles shown have been given new sizes, after the
        /// allocation that gave them.
        pub on_resized: RefCell<Option<Rc<dyn Fn()>>>,
        /// Told that a tile now shows a picture (`Some`) or none, and asked
        /// for one more tile when the pool is short.
        pub on_bind: RefCell<Option<Bind>>,
        pub on_grow: RefCell<Option<Rc<dyn Fn()>>>,
        /// The tile sizes last told of, by tile.
        pub told: RefCell<Vec<(i32, i32)>>,
        /// A rebinding is waiting for the main loop.
        pub pending: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Mosaic {
        const NAME: &'static str = "ImgFpMosaic";
        type Type = super::Mosaic;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Mosaic {
        fn dispose(&self) {
            for t in self.tiles.take() {
                t.unparent();
            }
            for p in self.popovers.take() {
                p.unparent();
            }
        }
    }

    impl Mosaic {
        /// Every picture's tile, laid out for `width`; computed again only
        /// when the width or the viewport changed.
        pub fn rects_for(&self, width: i32) -> std::cell::Ref<'_, Vec<Rect>> {
            let key = (width, self.viewport.get());
            if self.laid_for.get() != key || self.rects.borrow().len() != self.aspects.borrow().len() {
                let r = placed(&self.aspects.borrow(), width, self.viewport.get(), self.row.get());
                *self.rects.borrow_mut() = r;
                self.laid_for.set(key);
            }
            self.rects.borrow()
        }

        /// The pictures near the part of the page shown, at the last width.
        pub fn wanted(&self) -> std::ops::Range<usize> {
            let (width, _) = self.laid_for.get();
            if width <= 0 {
                return 0..0;
            }
            let vp = self.viewport.get().max(1.0);
            visible(&self.rects_for(width), self.offset.get(), vp, vp)
        }

        /// Give the pictures near the part shown a tile each and take them
        /// from the rest, telling `on_bind` of every change.
        pub fn rebind(&self) {
            self.pending.set(false);
            let want = self.wanted();
            let bind = self.on_bind.borrow().clone();
            let mut gone: Vec<usize> = Vec::new();
            {
                let slots = self.slot_item.borrow();
                for (slot, item) in slots.iter().enumerate() {
                    if item.is_some_and(|i| !want.contains(&i)) {
                        gone.push(slot);
                    }
                }
            }
            for slot in &gone {
                let item = self.slot_item.borrow_mut()[*slot].take();
                if let Some(i) = item {
                    self.item_slot.borrow_mut().remove(&i);
                }
                self.tiles.borrow()[*slot].set_child_visible(false);
                if let Some(f) = &bind {
                    f(*slot, None);
                }
            }
            let mut changed = !gone.is_empty();
            for item in want {
                if self.item_slot.borrow().contains_key(&item) {
                    continue;
                }
                let free = self.slot_item.borrow().iter().position(Option::is_none);
                let slot = match free {
                    Some(s) => s,
                    None => {
                        let next = self.slot_item.borrow().len();
                        let grow = self.on_grow.borrow().clone();
                        if next >= self.tiles.borrow().len()
                            && let Some(g) = grow
                        {
                            g();
                        }
                        if next >= self.tiles.borrow().len() {
                            break;
                        }
                        next
                    }
                };
                {
                    let mut slots = self.slot_item.borrow_mut();
                    if slots.len() <= slot {
                        slots.resize(slot + 1, None);
                    }
                    slots[slot] = Some(item);
                }
                self.item_slot.borrow_mut().insert(item, slot);
                self.tiles.borrow()[slot].set_child_visible(true);
                if let Some(f) = &bind {
                    f(slot, Some(item));
                }
                changed = true;
            }
            if changed {
                self.obj().queue_allocate();
            }
        }

        /// Whether a step in, and a step out, would change anything; see the
        /// head of the file.
        pub fn can_zoom(&self) -> (bool, bool) {
            let (width, _) = self.laid_for.get();
            if width <= 0 {
                return (false, false);
            }
            let page = (self.viewport.get() - 2.0 * PAD).max(1.0);
            let rects = self.rects_for(width);
            let Some(row) = rects.iter().map(|r| r.h).reduce(f32::max) else { return (false, false) };
            let tall = rects.iter().map(|r| r.y + r.h).fold(0.0f32, f32::max) - PAD;
            drop(rects);
            (row < page - 0.5, tall > page + 0.5 && row > self.least_row() + 0.5)
        }

        /// The shortest row a tile can take, as its owner says
        /// (`connect_least_row`): what the tile draws over its picture.
        pub fn least_row(&self) -> f32 {
            let least = self.least.borrow().clone();
            least.map_or(1.0, |f| f()).max(1.0)
        }

        /// Say whether the zoom can move, if that changed.
        fn tell_zoomable(&self) {
            let now = self.can_zoom();
            if self.zoomable.replace(Some(now)) != Some(now)
                && let Some(f) = self.on_zoomable.borrow().clone()
            {
                glib::idle_add_local_once(move || f(now.0, now.1));
            }
        }

        /// Rebind once the main loop is free: never inside an allocation,
        /// where a tile told of a new picture would change its style.
        pub fn rebind_soon(&self) {
            if self.pending.replace(true) {
                return;
            }
            let me = self.obj().downgrade();
            glib::idle_add_local_full(glib::Priority::HIGH_IDLE, move || {
                if let Some(m) = me.upgrade() {
                    m.imp().rebind();
                }
                glib::ControlFlow::Break
            });
        }
    }

    impl WidgetImpl for Mosaic {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                return (120, 600, -1, -1);
            }
            let w = if for_size < 0 { 600 } else { for_size };
            let rects = self.rects_for(w);
            let tall = rects.iter().map(|r| r.y + r.h).fold(0.0f32, f32::max);
            let tall = if rects.is_empty() { 0.0 } else { tall.max(self.viewport.get() - PAD) };
            let tall = (tall + PAD).ceil() as i32;
            (tall, tall, -1, -1)
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let tiles = self.tiles.borrow();
            let slots = self.slot_item.borrow();
            let rects = self.rects_for(width);
            let mut sizes = Vec::with_capacity(tiles.len());
            for (t, item) in tiles.iter().zip(slots.iter().chain(std::iter::repeat(&None))) {
                let Some(r) = item.and_then(|i| rects.get(i)) else {
                    sizes.push((0, 0));
                    continue;
                };
                let (x0, y0, w, h) = pixels(r);
                t.measure(gtk::Orientation::Horizontal, -1);
                t.size_allocate(&gtk::Allocation::new(x0, y0, w, h), -1);
                sizes.push((w, h));
            }
            for p in self.popovers.borrow().iter() {
                p.present();
            }
            let want = visible(&rects, self.offset.get(), self.viewport.get().max(1.0), self.viewport.get().max(1.0));
            drop(rects);
            // A picture near the screen without a tile, or a tile on a
            // picture far from it: bound again once this is over.
            let stale = slots.iter().any(|i| i.is_some_and(|i| !want.contains(&i)))
                || want.clone().any(|i| !self.item_slot.borrow().contains_key(&i));
            drop(slots);
            drop(tiles);
            if stale {
                self.rebind_soon();
            }
            self.tell_zoomable();
            // Pictures are made at the size their tile is drawn, so a tile
            // given another size wants another picture: said once the
            // allocation is over, never during it.
            if *self.told.borrow() != sizes {
                *self.told.borrow_mut() = sizes;
                if let Some(f) = self.on_resized.borrow().clone() {
                    glib::idle_add_local_once(move || f());
                }
            }
        }
    }
}

glib::wrapper! {
    pub struct Mosaic(ObjectSubclass<imp::Mosaic>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Mosaic {
    pub fn new() -> Mosaic {
        glib::Object::new()
    }

    /// One more tile for the pool, hidden until it is given a picture.
    pub fn append(&self, tile: &impl IsA<gtk::Widget>) {
        tile.set_parent(self);
        tile.set_child_visible(false);
        self.imp().tiles.borrow_mut().push(tile.clone().upcast());
    }

    /// Show pictures of these `(width, height)`s, none of them on a tile yet:
    /// every tile is told it shows nothing, and the pictures near the top
    /// are given tiles once the page has been laid out (or now, if it has).
    pub fn show(&self, sizes: &[(u32, u32)]) {
        let imp = self.imp();
        let bind = imp.on_bind.borrow().clone();
        let old: Vec<usize> = imp.slot_item.borrow().iter().enumerate().filter(|(_, i)| i.is_some()).map(|(s, _)| s).collect();
        for s in old {
            imp.slot_item.borrow_mut()[s] = None;
            imp.tiles.borrow()[s].set_child_visible(false);
            if let Some(f) = &bind {
                f(s, None);
            }
        }
        imp.item_slot.borrow_mut().clear();
        *imp.aspects.borrow_mut() = aspects(sizes);
        imp.laid_for.set((0, 0.0));
        imp.rects.borrow_mut().clear();
        imp.offset.set(0.0);
        if self.width() > 0 {
            imp.laid_for.set((self.width(), imp.viewport.get()));
            imp.rebind();
        }
        self.queue_resize();
    }

    /// Tell `bind` which picture tile `slot` now shows, or that it shows
    /// none; `grow` must append one more tile when called.
    pub fn connect_bind(&self, bind: impl Fn(usize, Option<usize>) + 'static, grow: impl Fn() + 'static) {
        *self.imp().on_bind.borrow_mut() = Some(std::rc::Rc::new(bind));
        *self.imp().on_grow.borrow_mut() = Some(std::rc::Rc::new(grow));
    }

    /// The tile picture `item` is on, if it is on one.
    pub fn slot_of(&self, item: usize) -> Option<usize> {
        self.imp().item_slot.borrow().get(&item).copied()
    }

    /// The pictures on tiles, with their tiles.
    pub fn bound(&self) -> Vec<(usize, usize)> {
        self.imp().slot_item.borrow().iter().enumerate().filter_map(|(s, i)| i.map(|i| (s, i))).collect()
    }

    /// Where picture `item` is, in the mosaic's own pixels.
    pub fn rect(&self, item: usize) -> Option<(i32, i32, i32, i32)> {
        let (w, _) = self.imp().laid_for.get();
        if w <= 0 {
            return None;
        }
        self.imp().rects_for(w).get(item).map(pixels)
    }

    /// The size each tile would have for pictures of `sizes`, laid out in the
    /// space the mosaic has now; `None` before it has any.
    pub fn tile_sizes(&self, sizes: &[(u32, u32)]) -> Option<Vec<(i32, i32)>> {
        let (width, h) = (self.width(), self.imp().viewport.get());
        if width <= 0 || h <= 0.0 {
            return None;
        }
        Some(placed(&aspects(sizes), width, h, self.imp().row.get()).iter().map(|r| (pixels(r).2, pixels(r).3)).collect())
    }

    /// The size tile `slot` was last allocated, its style's border included.
    pub fn allocated(&self, slot: usize) -> Option<(i32, i32)> {
        self.imp().told.borrow().get(slot).copied().filter(|&(w, _)| w > 0)
    }

    /// Call `f` when the tiles shown have new sizes.
    pub fn connect_resized(&self, f: impl Fn() + 'static) {
        *self.imp().on_resized.borrow_mut() = Some(std::rc::Rc::new(f));
    }

    /// One step larger (`in_`) or smaller; see the head of the file. Whether
    /// anything changed.
    pub fn zoom(&self, in_: bool) -> bool {
        let imp = self.imp();
        let (can_in, can_out) = imp.can_zoom();
        if !(if in_ { can_in } else { can_out }) {
            return false;
        }
        let (width, _) = imp.laid_for.get();
        let page = (imp.viewport.get() - 2.0 * PAD).max(1.0);
        // The height asked for, where the group flows at it; the fitted
        // rows' otherwise.
        let (rows, tall) = {
            let rects = imp.rects_for(width);
            (rects.iter().map(|r| r.h).fold(0.0f32, f32::max), rects.iter().map(|r| r.y + r.h).fold(0.0f32, f32::max) - PAD)
        };
        let now = match imp.row.get() {
            Some(r) if tall > page + 0.5 => r.min(page),
            _ => rows,
        };
        let row = if in_ { (now * std::f32::consts::SQRT_2).min(page) } else { (now / std::f32::consts::SQRT_2).max(imp.least_row()) };
        self.set_row(Some(row));
        true
    }

    /// Back to the page's own fit. Whether anything changed.
    pub fn zoom_fit(&self) -> bool {
        if self.imp().row.get().is_none() {
            return false;
        }
        self.set_row(None);
        true
    }

    fn set_row(&self, row: Option<f32>) {
        let imp = self.imp();
        imp.row.set(row);
        imp.laid_for.set((0, 0.0));
        let (w, vp) = (self.width(), imp.viewport.get());
        if w > 0 {
            imp.laid_for.set((w, vp));
            imp.rects.borrow_mut().clear();
            imp.rebind();
        }
        self.queue_resize();
    }

    /// Ask `f` for the shortest row a tile can take. An overlay's size is its
    /// picture's alone, so what is drawn over it — which a row must still
    /// have room for — only the tiles' owner can say.
    pub fn connect_least_row(&self, f: impl Fn() -> f32 + 'static) {
        *self.imp().least.borrow_mut() = Some(std::rc::Rc::new(f));
    }

    /// Call `f` with whether a step in and a step out would change anything,
    /// whenever that changes.
    pub fn connect_zoomable(&self, f: impl Fn(bool, bool) + 'static) {
        *self.imp().on_zoomable.borrow_mut() = Some(std::rc::Rc::new(f));
    }

    /// The height the page shows, which is what a group is fitted into.
    pub fn set_viewport(&self, h: f64) {
        let h = h as f32;
        if (self.imp().viewport.get() - h).abs() >= 0.5 {
            self.imp().viewport.set(h);
            self.queue_resize();
        }
    }

    /// How far down the page is scrolled.
    pub fn set_offset(&self, y: f64) {
        self.imp().offset.set(y as f32);
        self.imp().rebind_soon();
    }

    /// Bind the pictures near the part shown now, rather than once the main
    /// loop is free: the keyboard is about to go to one of them.
    pub fn rebind_now(&self) {
        self.imp().rebind();
    }

    pub fn add_popover(&self, p: &gtk::PopoverMenu) {
        p.set_parent(self);
        self.imp().popovers.borrow_mut().push(p.clone().upcast());
    }

    /// The tile above or below tile `i`, as laid out now.
    pub fn vertical_neighbour(&self, i: usize, down: bool) -> Option<usize> {
        let (w, _) = self.imp().laid_for.get();
        if w <= 0 {
            return None;
        }
        neighbour(&self.imp().rects_for(w), i, down)
    }
}

impl Default for Mosaic {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(r: &[Rect], w: f32, h: f32) -> bool {
        r.iter().all(|r| r.x >= -0.01 && r.y >= -0.01 && r.x + r.w <= w + 0.01 && r.y + r.h <= h + 0.01)
    }

    #[test]
    fn a_group_that_fits_fills_one_screen_at_its_own_shapes() {
        let a = [1.33, 1.5, 0.75, 1.33, 1.0, 1.77, 1.33, 1.2, 1.33, 1.33, 2.0];
        let (r, tall) = layout(&a, 1300.0, 800.0, None);
        assert_eq!(tall, 800.0);
        assert!(inside(&r, 1300.0, 800.0));
        for (rect, aspect) in r.iter().zip(a) {
            assert!((rect.w / rect.h - aspect).abs() < 1e-3);
            assert!(rect.h >= MIN_ROW);
        }
    }

    #[test]
    fn a_group_too_large_to_fit_scrolls_in_rows_of_row_height() {
        let a = vec![1.33; 200];
        let (r, tall) = layout(&a, 1300.0, 700.0, None);
        assert!(tall > 700.0);
        assert!(r.iter().all(|r| r.x + r.w <= 1300.01));
        assert!(r.iter().all(|r| r.h <= ROW + 60.0));
    }

    #[test]
    fn more_pictures_than_a_row_has_room_for_gaps_still_scroll() {
        let a = vec![1.0; 400];
        let (r, tall) = layout(&a, 1171.0, 725.0, None);
        assert!(tall > 725.0);
        assert!(r.iter().all(|r| r.x >= 0.0 && r.x + r.w <= 1171.01 && r.h <= ROW + 60.0));
    }

    /// A row height chosen by zooming: rows that tall where the group does
    /// not fit, the fit where it does with rows at least that tall, and never
    /// a row taller than the page shows.
    #[test]
    fn a_zoomed_group_flows_at_its_row_unless_it_fits_at_least_that_tall() {
        let many = vec![1.33; 200];
        let (r, tall) = layout(&many, 1300.0, 700.0, Some(100.0));
        assert!(tall > 700.0 && r.iter().all(|r| r.h <= 700.01));
        assert!(r.iter().filter(|r| (r.h - 100.0).abs() < 15.0).count() > 150, "rows of the height asked");
        let few = [1.33, 1.5, 0.75];
        let (r, tall) = layout(&few, 1300.0, 800.0, Some(100.0));
        assert_eq!(tall, 800.0, "three fit the page larger than asked, and are fitted");
        assert!(r.iter().all(|r| r.h > 100.0));
        let (r, tall) = layout(&few, 1300.0, 800.0, Some(5000.0));
        assert!(r.iter().all(|r| r.h <= 800.01), "no row taller than the page: {tall}");
    }

    /// Rows a third of the page wide end where their height is nearest the
    /// row asked for, stretched or shrunk, rather than always shrunk: no row
    /// half the height of the next.
    #[test]
    fn a_wide_row_ends_where_its_height_is_nearest() {
        let a = [1.5, 1.33, 1.5, 1.5, 1.33, 1.5, 0.75, 1.5, 1.33, 1.77, 1.5];
        let (r, _) = layout(&a, 1340.0, 730.0, Some(660.0));
        assert!(r.iter().all(|r| (r.h / 660.0).ln().abs() < std::f32::consts::LN_2 / 2.0 + 0.2), "{:?}", r.iter().map(|r| r.h).collect::<Vec<_>>());
    }

    #[test]
    fn the_pictures_near_the_screen_are_a_range() {
        let (r, _) = layout(&vec![1.0; 1000], 1000.0, 600.0, None);
        let v = visible(&r, 2000.0, 600.0, 600.0);
        assert!(r[v.start].y + r[v.start].h >= 1400.0 && (v.start == 0 || r[v.start - 1].y + r[v.start - 1].h < 1400.0));
        assert!(r[v.end - 1].y <= 3200.0 && r.get(v.end).is_none_or(|n| n.y > 3200.0));
        assert_eq!(visible(&r, 0.0, 600.0, 600.0).start, 0);
    }

    #[test]
    fn up_and_down_go_to_the_nearest_tile_of_the_next_row() {
        let r = [
            Rect { x: 0.0, y: 0.0, w: 100.0, h: 50.0 },
            Rect { x: 106.0, y: 0.0, w: 100.0, h: 50.0 },
            Rect { x: 0.0, y: 56.0, w: 60.0, h: 50.0 },
            Rect { x: 66.0, y: 56.0, w: 140.0, h: 50.0 },
        ];
        assert_eq!(neighbour(&r, 1, true), Some(3));
        assert_eq!(neighbour(&r, 2, false), Some(0));
        assert_eq!(neighbour(&r, 0, false), None);
    }
}
