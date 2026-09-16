//! File discovery and decoding to a single-channel working image.
//!
//! Formats are identified by content, not extension: a file with no extension
//! (or a wrong one) still decodes if its bytes are an image. Every path ends in
//! the same place — a `Gray` of f32 in 0..=1 whose long side is at most the
//! requested working size — so the rest of the pipeline never sees a format.
//!
//! The channel used is the mean of R, G and B rather than a luma weighting.
//! Mean is invariant to any permutation of the channels, which luma is not, so
//! a channel-swapped copy of an image yields the same working image.

use anyhow::{bail, Context, Result};
use image::{DynamicImage, ImageDecoder, ImageFormat};
use std::io::Cursor;
use std::path::Path;

/// Single-channel image, row-major, values in 0..=1.
#[derive(Clone, Debug)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Gray {
    pub fn new(w: usize, h: usize) -> Self {
        Gray { w, h, px: vec![0.0; w * h] }
    }
}

pub struct Decoded {
    /// Long side <= the working size passed to `decode`.
    pub work: Gray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image(ImageFormat),
    Jxl,
    Heif,
    Unknown,
}

const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "heic",
    "heif", "hif", "jxl", "ico", "pnm", "pbm", "pgm", "ppm", "tga", "dds", "qoi", "exr", "ff",
];

/// Cheap pre-filter on the path: known image extension, or no extension at
/// all (which then gets sniffed). Files with an unrelated extension are
/// skipped without being opened.
pub fn looks_like_image(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext = ext.to_ascii_lowercase();
            EXTENSIONS.contains(&ext.as_str())
        }
        None => true,
    }
}

/// Identify a format from the first bytes.
pub fn sniff(b: &[u8]) -> Kind {
    if b.len() < 12 {
        return Kind::Unknown;
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Kind::Image(ImageFormat::Jpeg);
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Kind::Image(ImageFormat::Png);
    }
    if b.starts_with(b"GIF8") {
        return Kind::Image(ImageFormat::Gif);
    }
    if b.starts_with(b"RIFF") && &b[8..12] == b"WEBP" {
        return Kind::Image(ImageFormat::WebP);
    }
    if b.starts_with(b"BM") {
        return Kind::Image(ImageFormat::Bmp);
    }
    if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
        return Kind::Image(ImageFormat::Tiff);
    }
    if b.starts_with(&[0xFF, 0x0A]) || b.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return Kind::Jxl;
    }
    if &b[4..8] == b"ftyp" {
        // ISO BMFF: HEIC/HEIF/AVIF brands all go to libheif.
        let brand = &b[8..12];
        if matches!(
            brand,
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"mif1" | b"msf1"
                | b"avif" | b"avis"
        ) {
            return Kind::Heif;
        }
    }
    if b.starts_with(b"qoif") {
        return Kind::Image(ImageFormat::Qoi);
    }
    if b.starts_with(&[0x76, 0x2F, 0x31, 0x01]) {
        return Kind::Image(ImageFormat::OpenExr);
    }
    if b.starts_with(b"farbfeld") {
        return Kind::Image(ImageFormat::Farbfeld);
    }
    if b[0] == b'P' && (b'1'..=b'7').contains(&b[1]) {
        return Kind::Image(ImageFormat::Pnm);
    }
    if b.starts_with(&[0, 0, 1, 0]) {
        return Kind::Image(ImageFormat::Ico);
    }
    Kind::Unknown
}

/// Decode a file to a working-resolution gray image.
pub fn decode(path: &Path, work_size: usize) -> Result<Decoded> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let kind = sniff(&bytes);
    let (w, h, gray) = match kind {
        Kind::Image(fmt) => decode_image_crate(&bytes, fmt, work_size)?,
        Kind::Jxl => decode_jxl(&bytes, work_size)?,
        Kind::Heif => decode_heif(&bytes, work_size)?,
        Kind::Unknown => {
            // Last resort: let the image crate guess (covers TGA/DDS, which
            // have no magic), then give up.
            match image::guess_format(&bytes) {
                Ok(fmt) => decode_image_crate(&bytes, fmt, work_size)?,
                Err(_) => bail!("not an image"),
            }
        }
    };
    let _ = (w, h);
    Ok(Decoded { work: gray })
}

fn decode_image_crate(bytes: &[u8], fmt: ImageFormat, work: usize) -> Result<(u32, u32, Gray)> {
    let reader = image::ImageReader::with_format(Cursor::new(bytes), fmt);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)?;
    if orientation != image::metadata::Orientation::NoTransforms {
        img.apply_orientation(orientation);
    }
    let (w, h) = (img.width(), img.height());
    let gray = dynamic_to_gray(&img, work);
    Ok((w, h, gray))
}

