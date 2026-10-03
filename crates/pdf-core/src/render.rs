//! Page rendering: display list, then raster, then RGBA pixels (or PNG).
//!
//! The display list is built on the document's thread. `DisplayList` is `Send + Sync`
//! (ADR 0001), so rasterizing and encoding can run on worker threads.
//!
//! The app shows raw RGBA drawn into a `<canvas>` (AGENTS.md section 3: PNG encoding took
//! over 30% of render time in Phase 0). Above a zoom threshold the frontend asks for
//! 512 px tiles instead of whole pages. Rendered images are kept in an [`ImageCache`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mupdf::{Colorspace, Device, DisplayList, Document, IRect, Matrix, Pixmap};

use crate::error::{Error, Result};

/// Edge length of a render tile in device pixels.
pub const TILE_SIZE: u32 = 512;

/// Largest render scale accepted (6400% at 72 dpi).
pub const MAX_SCALE: f32 = 64.0;

/// Largest image a single request may produce, in pixels (whole page or tile). Larger
/// pages must be requested as tiles.
pub const MAX_PIXELS: u64 = 40_000_000;

/// The pixel size of a page that is `width` x `height` points at `scale`, rounded the
/// way MuPDF rounds device rectangles (`fz_round_rect`).
pub fn pixel_size(width: f32, height: f32, scale: f32) -> (u32, u32) {
    // A page is at most 14,400 in (the PDF maximum) and the scale is capped, so the
    // result fits easily in u32.
    let round = |v: f32| (v - 0.001).ceil().max(1.0) as u32;
    (round(width * scale), round(height * scale))
}

/// A region of the rendered page in device pixels (origin at the page's top-left corner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Opaque RGBA pixels, 4 bytes per pixel, rows packed without padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

fn check_scale(scale: f32) -> Result<()> {
    if scale.is_finite() && scale > 0.0 && scale <= MAX_SCALE {
        Ok(())
    } else {
        Err(Error::InvalidArgument(format!(
            "render scale {scale} is out of range"
        )))
    }
}

/// Renders a display list at `scale` to opaque RGBA on white: the whole page, or only
/// `region` (clipped to the page) for a tile.
pub fn render_rgba(list: &DisplayList, scale: f32, region: Option<PixelRect>) -> Result<RgbaImage> {
    check_scale(scale)?;
    let bounds = list.bounds();
    let (page_w, page_h) = pixel_size(bounds.width(), bounds.height(), scale);
    let area = match region {
        None => PixelRect {
            x: 0,
            y: 0,
            width: page_w,
            height: page_h,
        },
        Some(r) => {
            let x1 = r.x.saturating_add(r.width).min(page_w);
            let y1 = r.y.saturating_add(r.height).min(page_h);
            if r.x >= x1 || r.y >= y1 {
                return Err(Error::InvalidArgument("tile is outside the page".into()));
            }
            PixelRect {
                x: r.x,
                y: r.y,
                width: x1 - r.x,
                height: y1 - r.y,
            }
        }
    };
    if u64::from(area.width) * u64::from(area.height) > MAX_PIXELS {
        return Err(Error::InvalidArgument(format!(
            "a {}x{} px image is too large; request tiles instead",
            area.width, area.height
        )));
    }
    let to_i32 =
        |v: u32| i32::try_from(v).map_err(|_| Error::InvalidArgument("image too large".into()));
    let (x, y, w, h) = (
        to_i32(area.x)?,
        to_i32(area.y)?,
        to_i32(area.width)?,
        to_i32(area.height)?,
    );

    // An RGBA pixmap whose origin is the tile's position in page pixels; MuPDF clips
    // drawing to it. Cleared to opaque white, so premultiplied and straight alpha agree
    // and the canvas can take the bytes as they are.
    let mut pixmap = Pixmap::new(&Colorspace::device_rgb(), x, y, w, h, true)?;
    pixmap.clear_with(255)?;
    {
        let device = Device::from_pixmap_with_clip(&pixmap, IRect::new(x, y, x + w, y + h))?;
        let ctm = Matrix::new_scale(scale, scale);
        // The scissor is in device space (after `ctm`): the tile's pixel rectangle.
        let scissor = mupdf::Rect::new(x as f32, y as f32, (x + w) as f32, (y + h) as f32);
        list.run(&device, &ctm, scissor)?;
        // Dropping the device closes it, which flushes pending drawing into the pixmap.
    }
    Ok(RgbaImage {
        width: area.width,
        height: area.height,
        data: packed_samples(&pixmap, 4)?,
    })
}

