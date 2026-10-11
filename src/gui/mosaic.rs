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
/// `w` wide whose visible part is `h` high, and how tall it all is.
pub fn layout(a: &[f32], w: f32, h: f32) -> (Vec<Rect>, f32) {
    if a.is_empty() || w <= 0.0 {
        return (Vec::new(), 0.0);
    }
    match fit(a, w, h) {
        Some(r) => (r, h),
        None => flow(a, w),
    }
}

/// Every picture on one screen, in as many rows as gives them the most area,
/// or `None` when that would make a row shorter than `MIN_ROW`.
fn fit(a: &[f32], w: f32, h: f32) -> Option<Vec<Rect>> {
    let n = a.len();
    let total: f32 = a.iter().sum();
    // k rows each at least MIN_ROW tall need k * MIN_ROW of height.
    let most = (((h + GAP) / (MIN_ROW + GAP)).floor() as usize).min(n);
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
        if hs.iter().any(|&x| x * scale < MIN_ROW) {
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

/// Rows `ROW` tall, each stretched to the width but the last, for a group
/// that cannot fit one screen.
fn flow(a: &[f32], w: f32) -> (Vec<Rect>, f32) {
    let mut out = Vec::with_capacity(a.len());
    let mut y = 0.0;
    let mut start = 0;
    while start < a.len() {
        let mut end = start;
        let mut sum = 0.0;
        while end < a.len() {
            sum += a[end];
            end += 1;
            if sum * ROW + GAP * (end - start - 1) as f32 >= w {
                break;
            }
        }
        let gaps = GAP * (end - start - 1) as f32;
        let full = sum * ROW + gaps >= w;
        let rh = if full { (w - gaps) / sum } else { ROW };
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
/// `h` of its height, `PAD` included.
fn placed(a: &[f32], width: i32, h: f32) -> Vec<Rect> {
    let (mut rects, _) = layout(a, width as f32 - 2.0 * PAD, (h - 2.0 * PAD).max(1.0));
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
                let r = placed(&self.aspects.borrow(), width, self.viewport.get());
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
        Some(placed(&aspects(sizes), width, h).iter().map(|r| (pixels(r).2, pixels(r).3)).collect())
    }

    /// The size tile `slot` was last allocated, its style's border included.
    pub fn allocated(&self, slot: usize) -> Option<(i32, i32)> {
        self.imp().told.borrow().get(slot).copied().filter(|&(w, _)| w > 0)
    }

    /// Call `f` when the tiles shown have new sizes.
    pub fn connect_resized(&self, f: impl Fn() + 'static) {
        *self.imp().on_resized.borrow_mut() = Some(std::rc::Rc::new(f));
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
        let (r, tall) = layout(&a, 1300.0, 800.0);
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
        let (r, tall) = layout(&a, 1300.0, 700.0);
        assert!(tall > 700.0);
        assert!(r.iter().all(|r| r.x + r.w <= 1300.01));
        assert!(r.iter().all(|r| r.h <= ROW + 60.0));
    }

    #[test]
    fn more_pictures_than_a_row_has_room_for_gaps_still_scroll() {
        let a = vec![1.0; 400];
        let (r, tall) = layout(&a, 1171.0, 725.0);
        assert!(tall > 725.0);
        assert!(r.iter().all(|r| r.x >= 0.0 && r.x + r.w <= 1171.01 && r.h <= ROW + 60.0));
    }

    #[test]
    fn the_pictures_near_the_screen_are_a_range() {
        let (r, _) = layout(&vec![1.0; 1000], 1000.0, 600.0);
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
