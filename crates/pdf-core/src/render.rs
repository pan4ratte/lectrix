//! Page rendering: display list, then raster, then RGBA pixels (or PNG).
//!
//! The display list is built on the document's thread. `DisplayList` is `Send + Sync`
//! (ADR 0001), so rasterizing and encoding can run on worker threads.
//!
//! Pages are rendered to opaque RGBA and kept in an [`ImageCache`]; the app sends them to
//! the webview as PNG encoded from that RGBA (ADR 0004). Above a zoom threshold the
//! frontend asks for 512 px tiles instead of whole pages.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mupdf::pdf::{PdfAnnotationType, PdfDocument};
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
    let area = image_area(list, scale, region)?;
    Ok(RgbaImage {
        width: area.width,
        height: area.height,
        data: draw(list, scale, area, None)?,
    })
}

/// The part of the page an image at `scale` covers: all of it, or `region` clipped to it.
fn image_area(list: &DisplayList, scale: f32, region: Option<PixelRect>) -> Result<PixelRect> {
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
    Ok(area)
}

/// Draws a display list at `scale` into `area` of the page, over opaque white or over
/// `under` (opaque pixels of that same area), and returns the packed RGBA.
fn draw(list: &DisplayList, scale: f32, area: PixelRect, under: Option<&[u8]>) -> Result<Vec<u8>> {
    let to_i32 =
        |v: u32| i32::try_from(v).map_err(|_| Error::InvalidArgument("image too large".into()));
    let (x, y, w, h) = (
        to_i32(area.x)?,
        to_i32(area.y)?,
        to_i32(area.width)?,
        to_i32(area.height)?,
    );

    // An RGBA pixmap whose origin is the tile's position in page pixels; MuPDF clips
    // drawing to it. Opaque from the start (white, or the pixels under), so premultiplied
    // and straight alpha agree and the canvas can take the bytes as they are.
    let mut pixmap = Pixmap::new(&Colorspace::device_rgb(), x, y, w, h, true)?;
    match under {
        None => pixmap.clear_with(255)?,
        Some(pixels) => {
            let row_bytes = area.width as usize * 4;
            if pixels.len() != row_bytes * area.height as usize {
                return Err(Error::InvalidArgument(
                    "the pixels under do not fit the image".into(),
                ));
            }
            let stride = usize::try_from(pixmap.stride())
                .map_err(|_| Error::InvalidArgument("negative pixmap stride".into()))?;
            for (row, src) in pixmap
                .samples_mut()
                .chunks_mut(stride)
                .zip(pixels.chunks(row_bytes))
            {
                row[..row_bytes].copy_from_slice(src);
            }
        }
    }
    {
        let device = Device::from_pixmap_with_clip(&pixmap, IRect::new(x, y, x + w, y + h))?;
        let ctm = Matrix::new_scale(scale, scale);
        // The scissor is in device space (after `ctm`): the tile's pixel rectangle.
        let scissor = mupdf::Rect::new(x as f32, y as f32, (x + w) as f32, (y + h) as f32);
        list.run(&device, &ctm, scissor)?;
        // Dropping the device closes it, which flushes pending drawing into the pixmap.
    }
    packed_samples(&pixmap, 4)
}