/// The pixmap's samples with row padding removed.
fn packed_samples(pixmap: &Pixmap, n: usize) -> Result<Vec<u8>> {
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let row_bytes = width * n;
    let stride = usize::try_from(pixmap.stride())
        .map_err(|_| Error::InvalidArgument("negative pixmap stride".into()))?;
    let samples = pixmap.samples();
    if stride == row_bytes {
        return Ok(samples[..row_bytes * height].to_vec());
    }
    let mut packed = Vec::with_capacity(row_bytes * height);
    for row in samples.chunks(stride).take(height) {
        packed.extend_from_slice(&row[..row_bytes]);
    }
    Ok(packed)
}

/// Display list of page `index`, including annotations and widgets.
pub fn display_list(doc: &Document, index: usize) -> Result<DisplayList> {
    display_list_with(doc, index, true)
}

/// Display list of page `index`; `annotations: false` draws page content only (used as the
/// baseline in the interop harness).
pub fn display_list_with(doc: &Document, index: usize, annotations: bool) -> Result<DisplayList> {
    let page_no = i32::try_from(index).map_err(|_| Error::PageOutOfRange(index))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(index));
    }
    let page = doc.load_page(page_no)?;
    Ok(page.to_display_list(annotations)?)
}

/// Rasterizes a display list at `scale` (1.0 = 72 dpi) to opaque RGB.
pub fn rasterize(list: &DisplayList, scale: f32) -> Result<Pixmap> {
    check_scale(scale)?;
    Ok(list.to_pixmap(
        &Matrix::new_scale(scale, scale),
        &Colorspace::device_rgb(),
        false,
    )?)
}

/// Encodes an RGB or RGBA pixmap as PNG with the fastest compression (AGENTS.md section 3).
pub fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>> {
    let (width, height) = (pixmap.width(), pixmap.height());
    let color = match (pixmap.n(), pixmap.alpha()) {
        (3, false) => png::ColorType::Rgb,
        (4, true) => png::ColorType::Rgba,
        (n, alpha) => {
            return Err(Error::InvalidArgument(format!(
                "cannot encode a pixmap with {n} components (alpha: {alpha}) as PNG"
            )));
        }
    };
    let row_bytes = width as usize * usize::from(pixmap.n());
    let stride = usize::try_from(pixmap.stride())
        .map_err(|_| Error::InvalidArgument("negative pixmap stride".into()))?;
    let samples = pixmap.samples();

    let mut out = Vec::with_capacity(row_bytes * height as usize / 4);
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fastest);
    let mut writer = encoder.write_header().map_err(png_error)?;
    if stride == row_bytes {
        writer
            .write_image_data(&samples[..row_bytes * height as usize])
            .map_err(png_error)?;
    } else {
        let mut packed = Vec::with_capacity(row_bytes * height as usize);
        for row in samples.chunks(stride).take(height as usize) {
            packed.extend_from_slice(&row[..row_bytes]);
        }
        writer.write_image_data(&packed).map_err(png_error)?;
    }
    writer.finish().map_err(png_error)?;
    Ok(out)
}

