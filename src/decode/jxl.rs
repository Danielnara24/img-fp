//! JPEG XL: its header parsed once for its facts and its size, its first
//! frame rendered and streamed a row at a time.

use super::budget::{max_alloc, too_large};
use super::picture::{claim, Facts, Layout, Orientation, Picture, Rows, Sink};
use anyhow::Result;
use std::io::Cursor;

/// A JPEG XL. Its facts are its header's: lossless (`lossless`), and more
/// than one picture when it is an animation, whose frames after the first
/// are pictures of their own.
pub(super) fn open(bytes: &[u8]) -> Result<Picture<'_>> {
    let image = image(bytes, max_alloc())?;
    let meta = &image.image_header().metadata;
    let facts = Facts {
        lossless: lossless(&image),
        frames: meta.animation.is_some(),
        exif: super::exif_in(bytes),
        deep: meta.bit_depth.bits_per_sample() > 8,
        profile: meta.colour_encoding.want_icc() || !meta.colour_encoding.is_srgb(),
        ..Facts::default()
    };
    Ok(Picture::new(move || facts, move |sink| read(image, bytes, sink)))
}

/// The first frame's rows into `sink`, as the render streams them: the
/// decoder orients the picture itself.
fn read(image: jxl_oxide::JxlImage, bytes: &[u8], sink: &mut dyn Sink) -> Result<()> {
    // What the render holds: two frames' worth of float planes at its widest,
    // measured — the decoded frame and the one the colour transform writes.
    // Claimed from the header, before the render that allocates them. The
    // rows are streamed out of the render into the sink, so nothing of the
    // picture is held beside it.
    let meta = &image.image_header().metadata;
    // Colour channels come first; a trailing channel is alpha if the header
    // says there is one. Anything else (black, spot) is ignored.
    let (colour, alpha) = (if meta.grayscale() { 1 } else { 3 }, meta.alpha().is_some());
    let (iw, ih) = (image.width() as usize, image.height() as usize);
    let _permit = claim(bytes, (iw as u64) * (ih as u64) * (colour + alpha as usize) as u64 * 8, sink, iw, ih);
    let render = image.render_frame(0).map_err(|e| anyhow::anyhow!("jxl render: {e}"))?;
    let mut stream = render.stream();
    let (w, h, ch) = (stream.width() as usize, stream.height() as usize, stream.channels() as usize);
    let layout = Layout::of(colour, alpha);
    let out_ch = layout.ch();
    // A row at a time: the stream remembers where it stopped, so a buffer one
    // row long reads the picture in the order a whole-picture buffer did, and
    // each byte is the one that buffer's conversion made. That buffer was a
    // float per sample — 154 MB for a 13.5-megapixel file, on top of the
    // render's own 306 and a byte copy of 38 — and it was the largest single
    // thing the benchmark corpus's run ever held.
    let mut rowf = vec![0f32; w * ch];
    let mut line = vec![0u8; w * out_ch];
    let wanted = sink.begin(&Rows { w, h, layout, orientation: Orientation::NoTransforms, grey_of_colour: false });
    for _ in 0..wanted {
        let got = stream.write_to_buffer(&mut rowf);
        // A stream shorter than its own header says reads as zeros, which is
        // what the whole buffer's unwritten tail held.
        rowf[got..].fill(0.0);
        for (px, out) in rowf.chunks_exact(ch).zip(line.chunks_exact_mut(out_ch)) {
            for (o, &v) in out.iter_mut().zip(px) {
                *o = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        sink.row(&line);
    }
    Ok(())
}

/// A JPEG XL decoder for `bytes`, held to `limit` bytes of allocation.
///
/// **The limit is `max_alloc`, as it is for every other decoder.** The image
/// crate, `zune-jpeg` and the PNG rows are all refused a picture larger than
/// the memory there is, with a message; this one was not, and a header can
/// claim any size at all. `jxl-oxide` sizes its frame buffers from the header
/// and allocates them whole, and an allocation that fails ends a Rust process
/// rather than returning — so a JPEG XL claiming sixty thousand pixels a side
/// took the whole run down with it instead of being one file that would not
/// decode. The header is checked first, against one float plane a channel at
/// the size shown, which is the least the render holds and refuses the absurd
/// case with a plain message; the decoder's own tracker then counts every
/// buffer it really makes against the same limit, and fails the decode, not
/// the process, when they pass it.
pub(super) fn image(bytes: &[u8], limit: u64) -> Result<jxl_oxide::JxlImage> {
    let image = jxl_oxide::JxlImage::builder()
        // jxl-oxide enables rayon by default and its default pool is the
        // *global* one — the same pool the per-image walk runs on. A decode
        // that dispatches there leaves its own thread waiting, and a waiting
        // rayon worker steals: the stolen task is another image, which decodes
        // and claims the budget while this thread's claim is still held.
        // Nothing here needs a second level of parallelism anyway; the images
        // are already one per thread.
        .pool(jxl_oxide::JxlThreadPool::none())
        .alloc_tracker(jxl_oxide::AllocTracker::with_limit(limit.min(usize::MAX as u64) as usize))
        .read(Cursor::new(bytes))
        .map_err(|e| anyhow::anyhow!("jxl: {e}"))?;
    let meta = &image.image_header().metadata;
    let channels = if meta.grayscale() { 1 } else { 3 } + meta.alpha().is_some() as u64;
    too_large("jxl", image.width(), image.height(), channels * 4, limit)?;
    Ok(image)
}

/// Whether a JPEG XL is lossless: its first frame is Modular and not in XYB,
/// which is what `cjxl -d 0` writes. A JPEG recompressed losslessly into JPEG
/// XL is VarDCT, and holds what the JPEG held: lossy.
fn lossless(image: &jxl_oxide::JxlImage) -> bool {
    !image.image_header().metadata.xyb_encoded
        && image.frame_header(0).is_some_and(|f| f.encoding == jxl_oxide::frame::Encoding::Modular)
}

/// A JPEG XL image's size, fed to the decoder a block at a time until the
/// header is complete, which is a few hundred bytes into the file.
pub(super) fn size(r: &mut impl std::io::Read) -> Option<(u32, u32)> {
    let mut uninit = jxl_oxide::JxlImage::builder().pool(jxl_oxide::JxlThreadPool::none()).build_uninit();
    let mut buf = vec![0u8; 4096];
    let mut valid = 0usize;
    // An ICC profile can sit ahead of the frame; a megabyte of header is a
    // file this is not going to learn anything cheap about.
    for _ in 0..256 {
        let n = r.read(&mut buf[valid..]).ok()?;
        if n == 0 {
            return None;
        }
        valid += n;
        let used = uninit.feed_bytes(&buf[..valid]).ok()?;
        buf.copy_within(used..valid, 0);
        valid -= used;
        match uninit.try_init().ok()? {
            jxl_oxide::InitializeResult::NeedMoreData(u) => uninit = u,
            jxl_oxide::InitializeResult::Initialized(img) => return Some((img.width(), img.height())),
        }
    }
    None
}