/// Mean of RGB, alpha flattened onto mid-grey, box-reduced towards the
/// working size in the same pass so a 50-megapixel file never exists as f32.
fn dynamic_to_gray(img: &DynamicImage, work: usize) -> Gray {
    let (w, h) = (img.width() as usize, img.height() as usize);
    match img {
        DynamicImage::ImageRgb8(b) => reduce_to_gray(w, h, b.as_raw(), 3, false, work),
        DynamicImage::ImageRgba8(b) => reduce_to_gray(w, h, b.as_raw(), 4, true, work),
        DynamicImage::ImageLuma8(b) => reduce_to_gray(w, h, b.as_raw(), 1, false, work),
        DynamicImage::ImageLumaA8(b) => reduce_to_gray(w, h, b.as_raw(), 2, true, work),
        _ => {
            let b = img.to_rgba8();
            reduce_to_gray(w, h, b.as_raw(), 4, true, work)
        }
    }
}

/// Integer box factor that keeps the long side at or above 2x the working
/// size (or 1), so the final area resample still has enough to average.
fn box_factor(w: usize, h: usize, work: usize) -> usize {
    let long = w.max(h);
    if work == 0 {
        return 1;
    }
    (long / (2 * work)).max(1)
}

pub fn reduce_to_gray(w: usize, h: usize, data: &[u8], ch: usize, alpha: bool, work: usize) -> Gray {
    let k = box_factor(w, h, work);
    let ow = (w / k).max(1);
    let oh = (h / k).max(1);
    let mut out = Gray::new(ow, oh);
    let color_ch = if alpha { ch - 1 } else { ch };
    let inv = 1.0 / (255.0 * (k * k) as f32);
    for oy in 0..oh {
        let row = &mut out.px[oy * ow..(oy + 1) * ow];
        for sy in oy * k..(oy * k + k).min(h) {
            let line = &data[sy * w * ch..(sy + 1) * w * ch];
            for ox in 0..ow {
                let mut acc = 0.0f32;
                for sx in ox * k..(ox * k + k).min(w) {
                    let p = &line[sx * ch..sx * ch + ch];
                    let mut v = 0u32;
                    for c in 0..color_ch {
                        v += p[c] as u32;
                    }
                    let mut g = v as f32 / color_ch as f32;
                    if alpha {
                        let a = p[ch - 1] as f32 / 255.0;
                        g = g * a + 128.0 * (1.0 - a);
                    }
                    acc += g;
                }
                row[ox] += acc;
            }
        }
        for v in row.iter_mut() {
            *v *= inv;
        }
    }
    // Edge pixels lost to the floor division are ignored on purpose: at most
    // k-1 rows/cols of a picture already 2x the working size.
    fit_to(out, work)
}

/// Area-resample so the long side is exactly `work` (never upscales).
pub fn fit_to(g: Gray, work: usize) -> Gray {
    let long = g.w.max(g.h);
    if work == 0 || long <= work {
        return g;
    }
    let s = work as f32 / long as f32;
    let tw = ((g.w as f32 * s).round() as usize).max(1);
    let th = ((g.h as f32 * s).round() as usize).max(1);
    resize_area(&g, tw, th)
}

/// Separable area (box with fractional edges) resampling. Exact averaging of
/// source pixel coverage, which is what a good downscaler does and what keeps
/// a 25-pixel thumbnail comparable to a 25-pixel thumbnail made by Pillow.
pub fn resize_area(g: &Gray, tw: usize, th: usize) -> Gray {
    if tw == g.w && th == g.h {
        return g.clone();
    }
    let xw = weights(g.w, tw);
    let yw = weights(g.h, th);
    // horizontal pass
    let mut tmp = vec![0.0f32; tw * g.h];
    for y in 0..g.h {
        let src = &g.px[y * g.w..(y + 1) * g.w];
        let dst = &mut tmp[y * tw..(y + 1) * tw];
        for (ox, (start, ws)) in xw.iter().enumerate() {
            let mut acc = 0.0;
            for (i, wgt) in ws.iter().enumerate() {
                acc += src[start + i] * wgt;
            }
            dst[ox] = acc;
        }
    }
    let mut out = Gray::new(tw, th);
    for (oy, (start, ws)) in yw.iter().enumerate() {
        let dst = &mut out.px[oy * tw..(oy + 1) * tw];
        for (i, wgt) in ws.iter().enumerate() {
            let src = &tmp[(start + i) * tw..(start + i + 1) * tw];
            for x in 0..tw {
                dst[x] += src[x] * wgt;
            }
        }
    }
    out
}