fn png_error(e: png::EncodingError) -> Error {
    Error::Io(std::io::Error::other(e))
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RenderTimings {
    pub display_list: Duration,
    pub raster: Duration,
    pub encode: Duration,
}

impl RenderTimings {
    pub fn render(&self) -> Duration {
        self.display_list + self.raster
    }
}

pub struct RenderedPng {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
    pub timings: RenderTimings,
}

/// Renders page `index` at `scale` to PNG, measuring each stage.
pub fn render_png(doc: &Document, index: usize, scale: f32) -> Result<RenderedPng> {
    render_png_with(doc, index, scale, true)
}

pub fn render_png_with(
    doc: &Document,
    index: usize,
    scale: f32,
    annotations: bool,
) -> Result<RenderedPng> {
    let t0 = Instant::now();
    let list = display_list_with(doc, index, annotations)?;
    let t1 = Instant::now();
    let pixmap = rasterize(&list, scale)?;
    let t2 = Instant::now();
    let png = encode_png(&pixmap)?;
    let t3 = Instant::now();
    Ok(RenderedPng {
        width: pixmap.width(),
        height: pixmap.height(),
        png,
        timings: RenderTimings {
            display_list: t1 - t0,
            raster: t2 - t1,
            encode: t3 - t2,
        },
    })
}

/// Identifies one rendered image. The revision is part of the key, so a changed document
/// never serves stale pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageKey {
    pub doc: u32,
    pub page: usize,
    pub revision: u64,
    /// The render scale in thousandths (the frontend sends scales from a fixed ladder).
    pub scale_milli: u32,
    /// `None` for a whole page.
    pub tile: Option<PixelRect>,
}

impl ImageKey {
    pub fn scale_milli(scale: f32) -> u32 {
        // Callers validate the scale first (at most MAX_SCALE), so this fits.
        (scale * 1000.0).round() as u32
    }
}

struct CacheEntry {
    image: Arc<RgbaImage>,
    last_used: u64,
}

#[derive(Default)]
struct CacheInner {
    entries: HashMap<ImageKey, CacheEntry>,
    bytes: usize,
    clock: u64,
}

/// A least-recently-used cache of rendered images with a memory cap.
pub struct ImageCache {
    cap_bytes: usize,
    inner: Mutex<CacheInner>,
}

