//! Page rendering: display list, then raster, then PNG.
//!
//! The display list is built on the document's thread. `DisplayList` is `Send + Sync`
//! (ADR 0001), so rasterizing and encoding can run on worker threads.

use std::time::{Duration, Instant};

use mupdf::{Colorspace, DisplayList, Document, Matrix, Pixmap};

use crate::error::{Error, Result};

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
    if !(scale.is_finite() && scale > 0.0 && scale <= 64.0) {
        return Err(Error::InvalidArgument(format!(
            "render scale {scale} is out of range"
        )));
    }
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