/// The annotations of a page on their own, for a page shown with visual adjustments that
/// leave them out (section 6.1). `content` is the page without them ([`Part::Content`] at
/// the same scale and region), `highlights` and `others` its annotations
/// ([`Part::Highlights`], [`Part::OtherAnnotations`]). They are drawn over the content as
/// the whole page draws them, and every pixel they change keeps what they made of it; the
/// rest is transparent. Laid over the adjusted content, the annotations look as they do on
/// the page as it is, and so does what lies under a highlight.
///
/// `dark` makes the layer for an inverted page, to be laid over it as light (CSS
/// `screen`, so light text under a highlight stays light): each changed pixel gets its
/// lightness inverted and its hue and saturation kept ([`dark_stroke`], [`dark_tint`]).
/// Black ink turns white and a red stays red, where inverting the page turns a yellow
/// highlight a muddy brown.
pub fn annotation_layer(
    content: &RgbaImage,
    highlights: &DisplayList,
    others: &DisplayList,
    scale: f32,
    region: Option<PixelRect>,
    dark: bool,
) -> Result<RgbaImage> {
    let area = image_area(highlights, scale, region)?;
    if (content.width, content.height) != (area.width, area.height) {
        return Err(Error::InvalidArgument(
            "the page's content does not fit the layer".into(),
        ));
    }
    let tinted = draw(highlights, scale, area, Some(&content.data))?;
    let mut data = draw(others, scale, area, Some(&tinted))?;
    let pixels = data
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(tinted.as_chunks::<4>().0)
        .zip(content.data.as_chunks::<4>().0);
    for ((out, tint), under) in pixels {
        let [r, g, b, _] = *out;
        *out = if out[..3] != tint[..3] {
            // An annotation other than a highlight, drawn last, made this pixel.
            let [r, g, b] = if dark {
                dark_stroke([r, g, b])
            } else {
                [r, g, b]
            };
            [r, g, b, 255]
        } else if tint[..3] != under[..3] {
            let [r, g, b] = if dark {
                dark_tint([r, g, b])
            } else {
                [r, g, b]
            };
            [r, g, b, 255]
        } else {
            [0; 4]
        };
    }
    Ok(RgbaImage {
        width: area.width,
        height: area.height,
        data,
    })
}

/// How light a fully saturated highlight may be on an inverted page, as relative
/// luminance, so that the light text over it stays readable. A grey may be as light as it
/// comes out.
const DARK_TINT_MOST: f32 = 0.4;
/// How light a highlight's colour is at least on an inverted page: a pale one (light pink
/// at 40%) inverts to nearly black. Reached by scaling the colour up, which keeps it as
/// saturated as it is; a highlight that is nearly grey gets less of it.
const DARK_TINT_LEAST: f32 = 0.2;
/// How light a fully saturated annotation colour is at least on an inverted page: pure
/// blue is nearly black there.
const DARK_LEAST: f32 = 0.2;

/// A colour's relative luminance (Rec. 709, on the channel values as they are).
fn luminance([r, g, b]: [f32; 3]) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// The colour with its HSL lightness inverted and its hue and saturation kept, and its
/// chroma (0 for a grey, 1 for a pure colour). The same amount added to every channel
/// keeps the hue and the chroma, and `1 - max - min` takes (max + min) / 2 to its inverse.
fn lightness_inverted(colour: [u8; 3]) -> ([f32; 3], f32) {
    let c = colour.map(|v| f32::from(v) / 255.0);
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let shift = 1.0 - max - min;
    (c.map(|v| v + shift), max - min)
}

/// The colour at `least` luminance or more: mixed with white as far as that takes.
fn lit_at_least(c: [f32; 3], least: f32) -> [f32; 3] {
    let l = luminance(c);
    if l >= least {
        return c;
    }
    // `l < least <= DARK_LEAST`, so `1 - l` is well above 0.
    let t = (least - l) / (1.0 - l);
    c.map(|v| v + (1.0 - v) * t)
}

