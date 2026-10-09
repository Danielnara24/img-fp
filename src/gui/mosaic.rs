//! The images of a group, as rows that fill the page.
//!
//! Each picture keeps its own shape, and the rows are as tall as they can be
//! with the whole group still fitting the space the page has: two pictures
//! fill it, eleven share it. Only a group too large for that, where a row
//! would fall under `MIN_ROW` pixels, is laid out at `ROW` pixels a row and
//! scrolls.
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

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct Mosaic {
        /// Every tile made so far. They are kept and reused from one group
        /// to the next: making a group's worth of tiles was most of what a
        /// change of group cost.
        pub tiles: RefCell<Vec<gtk::Widget>>,
        /// The shape of each picture shown, one for each of the first tiles;
        /// the tiles past them are hidden.
        pub aspects: RefCell<Vec<f32>>,
        /// Where the last allocation put them.
        pub rects: RefCell<Vec<Rect>>,
        /// The height the page shows of it, from the scrolled window.
        pub viewport: Cell<f32>,
        /// Popovers parented here, which a parent must present itself.
        pub popovers: RefCell<Vec<gtk::Popover>>,
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

    impl WidgetImpl for Mosaic {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                return (120, 600, -1, -1);
            }
            let w = if for_size < 0 { 600.0 } else { for_size as f32 };
            let a = self.aspects.borrow();
            let (_, tall) = layout(&a, w - 2.0 * PAD, self.viewport.get() - 2.0 * PAD);
            let tall = (tall + 2.0 * PAD).ceil() as i32;
            (tall, tall, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let tiles = self.tiles.borrow();
            let a = self.aspects.borrow();
            let h = self.viewport.get().min(height as f32) - 2.0 * PAD;
            let (mut rects, _) = layout(&a, width as f32 - 2.0 * PAD, h.max(1.0));
            for r in rects.iter_mut() {
                r.x += PAD;
                r.y += PAD;
            }
            for (t, r) in tiles.iter().zip(&rects) {
                // Edges rounded, not sizes, so that the gaps stay even.
                let (x0, y0) = (r.x.round() as i32, r.y.round() as i32);
                let (x1, y1) = ((r.x + r.w).round() as i32, (r.y + r.h).round() as i32);
                t.measure(gtk::Orientation::Horizontal, -1);
                t.size_allocate(&gtk::Allocation::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)), -1);
            }
            *self.rects.borrow_mut() = rects;
            for p in self.popovers.borrow().iter() {
                p.present();
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

    /// One more tile, hidden until `show` counts it in.
    pub fn append(&self, tile: &impl IsA<gtk::Widget>) {
        tile.set_parent(self);
        tile.set_child_visible(false);
        self.imp().tiles.borrow_mut().push(tile.clone().upcast());
    }

    pub fn tiles(&self) -> usize {
        self.imp().tiles.borrow().len()
    }

    /// Show the first tiles, one for each `(width, height)`, and hide the
    /// rest.
    pub fn show(&self, sizes: &[(u32, u32)]) {
        let aspects: Vec<f32> = sizes
            .iter()
            .map(|&(w, h)| if w > 0 && h > 0 { (w as f32 / h as f32).clamp(0.1, 10.0) } else { 4.0 / 3.0 })
            .collect();
        for (i, t) in self.imp().tiles.borrow().iter().enumerate() {
            t.set_child_visible(i < aspects.len());
        }
        *self.imp().aspects.borrow_mut() = aspects;
        self.imp().rects.borrow_mut().clear();
        self.queue_resize();
    }

    /// The height the page shows, which is what a group is fitted into.
    pub fn set_viewport(&self, h: f64) {
        let h = h as f32;
        if (self.imp().viewport.get() - h).abs() >= 0.5 {
            self.imp().viewport.set(h);
            self.queue_resize();
        }
    }

    pub fn add_popover(&self, p: &gtk::PopoverMenu) {
        p.set_parent(self);
        self.imp().popovers.borrow_mut().push(p.clone().upcast());
    }

    /// The tile above or below tile `i`, as laid out now.
    pub fn vertical_neighbour(&self, i: usize, down: bool) -> Option<usize> {
        neighbour(&self.imp().rects.borrow(), i, down)
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
