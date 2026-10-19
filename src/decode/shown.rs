//! A picture for a person to look at: in colour, the right way up, and, made
//! for a place, reduced as it is read where its decoder gives rows.

use super::picture::{shown_size, Layout, Orientation, Rows, Sink};
use anyhow::{Context, Result};
use image::DynamicImage;

/// The preview's sink. Made for a place, a picture large enough to be
/// reduced by a whole factor first is boxed down row by row (`RowBox`), so
/// it never exists whole; anything else is kept whole, as the decoder gives
/// it, to be shrunk by `fit_preview` or `preview`.
pub(super) struct Shown {
    place: Option<(u32, u32, bool)>,
    rows: Option<(RowBox, Orientation)>,
    whole: Option<DynamicImage>,
}

impl Shown {
    pub fn new(place: Option<(u32, u32, bool)>) -> Shown {
        Shown { place, rows: None, whole: None }
    }

    /// The largest whole factor that keeps a picture shown `w` x `h` at
    /// least the size `preview_size` asks for its place; 1 without a place.
    fn factor(&self, w: usize, h: usize) -> usize {
        self.place.map_or(1, |(pw, ph, cover)| {
            let (sw, sh) = preview_size(w as u32, h as u32, pw, ph, cover);
            ((w as u32 / sw).min(h as u32 / sh)).max(1) as usize
        })
    }

    /// The picture, the right way up.
    pub fn finish(self) -> Result<DynamicImage> {
        if let Some(img) = self.whole {
            return Ok(img);
        }
        let (boxes, orientation) = self.rows.context("the decoder gave no picture")?;
        let mut img = boxes.finish().context("frame size")?;
        img.apply_orientation(orientation);
        Ok(img)
    }
}

impl Sink for Shown {
    /// Rows only to be reduced: a picture kept whole is kept at the
    /// decoder's own depth, which `fit_preview` and `preview` shrink from.
    fn streams(&self, w: usize, h: usize) -> bool {
        self.factor(w, h) >= 2
    }

    fn grey(&self) -> bool {
        false
    }

    fn holds(&self, _: usize, _: usize) -> u64 {
        0
    }

    fn begin(&mut self, rows: &Rows) -> usize {
        let (sw, sh) = shown_size(rows.w as u32, rows.h as u32, rows.orientation);
        let boxes = RowBox::new(rows.w, rows.h, rows.layout, self.factor(sw as usize, sh as usize));
        let n = boxes.rows_wanted();
        self.whole = None;
        self.rows = Some((boxes, rows.orientation));
        n
    }

    fn row(&mut self, line: &[u8]) {
        if let Some((boxes, _)) = &mut self.rows {
            boxes.push(line);
        }
    }

    fn whole(&mut self, mut img: DynamicImage, orientation: Orientation) {
        img.apply_orientation(orientation);
        self.rows = None;
        self.whole = Some(img);
    }
}

/// A decoded picture made for a place `w` x `h` pixels, as `preview_fit`
/// makes one: RGBA.
pub fn fit_preview(img: DynamicImage, w: u32, h: u32, cover: bool) -> (u32, u32, Vec<u8>) {
    let (w, h) = (w.max(1), h.max(1));
    let (iw, ih) = (img.width(), img.height());
    let (sw, sh) = preview_size(iw, ih, w, h, cover);
    let img = if (sw, sh) != (iw, ih) { img.thumbnail_exact(sw, sh) } else { img };
    let (cw, ch) = if cover { (sw.min(w), sh.min(h)) } else { (sw, sh) };
    let img = if (cw, ch) != (sw, sh) { img.crop_imm((sw - cw) / 2, (sh - ch) / 2, cw, ch) } else { img };
    let rgba = img.into_rgba8();
    (rgba.width(), rgba.height(), rgba.into_raw())
}

/// The size a `iw` x `ih` picture is shrunk to for a `w` x `h` place: to fit
/// inside it, or with `cover` to fill it; never larger than it is.
fn preview_size(iw: u32, ih: u32, w: u32, h: u32, cover: bool) -> (u32, u32) {
    let (sx, sy) = (w as f64 / iw.max(1) as f64, h as f64 / ih.max(1) as f64);
    let s = if cover { sx.max(sy) } else { sx.min(sy) }.min(1.0);
    let at = |n: u32| ((n as f64 * s).round() as u32).clamp(1, n.max(1));
    (at(iw), at(ih))
}

/// Rows of `ch` 8-bit channels, `w` pixels wide, averaged over `k` x `k`
/// boxes as they arrive; a remainder at the right or bottom edge that makes
/// no whole box is dropped. Only the boxes' row of sums is held, never the
/// picture.
struct RowBox {
    k: usize,
    ch: usize,
    h: usize,
    acc: Vec<u32>,
    out: Vec<u8>,
    seen: usize,
}

impl RowBox {
    fn new(w: usize, h: usize, layout: Layout, k: usize) -> RowBox {
        let (ch, k) = (layout.ch(), k.max(1));
        let (ow, oh) = (w / k, h / k);
        RowBox { k, ch, h: oh, acc: vec![0; ow * ch], out: Vec::with_capacity(ow * oh * ch), seen: 0 }
    }

    /// The rows that make whole boxes; the rest need not be read.
    fn rows_wanted(&self) -> usize {
        self.h * self.k
    }

    fn push(&mut self, row: &[u8]) {
        let (k, ch) = (self.k, self.ch);
        for (a, px) in self.acc.chunks_exact_mut(ch).zip(row.chunks_exact(k * ch)) {
            for p in px.chunks_exact(ch) {
                for (s, &v) in a.iter_mut().zip(p) {
                    *s += v as u32;
                }
            }
        }
        self.seen += 1;
        if self.seen.is_multiple_of(k) {
            let area = (k * k) as u32;
            self.out.extend(self.acc.iter().map(|&s| ((s + area / 2) / area) as u8));
            self.acc.fill(0);
        }
    }

    fn finish(self) -> Option<DynamicImage> {
        let ow = (self.acc.len() / self.ch) as u32;
        let oh = self.h as u32;
        Some(match self.ch {
            1 => DynamicImage::ImageLuma8(image::GrayImage::from_raw(ow, oh, self.out)?),
            2 => DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_raw(ow, oh, self.out)?),
            3 => DynamicImage::ImageRgb8(image::RgbImage::from_raw(ow, oh, self.out)?),
            _ => DynamicImage::ImageRgba8(image::RgbaImage::from_raw(ow, oh, self.out)?),
        })
    }
}