fn to_bytes(c: [f32; 3]) -> [u8; 3] {
    // In 0..=255 once clamped, so the cast is exact.
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// What a pixel of ink, a line, a note's icon or a text box's text becomes on an inverted
/// page: its lightness inverted (black turns white, a dark blue light blue), and a pure
/// colour too dark to see there lit a little.
pub fn dark_stroke(colour: [u8; 3]) -> [u8; 3] {
    let (c, chroma) = lightness_inverted(colour);
    to_bytes(lit_at_least(c, DARK_LEAST * chroma))
}

/// What a pixel of a highlight becomes on an inverted page: as [`dark_stroke`], a bright
/// colour (a full yellow) dimmed until the light text over it stays readable, and a pale
/// one (a light colour at 40%) brightened until it shows.
pub fn dark_tint(colour: [u8; 3]) -> [u8; 3] {
    let (c, chroma) = lightness_inverted(colour);
    let c = lit_at_least(c, DARK_LEAST * chroma);
    let most = 1.0 - (1.0 - DARK_TINT_MOST) * chroma;
    let least = DARK_TINT_LEAST * (4.0 * chroma).min(1.0);
    let l = luminance(c);
    // Scaled only when `l` is well above 0: past `most >= DARK_TINT_MOST`, or a colour
    // with some luminance under `least`.
    let factor = if l > most {
        most / l
    } else if l < least && l > 1e-3 {
        least / l
    } else {
        1.0
    };
    to_bytes(c.map(|v| v * factor))
}

/// The tallest a preview may be, as a multiple of its width: a very tall page (a receipt,
/// a scroll) is shown at a width that keeps it this tall.
pub const PREVIEW_MAX_ASPECT: f32 = 2.0;

/// The first page of the PDF at `path`, `width` pixels wide (narrower for a page taller
/// than [`PREVIEW_MAX_ASPECT`]), with its annotations: a preview of a file that is not
/// open (the start screen's recent files). The file is read through the share-delete
/// stream, as open documents are, and closed again. A file that needs a password is
/// refused, since a preview would show its content.
pub fn first_page_preview(path: &Path, width: u32) -> Result<RgbaImage> {
    if width == 0 || width > TILE_SIZE * 2 {
        return Err(Error::InvalidArgument(format!(
            "preview width {width} is out of range"
        )));
    }
    let doc = crate::ffi::open_pdf_shared(path)?;
    if doc.needs_password()? {
        return Err(Error::PasswordRequired);
    }
    let list = display_list(&doc, 0)?;
    let bounds = list.bounds();
    let (page_w, page_h) = (bounds.width(), bounds.height());
    if !(page_w > 0.0 && page_h > 0.0) {
        return Err(Error::InvalidArgument("the first page has no area".into()));
    }
    let width = width as f32;
    let scale = (width / page_w)
        .min(width * PREVIEW_MAX_ASPECT / page_h)
        .min(MAX_SCALE);
    render_rgba(&list, scale, None)
}

/// Page `index` of the PDF at `path`, opened with `password`, with its annotations, at
/// `scale`, made smaller if needed so neither side passes `max_side` pixels: a page of a
/// file that is not open (Compress's before-and-after preview, section 6.11). The file is
/// read through the share-delete stream and closed again.
pub fn page_of_file(
    path: &Path,
    password: Option<&str>,
    index: usize,
    scale: f32,
    max_side: u32,
) -> Result<RgbaImage> {
    check_scale(scale)?;
    let mut doc = crate::ffi::open_pdf_shared(path)?;
    if doc.needs_password()? {
        let Some(password) = password else {
            return Err(Error::PasswordRequired);
        };
        if !doc.authenticate(password)? {
            return Err(Error::WrongPassword);
        }
    }
    let list = display_list(&doc, index)?;
    let bounds = list.bounds();
    let longest = bounds.width().max(bounds.height());
    if longest.is_nan() || longest <= 0.0 {
        return Err(Error::InvalidArgument("the page has no area".into()));
    }
    render_rgba(&list, scale.min(max_side as f32 / longest), None)
}

/// Encodes RGBA pixels as PNG at the fastest compression level: RGB for an opaque image (a
/// page: the alpha channel is always 255, so it is dropped), RGBA for one that is not (an
/// [`annotation_layer`]).
pub fn encode_rgba_png(image: &RgbaImage) -> Result<Vec<u8>> {
    let pixels = image.data.as_chunks::<4>().0;
    let opaque = pixels.iter().all(|p| p[3] == 255);
    let mut out = Vec::with_capacity(image.data.len() / 4);
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(if opaque {
        png::ColorType::Rgb
    } else {
        png::ColorType::Rgba
    });
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fastest);
    let mut writer = encoder.write_header().map_err(png_error)?;
    if opaque {
        let rgb: Vec<u8> = pixels
            .iter()
            .flat_map(|[r, g, b, _]| [*r, *g, *b])
            .collect();
        writer.write_image_data(&rgb).map_err(png_error)?;
    } else {
        writer.write_image_data(&image.data).map_err(png_error)?;
    }
    writer.finish().map_err(png_error)?;
    Ok(out)
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

/// Display list of page `index` without the annotations whose object numbers are in
/// `hidden` (0 stands for those that are not indirect objects): what the page shows while
/// the annotation list filters them out. Everything else is drawn as [`display_list`] draws
/// it, in the same order: content, annotations, widgets.
pub fn display_list_hiding(doc: &PdfDocument, index: usize, hidden: &[u32]) -> Result<DisplayList> {
    if hidden.is_empty() {
        return display_list(doc, index);
    }
    display_list_part(doc, index, hidden, Part::Whole)
}

/// What of a page a display list draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// All of it: content, annotations, form fields.
    Whole,
    /// The page without its annotations: content and form fields.
    Content,
    /// Its highlights alone, which tint what is under them.
    Highlights,
    /// Its annotations other than highlights, alone.
    OtherAnnotations,
    /// The page as it prints (section 6.12): content, annotations that have the Print
    /// flag (none if `annotations` is false) and form fields, with content hidden for
    /// printing left out. Annotations hidden in the list are printed all the same.
    Print { annotations: bool },
}