impl ImageCache {
    pub fn new(cap_bytes: usize) -> Self {
        ImageCache {
            cap_bytes,
            inner: Mutex::new(CacheInner::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CacheInner> {
        // Every update below leaves the cache consistent before it can panic, so a
        // poisoned lock is still safe to use.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self, key: &ImageKey) -> Option<Arc<RgbaImage>> {
        let mut inner = self.lock();
        inner.clock += 1;
        let clock = inner.clock;
        let entry = inner.entries.get_mut(key)?;
        entry.last_used = clock;
        Some(entry.image.clone())
    }

    /// Stores an image, evicting the least recently used ones beyond the cap. An image
    /// larger than the whole cap is not stored.
    pub fn insert(&self, key: ImageKey, image: Arc<RgbaImage>) {
        let size = image.data.len();
        if size > self.cap_bytes {
            return;
        }
        let mut inner = self.lock();
        inner.clock += 1;
        let entry = CacheEntry {
            image,
            last_used: inner.clock,
        };
        if let Some(old) = inner.entries.insert(key, entry) {
            inner.bytes -= old.image.data.len();
        }
        inner.bytes += size;
        while inner.bytes > self.cap_bytes {
            let oldest = inner
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| *k);
            let Some(evicted) = oldest.and_then(|k| inner.entries.remove(&k)) else {
                break;
            };
            inner.bytes -= evicted.image.data.len();
        }
    }

    /// Drops every image of document `doc` (when it closes).
    pub fn remove_document(&self, doc: u32) {
        let mut inner = self.lock();
        let mut freed = 0;
        inner.entries.retain(|k, e| {
            let keep = k.doc != doc;
            if !keep {
                freed += e.image.data.len();
            }
            keep
        });
        inner.bytes -= freed;
    }

    /// Bytes currently held.
    pub fn bytes(&self) -> usize {
        self.lock().bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};

    #[test]
    fn pixel_size_rounds_like_mupdf() {
        assert_eq!(pixel_size(612.0, 792.0, 1.0), (612, 792));
        assert_eq!(pixel_size(612.0, 792.0, 1.5), (918, 1188));
        assert_eq!(pixel_size(100.4, 10.0, 1.0), (101, 10));
        assert_eq!(pixel_size(0.0, 0.0, 1.0), (1, 1));
    }

    #[test]
    fn rgba_matches_rgb_render() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = display_list(&doc, 0).unwrap();
        let rgba = render_rgba(&list, 1.25, None).unwrap();
        let rgb = rasterize(&list, 1.25).unwrap();
        assert_eq!((rgba.width, rgba.height), (rgb.width(), rgb.height()));
        let rgb = packed_samples(&rgb, 3).unwrap();
        assert_eq!(rgba.data.len(), rgb.len() / 3 * 4);
        for (px4, px3) in rgba.data.chunks(4).zip(rgb.chunks(3)) {
            assert_eq!(&px4[..3], px3);
            assert_eq!(px4[3], 255);
        }
    }

    #[test]
    fn tiles_reassemble_the_page() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = display_list(&doc, 0).unwrap();
        let scale = 2.0;
        let page = render_rgba(&list, scale, None).unwrap();
        let mut assembled = vec![0u8; page.data.len()];
        for y in (0..page.height).step_by(TILE_SIZE as usize) {
            for x in (0..page.width).step_by(TILE_SIZE as usize) {
                let region = PixelRect {
                    x,
                    y,
                    width: TILE_SIZE,
                    height: TILE_SIZE,
                };
                let tile = render_rgba(&list, scale, Some(region)).unwrap();
                assert!(tile.width <= TILE_SIZE && tile.height <= TILE_SIZE);
                let len = (tile.width * 4) as usize;
                for row in 0..tile.height {
                    let src = (row * tile.width * 4) as usize;
                    let dst = (((y + row) * page.width + x) * 4) as usize;
                    assembled[dst..dst + len].copy_from_slice(&tile.data[src..src + len]);
                }
            }
        }
        // Antialiasing at tile seams may differ by a level or two; nothing else may.
        let max_diff = assembled
            .iter()
            .zip(&page.data)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max_diff <= 2, "tiles differ from the page by {max_diff}");
    }

    #[test]
    fn rejects_bad_requests() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = display_list(&doc, 0).unwrap();
        assert!(render_rgba(&list, 0.0, None).is_err());
        assert!(render_rgba(&list, f32::NAN, None).is_err());
        let outside = PixelRect {
            x: 5000,
            y: 0,
            width: 512,
            height: 512,
        };
        assert!(render_rgba(&list, 1.0, Some(outside)).is_err());
        // 612x792 pt at 12x is about 70 megapixels: too large for one image.
        assert!(render_rgba(&list, 12.0, None).is_err());
    }

    fn image(bytes: usize) -> Arc<RgbaImage> {
        Arc::new(RgbaImage {
            width: 1,
            height: 1,
            data: vec![0; bytes],
        })
    }

    fn key(doc: u32, page: usize) -> ImageKey {
        ImageKey {
            doc,
            page,
            revision: 0,
            scale_milli: 1000,
            tile: None,
        }
    }

    #[test]
    fn cache_evicts_least_recently_used() {
        let cache = ImageCache::new(300);
        cache.insert(key(1, 0), image(100));
        cache.insert(key(1, 1), image(100));
        cache.insert(key(1, 2), image(100));
        assert!(cache.get(&key(1, 0)).is_some()); // page 1 is now the oldest
        cache.insert(key(1, 3), image(100));
        assert!(cache.get(&key(1, 1)).is_none());
        assert!(cache.get(&key(1, 0)).is_some());
        assert_eq!(cache.bytes(), 300);
        cache.insert(key(1, 4), image(1000)); // larger than the cap: not stored
        assert!(cache.get(&key(1, 4)).is_none());
        assert_eq!(cache.bytes(), 300);
    }

    #[test]
    fn cache_separates_revisions_and_documents() {
        let cache = ImageCache::new(1000);
        cache.insert(key(1, 0), image(10));
        let newer = ImageKey {
            revision: 1,
            ..key(1, 0)
        };
        assert!(cache.get(&newer).is_none());
        cache.insert(key(2, 0), image(10));
        cache.remove_document(1);
        assert!(cache.get(&key(1, 0)).is_none());
        assert!(cache.get(&key(2, 0)).is_some());
        assert_eq!(cache.bytes(), 10);
    }
}