/// For each output index: (first source index, normalised weights) covering
/// the source interval [o*s, (o+1)*s) where s = src/dst. Also correct when
/// upscaling (weights become mostly a single 1.0, i.e. nearest-ish box).
fn weights(src: usize, dst: usize) -> Vec<(usize, Vec<f32>)> {
    let s = src as f64 / dst as f64;
    (0..dst)
        .map(|o| {
            let a = o as f64 * s;
            let b = ((o + 1) as f64 * s).min(src as f64);
            let i0 = a.floor() as usize;
            let i1 = (b.ceil() as usize).min(src).max(i0 + 1);
            let mut ws = Vec::with_capacity(i1 - i0);
            let mut total = 0.0;
            for i in i0..i1 {
                let lo = (i as f64).max(a);
                let hi = ((i + 1) as f64).min(b);
                let wgt = (hi - lo).max(0.0);
                ws.push(wgt as f32);
                total += wgt;
            }
            let inv = if total > 0.0 { (1.0 / total) as f32 } else { 0.0 };
            for w in ws.iter_mut() {
                *w *= inv;
            }
            (i0, ws)
        })
        .collect()
}

fn decode_jxl(bytes: &[u8], work: usize) -> Result<(u32, u32, Gray)> {
    let image = jxl_oxide::JxlImage::builder()
        .read(Cursor::new(bytes))
        .map_err(|e| anyhow::anyhow!("jxl: {e}"))?;
    let render = image
        .render_frame(0)
        .map_err(|e| anyhow::anyhow!("jxl render: {e}"))?;
    let mut stream = render.stream();
    let (w, h, ch) = (stream.width() as usize, stream.height() as usize, stream.channels() as usize);
    let mut buf = vec![0f32; w * h * ch];
    stream.write_to_buffer(&mut buf);
    // Colour channels come first; a trailing channel is alpha if the header
    // says there is one. Anything else (black, spot) is ignored.
    let color = if image.image_header().metadata.grayscale() { 1 } else { 3 };
    let has_alpha = image.image_header().metadata.alpha().is_some();
    let mut u8buf = Vec::with_capacity(w * h * (color + has_alpha as usize));
    for px in buf.chunks_exact(ch) {
        for c in 0..color {
            u8buf.push((px[c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
        if has_alpha {
            u8buf.push((px[color].clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
        }
    }
    let g = reduce_to_gray(w, h, &u8buf, color + has_alpha as usize, has_alpha, work);
    Ok((w as u32, h as u32, g))
}

fn decode_heif(bytes: &[u8], work: usize) -> Result<(u32, u32, Gray)> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_bytes(bytes).map_err(|e| anyhow::anyhow!("heif: {e}"))?;
    let handle = ctx.primary_image_handle().map_err(|e| anyhow::anyhow!("heif: {e}"))?;
    let has_alpha = handle.has_alpha_channel();
    let chroma = if has_alpha { RgbChroma::Rgba } else { RgbChroma::Rgb };
    let img = lib
        .decode(&handle, ColorSpace::Rgb(chroma), None)
        .map_err(|e| anyhow::anyhow!("heif decode: {e}"))?;
    let planes = img.planes();
    let plane = planes.interleaved.context("heif: no interleaved plane")?;
    let (w, h) = (plane.width as usize, plane.height as usize);
    let ch = if has_alpha { 4 } else { 3 };
    // Re-pack rows to remove the stride padding.
    let mut packed = Vec::with_capacity(w * h * ch);
    for y in 0..h {
        let row = &plane.data[y * plane.stride..y * plane.stride + w * ch];
        packed.extend_from_slice(row);
    }
    let g = reduce_to_gray(w, h, &packed, ch, has_alpha, work);
    Ok((w as u32, h as u32, g))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_resize_preserves_mean() {
        let mut g = Gray::new(10, 7);
        for (i, v) in g.px.iter_mut().enumerate() {
            *v = (i % 13) as f32 / 12.0;
        }
        let mean: f32 = g.px.iter().sum::<f32>() / g.px.len() as f32;
        let r = resize_area(&g, 3, 2);
        let m2: f32 = r.px.iter().sum::<f32>() / r.px.len() as f32;
        assert!((mean - m2).abs() < 0.05, "{mean} vs {m2}");
    }

    #[test]
    fn sniff_basics() {
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0\0\x10JFIF\0\x01\x01"), Kind::Image(ImageFormat::Jpeg));
        assert_eq!(sniff(b"\0\0\0\x18ftypavif\0\0\0\0"), Kind::Heif);
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Kind::Image(ImageFormat::WebP));
    }
}