/// Display list of `part` of page `index`, without the annotations in `hidden` (as
/// [`display_list_hiding`]). Every part has the page's bounds. Drawn one over the other,
/// content, highlights and the other annotations make the page as it is, unless a
/// highlight lies over another annotation.
pub fn display_list_part(
    doc: &PdfDocument,
    index: usize,
    hidden: &[u32],
    part: Part,
) -> Result<DisplayList> {
    let page_no = i32::try_from(index).map_err(|_| Error::PageOutOfRange(index))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(index));
    }
    let page = doc.load_pdf_page(page_no)?;
    let mut list = DisplayList::new(page.bounds()?)?;
    if let Part::Print { annotations } = part {
        {
            let device = Device::from_display_list(&mut list)?;
            crate::ffi::run_page_for_print(&page, &device, annotations)?;
        }
        return Ok(list);
    }
    let content = matches!(part, Part::Whole | Part::Content);
    {
        let device = Device::from_display_list(&mut list)?;
        if content {
            page.run_contents(&device, &Matrix::IDENTITY)?;
        }
        if part != Part::Content {
            for annot in page.annotations() {
                let highlight =
                    || -> Result<bool> { Ok(annot.r#type()? == PdfAnnotationType::Highlight) };
                let wanted = match part {
                    Part::Highlights => highlight()?,
                    Part::OtherAnnotations => !highlight()?,
                    Part::Whole | Part::Content | Part::Print { .. } => true,
                };
                if !wanted {
                    continue;
                }
                let object = annot.object();
                let number = if object.is_indirect()? {
                    u32::try_from(object.as_indirect()?).unwrap_or(0)
                } else {
                    0
                };
                if !hidden.contains(&number) {
                    crate::ffi::run_annotation(&annot, &device)?;
                }
            }
        }
        if content {
            page.run_widgets(&device, &Matrix::IDENTITY)?;
        }
    }
    Ok(list)
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
    /// The render scale in thousandths (the frontend rounds its scales to them).
    pub scale_milli: u32,
    /// `None` for a whole page.
    pub tile: Option<PixelRect>,
    /// Which annotations are left out ([`ImageKey::hidden`]); 0 when all are drawn.
    pub hidden: u64,
    pub layer: Layer,
}

/// Which picture of a page an image is. A page shown with visual adjustments that leave
/// its annotations out (section 6.1) is its content, adjusted, with its annotations over
/// it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Layer {
    /// The page as it is.
    #[default]
    Page,
    /// The page without its annotations ([`Part::Content`]).
    Content,
    /// Its annotations on their own ([`annotation_layer`]).
    Annotations,
    /// Its annotations on their own, for an inverted page.
    DarkAnnotations,
    /// The page as it prints, with its annotations ([`Part::Print`]): the Print dialog's
    /// preview (section 6.12).
    Print,
    /// The page as it prints without its annotations.
    PrintContent,
}

impl ImageKey {
    pub fn scale_milli(scale: f32) -> u32 {
        // Callers validate the scale first (at most MAX_SCALE), so this fits.
        (scale * 1000.0).round() as u32
    }

    /// Stands for a set of hidden annotations (sorted, without repeats) in a key: 0 for
    /// none, otherwise a 64-bit FNV-1a hash of their object numbers.
    pub fn hidden(numbers: &[u32]) -> u64 {
        if numbers.is_empty() {
            return 0;
        }
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in numbers.iter().flat_map(|n| n.to_le_bytes()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        // 0 is taken by "none hidden".
        hash.max(1)
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

    /// A sample page with two squares on it, and their object numbers.
    fn two_squares() -> (PdfDocument, u32, u32) {
        use mupdf::Rect;
        use mupdf::pdf::PdfAnnotationType;
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let mut page = doc.load_pdf_page(0).unwrap();
        let mut numbers = Vec::new();
        for x in [100.0, 300.0] {
            let mut annot = page.create_annotation(PdfAnnotationType::Square).unwrap();
            annot
                .set_rect(Rect::new(x, 400.0, x + 100.0, 500.0))
                .unwrap();
            annot.update().unwrap();
            numbers.push(u32::try_from(annot.object().as_indirect().unwrap()).unwrap());
        }
        drop(page);
        (doc, numbers[0], numbers[1])
    }

    #[test]
    fn hidden_annotations_are_not_drawn_and_the_rest_is_unchanged() {
        let (doc, first, second) = two_squares();
        let pixels = |list: &DisplayList| render_rgba(list, 1.0, None).unwrap().data;
        let all = pixels(&display_list(&doc, 0).unwrap());
        let none = pixels(&display_list_with(&doc, 0, false).unwrap());
        assert_ne!(all, none);

        // Both hidden: the page as it is without annotations.
        assert_eq!(
            pixels(&display_list_hiding(&doc, 0, &[first, second]).unwrap()),
            none
        );
        // A number no annotation has hides nothing: drawn one by one, the annotations
        // come out as MuPDF draws them all at once.
        assert_eq!(
            pixels(&display_list_hiding(&doc, 0, &[u32::MAX]).unwrap()),
            all
        );
        // One hidden: its square is gone (as without annotations), the other is there.
        let one = render_rgba(&display_list_hiding(&doc, 0, &[first]).unwrap(), 1.0, None).unwrap();
        let row = |data: &[u8], y: usize, x0: usize, x1: usize| {
            let width = one.width as usize;
            data[(y * width + x0) * 4..(y * width + x1) * 4].to_vec()
        };
        // A row through both squares (400 to 500 down the page, in MuPDF's space).
        assert_eq!(row(&one.data, 450, 90, 210), row(&none, 450, 90, 210));
        assert_eq!(row(&one.data, 450, 290, 410), row(&all, 450, 290, 410));
        assert_ne!(row(&one.data, 450, 290, 410), row(&none, 450, 290, 410));
        assert!(display_list_hiding(&doc, 99, &[first]).is_err());
    }

    #[test]
    fn the_annotation_layer_over_the_content_makes_the_page() {
        let (doc, first, _) = two_squares();
        let whole = render_rgba(&display_list(&doc, 0).unwrap(), 1.0, None).unwrap();
        let none = render_rgba(&display_list_with(&doc, 0, false).unwrap(), 1.0, None).unwrap();
        let content = render_rgba(
            &display_list_part(&doc, 0, &[], Part::Content).unwrap(),
            1.0,
            None,
        )
        .unwrap();
        assert_eq!(content, none, "the content is the page without annotations");
        let layer = |hidden: &[u32], region: Option<PixelRect>, dark: bool| {
            let content = render_rgba(
                &display_list_part(&doc, 0, &[], Part::Content).unwrap(),
                1.0,
                region,
            )
            .unwrap();
            let highlights = display_list_part(&doc, 0, hidden, Part::Highlights).unwrap();
            let others = display_list_part(&doc, 0, hidden, Part::OtherAnnotations).unwrap();
            annotation_layer(&content, &highlights, &others, 1.0, region, dark).unwrap()
        };

        // Opaque where the annotations change the page, transparent elsewhere; laid over
        // the content, the page as it is.
        let plain = layer(&[], None, false);
        let mut drawn = 0;
        for ((l, c), w) in plain
            .data
            .chunks(4)
            .zip(content.data.chunks(4))
            .zip(whole.data.chunks(4))
        {
            if l[3] == 255 {
                drawn += 1;
                assert_eq!(l, w);
            } else {
                assert_eq!(l, [0; 4]);
                assert_eq!(c, w);
            }
        }
        assert!(drawn > 0, "the squares are on the layer");

        // A hidden annotation is not on it.
        let one = layer(&[first], None, false);
        let at = |image: &RgbaImage, x: usize, y: usize| {
            let i = (y * image.width as usize + x) * 4;
            image.data[i..i + 4].to_vec()
        };
        // The squares' edges, 100 to 200 and 300 to 400 across, 400 to 500 down.
        assert_eq!(at(&plain, 100, 450)[3], 255);
        assert_eq!(at(&one, 100, 450), [0; 4]);
        assert_eq!(at(&one, 300, 450), at(&plain, 300, 450));

        // For an inverted page, the same pixels, their lightness inverted.
        let dark = layer(&[], None, true);
        for (d, l) in dark.data.chunks(4).zip(plain.data.chunks(4)) {
            assert_eq!(d[3], l[3]);
            if l[3] == 255 {
                assert_eq!(&d[..3], dark_stroke([l[0], l[1], l[2]]));
            }
        }

        // A tile is that part of the layer.
        let region = PixelRect {
            x: 64,
            y: 384,
            width: 256,
            height: 128,
        };
        let tile = layer(&[], Some(region), false);
        assert_eq!((tile.width, tile.height), (256, 128));
        assert_eq!(at(&tile, 100 - 64, 450 - 384), at(&plain, 100, 450));
        assert_eq!(at(&tile, 250 - 64, 450 - 384), [0; 4]);
    }

    #[test]
    fn annotation_colours_on_an_inverted_page_keep_their_hue() {
        // Greys turn over: black ink is white there, white black.
        assert_eq!(dark_stroke([0, 0, 0]), [255, 255, 255]);
        assert_eq!(dark_stroke([255, 255, 255]), [0, 0, 0]);
        assert_eq!(dark_stroke([64, 64, 64]), [191, 191, 191]);
        // A pure colour has the lightness of its inverse already: Acrobat's red stays.
        assert_eq!(dark_stroke([255, 0, 0]), [255, 0, 0]);
        // A dark blue turns light blue; pure blue, nearly black on a dark page, is lit.
        let [r, g, b] = dark_stroke([0, 0, 128]);
        assert!(b == 255 && r > 100 && r == g, "{r} {g} {b}");
        let [r, g, b] = dark_stroke([0, 0, 255]);
        assert!(b == 255 && r > 0 && r == g && r < 100, "{r} {g} {b}");

        // A highlight stays its colour, dark enough under light text: a full yellow is
        // dimmed, and Acrobat's yellow at 40% (on white paper) comes out a dark gold.
        let lum = |c: [u8; 3]| luminance(c.map(|v| f32::from(v) / 255.0));
        let full = dark_tint([255, 255, 0]);
        assert!(full[0] == full[1] && full[2] == 0, "{full:?}");
        assert!((lum(full) - DARK_TINT_MOST).abs() < 0.01, "{full:?}");
        let acrobat = dark_tint([255, 237, 153]);
        assert!(
            acrobat[0] > acrobat[1] && acrobat[1] > acrobat[2],
            "{acrobat:?}"
        );
        assert!(
            lum(acrobat) > 0.2 && lum(acrobat) <= DARK_TINT_MOST,
            "{acrobat:?}"
        );
        // Inverted, as the page is, the same highlight was a muddy brown (luminance
        // under 0.1). A pale pink at 40%, nearly black inverted, is a clear crimson.
        let pink = dark_tint([255, 204, 216]);
        assert!(pink[0] > pink[2] && pink[2] > pink[1], "{pink:?}");
        assert!((lum(pink) - 0.16).abs() < 0.01, "{pink:?}");
        // A highlight's grey is neither dimmed nor brightened.
        assert_eq!(dark_tint([0, 0, 0]), [255, 255, 255]);
        assert_eq!(dark_tint([255, 255, 255]), [0, 0, 0]);
    }

    #[test]
    fn hidden_sets_get_keys_of_their_own() {
        assert_eq!(ImageKey::hidden(&[]), 0);
        assert_ne!(ImageKey::hidden(&[0]), 0);
        assert_ne!(ImageKey::hidden(&[12]), ImageKey::hidden(&[21]));
        assert_ne!(ImageKey::hidden(&[1, 2]), ImageKey::hidden(&[1, 3]));
        assert_eq!(ImageKey::hidden(&[7, 9]), ImageKey::hidden(&[7, 9]));
    }

    #[test]
    fn png_of_rgba_decodes_to_the_same_pixels() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = display_list(&doc, 0).unwrap();
        let rgba = render_rgba(&list, 0.5, None).unwrap();
        let png = encode_rgba_png(&rgba).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (rgba.width, rgba.height));
        let rgb: Vec<u8> = rgba
            .data
            .chunks(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        assert_eq!(&buf[..info.buffer_size()], &rgb[..]);

        // An image that is not opaque keeps its alpha.
        let mut layer = rgba.clone();
        layer.data[3] = 0;
        let png = encode_rgba_png(&layer).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(&buf[..info.buffer_size()], &layer.data[..]);
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

    fn saved_sample(name: &str, spec: &SampleSpec) -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output/preview");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let doc = sample_document(spec).unwrap();
        crate::save::save_atomic(&doc, crate::save::SaveKind::Full, None, &path).unwrap();
        path
    }

    #[test]
    fn previews_the_first_page_at_the_asked_width() {
        let path = saved_sample("letter.pdf", &SampleSpec::default());
        let image = first_page_preview(&path, 200).unwrap();
        // 612 x 792 pt, 200 px wide.
        assert_eq!((image.width, image.height), (200, 259));
        assert!(
            image.data.chunks(4).any(|p| p[0] < 128),
            "the page's text is drawn"
        );

        // A turned page is previewed as it is shown: landscape.
        let turned = saved_sample(
            "turned.pdf",
            &SampleSpec {
                rotate: 90,
                ..SampleSpec::default()
            },
        );
        let image = first_page_preview(&turned, 200).unwrap();
        assert_eq!((image.width, image.height), (200, 155));
    }

    #[test]
    fn previews_a_tall_page_narrower() {
        let path = saved_sample(
            "receipt.pdf",
            &SampleSpec {
                media_box: crate::geometry::Rect::new(0.0, 0.0, 200.0, 1000.0),
                ..SampleSpec::default()
            },
        );
        let image = first_page_preview(&path, 100).unwrap();
        assert_eq!((image.width, image.height), (40, 200));
    }

    #[test]
    fn refuses_previews_it_cannot_make() {
        let path = saved_sample("ok.pdf", &SampleSpec::default());
        assert!(first_page_preview(&path, 0).is_err());
        assert!(first_page_preview(&path, 5000).is_err());
        let missing = path.with_file_name("missing.pdf");
        assert!(first_page_preview(&missing, 200).is_err());
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
            hidden: 0,
            layer: Layer::Page,
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
