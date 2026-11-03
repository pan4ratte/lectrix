//! The images in a document exported as they are kept in it (section 6.14, ADR 0035): each
//! image drawn on the pages chosen, once, at its own size.
//!
//! - **JPEG** and **JPEG 2000** are copied byte for byte, when the file holds what the
//!   document shows ([`kind_of`]): a JPEG gets the colour profile the document gives it,
//!   and an EXIF orientation that would turn it (PDF ignores it) is set upright.
//! - **The rest** is decoded and written as PNG, in grey or RGB (CMYK, Lab and spot
//!   colours converted to RGB), with its soft mask as alpha and its own colour profile, a
//!   stencil mask in black on white; one bit a pixel where the image has one.
//!
//! [`page_images`] finds the images on pages (by reading their contents, forms, patterns
//! and annotation appearances, as Compress does) with what tells images alike apart;
//! [`plan`] leaves out repeats and small images; [`write_images`] writes the files on
//! worker threads into temporary files, which take their names only once all are written.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use mupdf::pdf::{PdfDocument, PdfObject};

use super::run::temporary;
use crate::error::{Error, Result};
use crate::ffi::compress::{Decoded, ImageRole, PixelSpace, walk_page_images_with_roles};
use crate::ffi::images::{Compressed, ImageFacts, LoadedImage, SpaceKind};

/// What an image's file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageKind {
    /// The image's own JPEG.
    Jpeg,
    /// The image's own JPEG 2000: a JP2 file, or a bare codestream (`.j2k`).
    Jpeg2000 { codestream: bool },
    /// The image decoded and written as PNG.
    Png,
}

impl ImageKind {
    pub fn extension(self) -> &'static str {
        match self {
            ImageKind::Jpeg => "jpg",
            ImageKind::Jpeg2000 { codestream: false } => "jp2",
            ImageKind::Jpeg2000 { codestream: true } => "j2k",
            ImageKind::Png => "png",
        }
    }
}

/// An image drawn on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageImage {
    /// The page (0-based), and the image's object number.
    pub page: usize,
    pub num: i32,
    /// Its own size, pixels.
    pub width: u32,
    pub height: u32,
    pub kind: ImageKind,
    /// Its file's size: exact for a copy; for PNG, estimated from how the document keeps it.
    pub bytes: u64,
    /// Equal for images alike: the image written out with everything it refers to (its
    /// colour space, its masks), its data as the file keeps it.
    pub key: u128,
}

/// The images drawn on `pages` (0-based, in order), each page's in the order they are
/// drawn, each once a page. Masks are part of their image, not images of their own. An
/// image MuPDF can't read is left out.
pub fn page_images(doc: &PdfDocument, pages: &[usize]) -> Result<Vec<PageImage>> {
    // Each image read once: as found on the first page it is on.
    let mut known: HashMap<i32, Option<PageImage>> = HashMap::new();
    let mut found = Vec::new();
    for &page in pages {
        let index = i32::try_from(page).map_err(|_| Error::PageOutOfRange(page))?;
        let mut drawn: Vec<i32> = Vec::new();
        walk_page_images_with_roles(doc, index, &mut |num, _, role| {
            if role == ImageRole::Drawn && !drawn.contains(&num) {
                drawn.push(num);
            }
        })?;
        for num in drawn {
            let image = known
                .entry(num)
                .or_insert_with(|| describe(doc, page, num).ok());
            if let Some(image) = image {
                found.push(PageImage {
                    page,
                    ..image.clone()
                });
            }
        }
    }
    Ok(found)
}

/// Image `num`, drawn on `page`: its size, kind, file size and key.
fn describe(doc: &PdfDocument, page: usize, num: i32) -> Result<PageImage> {
    let image = LoadedImage::load(doc, num)?;
    let facts = image.facts();
    if facts.width == 0 || facts.height == 0 {
        return Err(Error::InvalidArgument("an image without pixels".into()));
    }
    let kind = kind_of(&facts, image.compressed());
    let (key, raw_len) = content_key(doc, num)?;
    let bytes = match kind {
        ImageKind::Png => raw_len,
        _ => facts.compressed_len as u64,
    };
    Ok(PageImage {
        page,
        num,
        width: facts.width,
        height: facts.height,
        kind,
        bytes,
        key,
    })
}

/// What an image's file is: its own JPEG or JPEG 2000 where those bytes, opened alone, show
/// what the document shows; otherwise PNG.
///
/// A JPEG is copied in grey, RGB or CMYK, without a `/Decode` of its own, a mask or a
/// colour key, and with no `/ColorTransform` other than its own markers imply. A CMYK one
/// only with an Adobe marker and `/Decode [1 0 1 0 1 0 1 0]`: apps opening a JPEG file
/// invert Adobe CMYK, which PDF doesn't, so only what the document inverts back comes out
/// the same (Photoshop's CMYK JPEGs, placed as they are). JPEG 2000 in grey or RGB without
/// `/Decode` or a mask.
pub fn kind_of(facts: &ImageFacts, data: Option<&[u8]>) -> ImageKind {
    if facts.has_mask || facts.use_colorkey || facts.image_mask {
        return ImageKind::Png;
    }
    match (facts.compressed, data) {
        (Some(Compressed::Jpeg), Some(data)) => {
            let Some(info) = jpeg_info(data) else {
                return ImageKind::Png;
            };
            let implied = info
                .adobe_transform
                .unwrap_or(if info.components == 3 { 1 } else { 0 });
            if let Some(asked) = facts.color_transform
                && (asked != 0) != (implied != 0)
            {
                return ImageKind::Png;
            }
            let fits = match (facts.space, info.components) {
                (SpaceKind::Gray, 1) | (SpaceKind::Rgb, 3) => !facts.use_decode,
                (SpaceKind::Cmyk, 4) => {
                    info.adobe_transform.is_some() && facts.use_decode && facts.decode_inverted
                }
                _ => false,
            };
            if fits {
                ImageKind::Jpeg
            } else {
                ImageKind::Png
            }
        }
        (Some(Compressed::Jpeg2000), Some(data)) => {
            let grey_or_rgb = matches!(facts.space, SpaceKind::Gray | SpaceKind::Rgb);
            if facts.use_decode || !grey_or_rgb {
                return ImageKind::Png;
            }
            if data.starts_with(JP2_SIGNATURE) {
                ImageKind::Jpeg2000 { codestream: false }
            } else if data.starts_with(&[0xFF, 0x4F, 0xFF, 0x51]) {
                ImageKind::Jpeg2000 { codestream: true }
            } else {
                ImageKind::Png
            }
        }
        _ => ImageKind::Png,
    }
}

/// A JP2 file's first box: its signature.
const JP2_SIGNATURE: &[u8] = &[
    0, 0, 0, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
];

// ----- images alike -----

/// Keys of an image's dictionary that don't change what it shows.
const IGNORED_KEYS: &[&[u8]] = &[
    b"Length",
    b"DL",
    b"Metadata",
    b"StructParent",
    b"StructParents",
    b"Name",
    b"ID",
    b"OC",
    b"OPI",
    b"Alternates",
    b"PieceInfo",
    b"LastModified",
    b"Interpolate",
    b"Intent",
];

/// Deeper than this, an object is told apart by its number alone.
const MAX_DEPTH: usize = 24;

/// Two hashers with different salts, for a 128-bit key.
struct Keyed(DefaultHasher, DefaultHasher);

impl Keyed {
    fn new() -> Self {
        let mut a = DefaultHasher::new();
        let mut b = DefaultHasher::new();
        0u8.hash(&mut a);
        1u8.hash(&mut b);
        Keyed(a, b)
    }

    fn put(&mut self, bytes: &[u8]) {
        bytes.hash(&mut self.0);
        bytes.hash(&mut self.1);
    }

    fn finish(&self) -> u128 {
        (u128::from(self.0.finish()) << 64) | u128::from(self.1.finish())
    }
}

/// Image `num`'s key, and how long its data is as the file keeps it.
fn content_key(doc: &PdfDocument, num: i32) -> Result<(u128, u64)> {
    let obj = doc.new_indirect(num, 0)?;
    let raw = obj.read_raw_stream()?;
    let mut key = Keyed::new();
    let mut visiting = vec![num];
    if let Some(dict) = obj.resolve()? {
        put_dict(&dict, &mut key, 0, &mut visiting, true)?;
    }
    key.put(b"data");
    key.put(&raw);
    Ok((key.finish(), raw.len() as u64))
}

/// Puts `obj` in `key`, following references (a stream's data as the file keeps it).
fn put_object(
    obj: &PdfObject,
    key: &mut Keyed,
    depth: usize,
    visiting: &mut Vec<i32>,
) -> Result<()> {
    if obj.is_indirect()? {
        let num = obj.as_indirect()?;
        if depth >= MAX_DEPTH || visiting.contains(&num) {
            key.put(format!("R{num};").as_bytes());
            return Ok(());
        }
        visiting.push(num);
        if let Some(resolved) = obj.resolve()? {
            if obj.is_stream()? {
                key.put(b"S");
                put_dict(&resolved, key, depth + 1, visiting, false)?;
                key.put(&obj.read_raw_stream()?);
            } else {
                put_object(&resolved, key, depth + 1, visiting)?;
            }
        } else {
            key.put(b"n");
        }
        visiting.pop();
    } else if obj.is_null()? {
        key.put(b"n");
    } else if obj.is_bool()? {
        key.put(if obj.as_bool()? { b"t" } else { b"f" });
    } else if obj.is_int()? {
        key.put(format!("i{};", obj.as_int()?).as_bytes());
    } else if obj.is_real()? {
        key.put(format!("r{};", obj.as_float()?).as_bytes());
    } else if obj.is_name()? {
        let name = obj.as_name()?;
        key.put(format!("/{}:", name.len()).as_bytes());
        key.put(&name);
    } else if obj.is_string()? {
        let bytes = obj.as_bytes()?;
        key.put(format!("({}:", bytes.len()).as_bytes());
        key.put(&bytes);
    } else if obj.is_array()? {
        key.put(b"[");
        for i in 0..obj.len()? {
            match obj.get_array(i as i32)? {
                Some(item) => put_object(&item, key, depth + 1, visiting)?,
                None => key.put(b"n"),
            }
        }
        key.put(b"]");
    } else if obj.is_dict()? {
        put_dict(obj, key, depth, visiting, false)?;
    } else {
        key.put(b"?");
    }
    Ok(())
}

/// Puts a dictionary in `key`, its keys in order; an image's own (`top`) without the keys
/// that don't change what it shows, any stream's without its length.
fn put_dict(
    dict: &PdfObject,
    key: &mut Keyed,
    depth: usize,
    visiting: &mut Vec<i32>,
    top: bool,
) -> Result<()> {
    let mut entries: Vec<(Vec<u8>, PdfObject)> = Vec::new();
    for i in 0..dict.dict_len()? {
        let (Some(k), Some(v)) = (dict.get_dict_key(i as i32)?, dict.get_dict_val(i as i32)?)
        else {
            continue;
        };
        let k = k.as_name()?;
        if k == b"Length" || (top && IGNORED_KEYS.contains(&k.as_slice())) {
            continue;
        }
        entries.push((k, v));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    key.put(b"<");
    for (k, v) in entries {
        key.put(format!("/{}:", k.len()).as_bytes());
        key.put(&k);
        put_object(&v, key, depth + 1, visiting)?;
    }
    key.put(b">");
    Ok(())
}

// ----- what is exported -----

/// An image to export: the first time it is drawn, on `page`, the `index`th (from 1) of
/// that page's images exported.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedImage {
    pub page: usize,
    pub index: usize,
    pub num: i32,
    pub width: u32,
    pub height: u32,
    pub kind: ImageKind,
    pub bytes: u64,
    pub key: u128,
}

/// The images to export of those found, and those left out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    pub images: Vec<PlannedImage>,
    /// Times an image is drawn again, on the same page or another, or an image alike is
    /// drawn: each is exported once.
    pub repeats: usize,
    /// Images (each counted once) smaller than the size asked for, left out.
    pub small: usize,
}

/// The images to export of `found` (in page order): each image once, the first time it is
/// drawn, and none narrower or lower than `min_side` pixels if given.
pub fn plan(found: &[PageImage], min_side: Option<u32>) -> Plan {
    let mut plan = Plan::default();
    let mut seen: HashSet<u128> = HashSet::new();
    let mut small: HashSet<u128> = HashSet::new();
    let mut on_page: HashMap<usize, usize> = HashMap::new();
    for image in found {
        if min_side.is_some_and(|m| image.width.min(image.height) < m) {
            small.insert(image.key);
            continue;
        }
        if !seen.insert(image.key) {
            plan.repeats += 1;
            continue;
        }
        let index = on_page.entry(image.page).or_insert(0);
        *index += 1;
        plan.images.push(PlannedImage {
            page: image.page,
            index: *index,
            num: image.num,
            width: image.width,
            height: image.height,
            kind: image.kind,
            bytes: image.bytes,
            key: image.key,
        });
    }
    plan.small = small.len();
    plan
}

// ----- the files -----

/// An image's file before it is written: its bytes, or the image to decode for PNG.
enum ImageData {
    Bytes(Vec<u8>),
    Decode(LoadedImage),
}

/// Pixels for a PNG file.
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub layout: Layout,
    /// Rows of `width` pixels; for [`Layout::Bits`], `width.div_ceil(8)` bytes a row, white
    /// as 1.
    pub data: Vec<u8>,
    /// The image's own colour profile, for grey and RGB that keep their colours.
    pub icc: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Bits,
    Gray,
    GrayAlpha,
    Rgb,
    Rgba,
}

impl Layout {
    fn channels(self) -> usize {
        match self {
            Layout::Bits | Layout::Gray => 1,
            Layout::GrayAlpha => 2,
            Layout::Rgb => 3,
            Layout::Rgba => 4,
        }
    }
}

/// What image `num` of `doc` is written as, as `kind` says: a copy's bytes, or the image
/// loaded, to decode on a worker thread; and the memory writing it takes, bytes.
fn image_data(doc: &PdfDocument, num: i32, kind: ImageKind) -> Result<(ImageData, u64)> {
    let image = LoadedImage::load(doc, num)?;
    if kind == ImageKind::Png {
        let f = image.facts();
        // Colour (RGB once converted) and alpha, decoded, then copied for PNG.
        let colours = if f.image_mask || f.space == SpaceKind::Gray {
            1
        } else {
            3
        };
        let channels = colours + u64::from(f.has_mask || f.use_colorkey);
        let need = u64::from(f.width) * u64::from(f.height) * (2 * channels + 2);
        return Ok((ImageData::Decode(image), need));
    }
    let data = image
        .compressed()
        .ok_or_else(|| Error::InvalidArgument("the image has no data of its own".into()))?;
    let bytes = if kind == ImageKind::Jpeg {
        jpeg_file(data, image.icc_profile())
    } else {
        data.to_vec()
    };
    let need = bytes.len() as u64;
    Ok((ImageData::Bytes(bytes), need))
}

/// An image decoded and written as PNG.
fn png_of(image: &LoadedImage) -> Result<Vec<u8>> {
    let facts = image.facts();
    let decoded = image.pixels(0)?;
    let pixels = pixels_of(&decoded, &facts, image.icc_profile(), false);
    drop(decoded);
    encode_png(&pixels, png::Compression::High)
}

/// Decoded pixels as a PNG takes them: alpha undone from MuPDF's premultiplied colour, a
/// stencil mask in black on white, one bit a pixel for an image of one bit (unless
/// `smooth`, for a thumbnail scaled down, which keeps its greys).
fn pixels_of(decoded: &Decoded, facts: &ImageFacts, icc: Option<&[u8]>, smooth: bool) -> Pixels {
    let (w, h) = (decoded.width, decoded.height);
    let colours = decoded.n - usize::from(decoded.alpha);
    let mut data = Vec::new();
    let layout;
    if decoded.space == PixelSpace::None || colours == 0 {
        // A stencil mask: black where it paints.
        if smooth {
            layout = Layout::Gray;
            data.reserve(w * h);
            for y in 0..h {
                data.extend(decoded.row(y).iter().map(|&a| 255 - a));
            }
        } else {
            layout = Layout::Bits;
            data = pack_bits(decoded, |a| a < 128);
        }
    } else if colours == 1 && !decoded.alpha && facts.bits_per_component == 1 && !smooth {
        layout = Layout::Bits;
        data = pack_bits(decoded, |v| v >= 128);
    } else {
        layout = match (colours, decoded.alpha) {
            (1, false) => Layout::Gray,
            (1, true) => Layout::GrayAlpha,
            (_, false) => Layout::Rgb,
            (_, true) => Layout::Rgba,
        };
        data.reserve(w * h * layout.channels());
        for y in 0..h {
            let row = decoded.row(y);
            if decoded.alpha {
                for px in row.chunks_exact(decoded.n) {
                    let a = px[decoded.n - 1];
                    for &c in &px[..decoded.n - 1] {
                        data.push(unpremultiply(c, a));
                    }
                    data.push(a);
                }
            } else {
                data.extend_from_slice(row);
            }
        }
    }
    // The profile is kept where the colours are the image's own.
    let wanted: &[u8] = if colours == 1 { b"GRAY" } else { b"RGB " };
    let keeps = matches!(
        facts.space,
        SpaceKind::Gray | SpaceKind::Rgb | SpaceKind::Indexed
    );
    let icc = icc
        .filter(|p| keeps && layout != Layout::Bits && p.get(16..20) == Some(wanted))
        .map(<[u8]>::to_vec);
    Pixels {
        width: w as u32,
        height: h as u32,
        layout,
        data,
        icc,
    }
}

/// One-channel rows packed a bit a pixel, first pixel in the high bit, 1 where `white`.
fn pack_bits(decoded: &Decoded, white: impl Fn(u8) -> bool) -> Vec<u8> {
    let stride = decoded.width.div_ceil(8);
    let mut out = vec![0u8; stride * decoded.height];
    for y in 0..decoded.height {
        let row = decoded.row(y);
        for x in 0..decoded.width {
            if white(row[x * decoded.n]) {
                out[y * stride + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    out
}

fn unpremultiply(c: u8, a: u8) -> u8 {
    if a == 0 {
        0
    } else {
        ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
    }
}

/// `pixels` written as PNG: with their colour profile, or sRGB.
pub fn encode_png(pixels: &Pixels, compression: png::Compression) -> Result<Vec<u8>> {
    let e = |e: png::EncodingError| Error::Encode {
        format: "PNG",
        detail: e.to_string(),
    };
    let mut info = png::Info::with_size(pixels.width, pixels.height);
    info.color_type = match pixels.layout {
        Layout::Bits | Layout::Gray => png::ColorType::Grayscale,
        Layout::GrayAlpha => png::ColorType::GrayscaleAlpha,
        Layout::Rgb => png::ColorType::Rgb,
        Layout::Rgba => png::ColorType::Rgba,
    };
    info.bit_depth = if pixels.layout == Layout::Bits {
        png::BitDepth::One
    } else {
        png::BitDepth::Eight
    };
    info.icc_profile = pixels.icc.as_deref().map(std::borrow::Cow::Borrowed);
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::with_info(&mut out, info).map_err(e)?;
        if pixels.icc.is_none() && pixels.layout != Layout::Bits {
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        }
        encoder.set_compression(compression);
        let mut writer = encoder.write_header().map_err(e)?;
        writer.write_image_data(&pixels.data).map_err(e)?;
        writer.finish().map_err(e)?;
    }
    Ok(out)
}

/// Image `num` of `doc` as a PNG at most `side` pixels a side, for the dialog's list.
pub fn thumbnail(doc: &PdfDocument, num: i32, side: u32) -> Result<Vec<u8>> {
    let image = LoadedImage::load(doc, num)?;
    let facts = image.facts();
    let decoded = image.pixels(side.max(1))?;
    let mut pixels = pixels_of(&decoded, &facts, None, true);
    pixels.icc = None;
    encode_png(&pixels, png::Compression::Fast)
}

// ----- JPEG markers -----

/// What a JPEG's markers say, up to its frame header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpegInfo {
    pub components: u8,
    /// The Adobe marker's colour transform (0 none, 1 YCbCr, 2 YCCK), if it has one.
    pub adobe_transform: Option<u8>,
    /// It has a colour profile of its own (`ICC_PROFILE` in APP2).
    pub has_icc: bool,
    /// Where its EXIF orientation's value is, and in which byte order (true: big-endian),
    /// if it has one other than upright.
    pub orientation: Option<(usize, bool)>,
    /// Where a profile goes: after the JFIF and EXIF segments it starts with.
    pub insert_at: usize,
}

/// Reads a JPEG's markers up to its frame header; `None` if it is not one.
pub fn jpeg_info(data: &[u8]) -> Option<JpegInfo> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut info = JpegInfo {
        components: 0,
        adobe_transform: None,
        has_icc: false,
        orientation: None,
        insert_at: 2,
    };
    let mut leading = true;
    let mut i = 2;
    loop {
        // Fill bytes before a marker.
        while i + 1 < data.len() && data[i] == 0xFF && data[i + 1] == 0xFF {
            i += 1;
        }
        if i + 4 > data.len() || data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xD8 || marker == 0xD9 || marker == 0xDA {
            // Scan data before a frame header.
            return None;
        }
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        if len < 2 || i + 2 + len > data.len() {
            return None;
        }
        let body = &data[i + 4..i + 2 + len];
        let end = i + 2 + len;
        match marker {
            0xE0 | 0xE1 if leading => {
                if marker == 0xE1 {
                    info.orientation = exif_orientation(body, i + 4);
                }
                info.insert_at = end;
            }
            0xE1 => info.orientation = info.orientation.or(exif_orientation(body, i + 4)),
            0xE2 if body.starts_with(b"ICC_PROFILE\0") => info.has_icc = true,
            0xEE if body.len() >= 12 && body.starts_with(b"Adobe") => {
                info.adobe_transform = Some(body[11]);
            }
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                info.components = *body.get(5)?;
                return Some(info);
            }
            _ => {}
        }
        if !matches!(marker, 0xE0 | 0xE1) {
            leading = false;
        }
        i = end;
    }
}

/// Where an EXIF segment's orientation value is (at `at` in the file), if it is not 1.
fn exif_orientation(body: &[u8], at: usize) -> Option<(usize, bool)> {
    let tiff = body.strip_prefix(b"Exif\0\0")?;
    let base = at + 6;
    let big = match tiff.get(0..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |o: usize| -> Option<u16> {
        let b = [*tiff.get(o)?, *tiff.get(o + 1)?];
        Some(if big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |o: usize| -> Option<u32> {
        let b = [
            *tiff.get(o)?,
            *tiff.get(o + 1)?,
            *tiff.get(o + 2)?,
            *tiff.get(o + 3)?,
        ];
        Some(if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let count = usize::from(u16_at(ifd)?);
    for e in 0..count {
        let entry = ifd + 2 + e * 12;
        // Orientation, a SHORT.
        if u16_at(entry)? == 0x0112 && u16_at(entry + 2)? == 3 {
            let value = u16_at(entry + 8)?;
            return (value != 1).then_some((base + entry + 8, big));
        }
    }
    None
}

/// The most bytes of a profile one APP2 segment holds.
const ICC_CHUNK: usize = 65_535 - 2 - 14;

/// A JPEG as written to its file: the image's own bytes, with the document's colour
/// profile for it if it has none of its own (and the profile is of its colours), and an
/// EXIF orientation that would turn it set upright, as the document shows it.
pub fn jpeg_file(data: &[u8], icc: Option<&[u8]>) -> Vec<u8> {
    let Some(info) = jpeg_info(data) else {
        return data.to_vec();
    };
    let mut out = data.to_vec();
    if let Some((at, big)) = info.orientation {
        let one = if big { [0, 1] } else { [1, 0] };
        out[at..at + 2].copy_from_slice(&one);
    }
    let wanted: &[u8] = match info.components {
        1 => b"GRAY",
        3 => b"RGB ",
        _ => b"CMYK",
    };
    let Some(profile) = icc.filter(|p| !info.has_icc && p.get(16..20) == Some(wanted)) else {
        return out;
    };
    let chunks: Vec<&[u8]> = profile.chunks(ICC_CHUNK).collect();
    if chunks.len() > 255 {
        return out;
    }
    let mut segments = Vec::with_capacity(profile.len() + chunks.len() * 18);
    for (n, chunk) in chunks.iter().enumerate() {
        let len = (2 + 14 + chunk.len()) as u16;
        segments.extend_from_slice(&[0xFF, 0xE2]);
        segments.extend_from_slice(&len.to_be_bytes());
        segments.extend_from_slice(b"ICC_PROFILE\0");
        segments.push((n + 1) as u8);
        segments.push(chunks.len() as u8);
        segments.extend_from_slice(chunk);
    }
    out.splice(info.insert_at..info.insert_at, segments);
    out
}

// ----- writing -----

/// Memory the images being written may take at once, in bytes.
const MEMORY_BUDGET: u64 = 1 << 30;

/// How often the document's thread looks whether it was asked to stop.
const POLL: Duration = Duration::from_millis(50);

/// An image to write, and the file it goes to.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageFile {
    pub num: i32,
    pub kind: ImageKind,
    pub path: PathBuf,
}

/// Gives back an image's memory when dropped.
struct Release<'a>(&'a AtomicU64, u64);

impl Drop for Release<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(self.1, Ordering::AcqRel);
    }
}

/// What writing the images did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Written {
    pub files: usize,
    /// Images MuPDF couldn't read, left out.
    pub unreadable: usize,
}

/// Writes `files` from `doc`, telling `progress(done, total)` as each is written. The
/// document's thread loads each image (and copies JPEG and JPEG 2000); worker threads, one
/// a core but one, decode the rest and write the files, taking at most the memory budget
/// between them (an image larger than it alone). Each file goes to a temporary file beside its name and takes the name
/// once every one is written; once `stop` is set it stops as soon as it can and fails with
/// [`Error::Cancelled`], leaving nothing. An image that can't be read is left out and
/// counted; a file that can't be written fails the whole.
pub fn write_images(
    doc: &PdfDocument,
    files: &[ImageFile],
    stop: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Written> {
    if files.is_empty() {
        return Err(Error::InvalidArgument("no images to export".into()));
    }
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    let workers = cores.saturating_sub(1).min(files.len()).max(1);
    // Memory the images given to the workers take, until each is written.
    let in_use = AtomicU64::new(0);
    let temps: Vec<PathBuf> = files.iter().map(|f| temporary(&f.path)).collect();
    let written: Mutex<Vec<bool>> = Mutex::new(vec![false; files.len()]);
    let total = files.len();
    let done = AtomicUsize::new(0);
    let halt = AtomicBool::new(false);
    let unreadable = AtomicUsize::new(0);
    let (tx, rx) = mpsc::sync_channel::<(usize, ImageData, u64)>(workers);
    let (err_tx, err_rx) = mpsc::channel::<Error>();
    let rx = Mutex::new(rx);
    let failure = std::thread::scope(|scope| -> Option<Error> {
        for _ in 0..workers {
            let (rx, err_tx, temps, written, done, halt, unreadable, in_use) = (
                &rx,
                err_tx.clone(),
                &temps,
                &written,
                &done,
                &halt,
                &unreadable,
                &in_use,
            );
            scope.spawn(move || {
                loop {
                    let next = match rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => break,
                    };
                    let Ok((i, data, need)) = next else { break };
                    // Its memory is free again once it is written, or given up.
                    let _release = Release(in_use, need);
                    if halt.load(Ordering::Relaxed) {
                        continue;
                    }
                    let bytes = match data {
                        ImageData::Bytes(b) => b,
                        ImageData::Decode(image) => match png_of(&image) {
                            Ok(b) => b,
                            Err(e @ Error::Io(_)) => {
                                halt.store(true, Ordering::Relaxed);
                                let _ = err_tx.send(e);
                                continue;
                            }
                            Err(_) => {
                                unreadable.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                        },
                    };
                    let result = (|| -> Result<()> {
                        let mut file = File::create(&temps[i])?;
                        file.write_all(&bytes)?;
                        file.flush()?;
                        Ok(())
                    })();
                    match result {
                        Ok(()) => {
                            if let Ok(mut w) = written.lock() {
                                w[i] = true;
                            }
                            done.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            halt.store(true, Ordering::Relaxed);
                            let _ = err_tx.send(e);
                        }
                    }
                }
            });
        }
        drop(err_tx);
        let mut reported = 0;
        for (i, file) in files.iter().enumerate() {
            if stop.load(Ordering::Relaxed) || halt.load(Ordering::Relaxed) {
                halt.store(true, Ordering::Relaxed);
                break;
            }
            match image_data(doc, file.num, file.kind) {
                Ok((data, need)) => {
                    let mut item = Some((i, data, need));
                    let mut reserved = false;
                    // Waits for memory and a worker, looking meanwhile whether to stop.
                    while let Some(it) = item.take() {
                        if stop.load(Ordering::Relaxed) || halt.load(Ordering::Relaxed) {
                            halt.store(true, Ordering::Relaxed);
                            if reserved {
                                in_use.fetch_sub(need, Ordering::AcqRel);
                            }
                            break;
                        }
                        if !reserved {
                            let used = in_use.load(Ordering::Acquire);
                            if used > 0 && used + need > MEMORY_BUDGET {
                                std::thread::sleep(Duration::from_millis(2));
                                item = Some(it);
                                continue;
                            }
                            in_use.fetch_add(need, Ordering::AcqRel);
                            reserved = true;
                        }
                        match tx.try_send(it) {
                            Ok(()) => {}
                            Err(mpsc::TrySendError::Full(it)) => {
                                std::thread::sleep(Duration::from_millis(2));
                                item = Some(it);
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => {
                                halt.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                }
                Err(e) => {
                    if matches!(e, Error::Io(_)) {
                        halt.store(true, Ordering::Relaxed);
                        drop(tx);
                        return Some(e);
                    }
                    unreadable.fetch_add(1, Ordering::Relaxed);
                }
            }
            let now = done.load(Ordering::Relaxed) + unreadable.load(Ordering::Relaxed);
            if now != reported {
                reported = now;
                progress(now.min(total), total);
            }
        }
        drop(tx);
        // The workers finish what they were given.
        loop {
            match err_rx.recv_timeout(POLL) {
                Ok(e) => {
                    halt.store(true, Ordering::Relaxed);
                    return Some(e);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if stop.load(Ordering::Relaxed) {
                        halt.store(true, Ordering::Relaxed);
                    }
                    let now = done.load(Ordering::Relaxed) + unreadable.load(Ordering::Relaxed);
                    if now != reported {
                        reported = now;
                        progress(now.min(total), total);
                    }
                }
            }
        }
        None
    });
    let written = written.into_inner().unwrap_or_default();
    let outcome = match failure {
        Some(e) => Err(e),
        None if stop.load(Ordering::Relaxed) || halt.load(Ordering::Relaxed) => {
            Err(Error::Cancelled)
        }
        None => (|| -> Result<Written> {
            let mut count = 0;
            for (i, file) in files.iter().enumerate() {
                if written.get(i).copied().unwrap_or(false) {
                    std::fs::rename(&temps[i], &file.path)?;
                    count += 1;
                }
            }
            progress(total, total);
            Ok(Written {
                files: count,
                unreadable: unreadable.load(Ordering::Relaxed),
            })
        })(),
    };
    if outcome.is_err() {
        for temp in &temps {
            let _ = std::fs::remove_file(temp);
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::ffi::compress::{encode_jpeg, encode_jpx};
    use crate::testgen::{add_drawing_page, add_image, gray_dict, image_sample, photo, rgb_dict};

    fn out_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/embedded-images")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The object number of the image page `page` calls `name`.
    fn xobject(doc: &PdfDocument, page: i32, name: &str) -> i32 {
        doc.find_page(page)
            .unwrap()
            .get_dict("Resources")
            .unwrap()
            .unwrap()
            .get_dict("XObject")
            .unwrap()
            .unwrap()
            .get_dict(name)
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap()
    }

    struct Png {
        color: png::ColorType,
        depth: png::BitDepth,
        width: u32,
        height: u32,
        data: Vec<u8>,
        icc: bool,
    }

    fn read_png(path: &Path) -> Png {
        let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let frame = reader.next_frame(&mut buf).unwrap();
        buf.truncate(frame.buffer_size());
        Png {
            color: frame.color_type,
            depth: frame.bit_depth,
            width: frame.width,
            height: frame.height,
            data: buf,
            icc: reader.info().icc_profile.is_some(),
        }
    }

    fn files_for(plan: &Plan, dir: &Path) -> Vec<ImageFile> {
        plan.images
            .iter()
            .map(|i| ImageFile {
                num: i.num,
                kind: i.kind,
                path: dir.join(format!(
                    "p{}-{}.{}",
                    i.page + 1,
                    i.index,
                    i.kind.extension()
                )),
            })
            .collect()
    }

    fn write_all(doc: &PdfDocument, plan: &Plan, dir: &Path) -> (Vec<ImageFile>, Written) {
        let files = files_for(plan, dir);
        let written = write_images(doc, &files, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        (files, written)
    }

    #[test]
    fn each_image_drawn_is_found_once_with_its_kind_and_masks_are_not_images() {
        let doc = image_sample().unwrap();
        let found = page_images(&doc, &[0, 1, 2]).unwrap();
        let named = |page: i32, name: &str| xobject(&doc, page, name);
        let seen: Vec<(usize, i32, ImageKind)> =
            found.iter().map(|i| (i.page, i.num, i.kind)).collect();
        assert_eq!(seen.len(), 12, "{seen:?}");
        let formed = found[2].num;
        let tiled = found[10].num;
        let stamp = found[11].num;
        assert_eq!(
            seen,
            vec![
                (0, named(0, "Photo"), ImageKind::Png),
                (0, named(0, "Screen"), ImageKind::Png),
                // Inside a form.
                (0, formed, ImageKind::Png),
                (1, named(1, "GrayRgb"), ImageKind::Png),
                (1, named(1, "Bitonal"), ImageKind::Png),
                (1, named(1, "Mask"), ImageKind::Png),
                // Its soft mask is its alpha, not an image of its own.
                (1, named(1, "Soft"), ImageKind::Png),
                (2, named(2, "Jpeg"), ImageKind::Jpeg),
                // Adobe CMYK that PDF doesn't invert: a JPEG file would show it inverted.
                (2, named(2, "Cmyk"), ImageKind::Png),
                (2, named(2, "Palette"), ImageKind::Png),
                // In a pattern, and in a stamp's appearance.
                (2, tiled, ImageKind::Png),
                (2, stamp, ImageKind::Png),
            ]
        );
        assert_eq!((found[0].width, found[0].height), (1200, 900));
        assert!(
            found[7].bytes > 10_000,
            "a JPEG's size is its own: {}",
            found[7].bytes
        );
        // Different images have different keys.
        let keys: HashSet<u128> = found.iter().map(|i| i.key).collect();
        assert_eq!(keys.len(), found.len());
    }

    #[test]
    fn files_hold_the_images_as_the_document_shows_them() {
        let doc = image_sample().unwrap();
        let found = page_images(&doc, &[0, 1, 2]).unwrap();
        let plan = plan(&found, None);
        assert_eq!(plan.images.len(), 12);
        let dir = out_dir("files");
        let (files, written) = write_all(&doc, &plan, &dir);
        assert_eq!(
            written,
            Written {
                files: 12,
                unreadable: 0
            }
        );
        for f in &files {
            assert!(f.path.exists(), "{}", f.path.display());
            assert!(!temporary(&f.path).exists(), "no temporary file is left");
        }
        let at = |page: usize, index: usize| -> &Path {
            let i = plan
                .images
                .iter()
                .position(|p| p.page == page && p.index == index)
                .unwrap();
            &files[i].path
        };
        // The photograph: RGB, sRGB, every pixel.
        let p = read_png(at(0, 1));
        assert_eq!(
            (p.color, p.depth, p.width, p.height),
            (png::ColorType::Rgb, png::BitDepth::Eight, 1200, 900)
        );
        assert!(!p.icc);
        assert!(p.data == photo(1200, 900, 1), "decoded as it was written");
        // The stencil mask: one bit a pixel, black where it paints (the ellipse's middle).
        let p = read_png(at(1, 3));
        assert_eq!(
            (p.color, p.depth, p.width, p.height),
            (png::ColorType::Grayscale, png::BitDepth::One, 800, 200)
        );
        let white = |x: usize, y: usize| p.data[y * 100 + x / 8] & (0x80 >> (x % 8)) != 0;
        assert!(!white(400, 100), "black in the ellipse");
        assert!(white(5, 5), "white outside it");
        // The soft-masked photograph: RGBA, its alpha the mask's gradient, its colour kept.
        let p = read_png(at(1, 4));
        assert_eq!(
            (p.color, p.width, p.height),
            (png::ColorType::Rgba, 400, 400)
        );
        let original = photo(400, 400, 4);
        let px = |x: usize, y: usize| &p.data[(y * 400 + x) * 4..(y * 400 + x) * 4 + 4];
        assert_eq!(px(0, 10)[3], 0, "transparent where the mask is 0");
        assert!(px(399, 10)[3] > 250, "opaque where it is 255");
        let o = &original[(200 * 400 + 390) * 3..(200 * 400 + 390) * 3 + 3];
        for c in 0..3 {
            assert!(
                px(390, 200)[c].abs_diff(o[c]) <= 2,
                "colour kept under alpha: {:?} {o:?}",
                px(390, 200)
            );
        }
        // The JPEG: its own bytes.
        let jpeg = xobject(&doc, 2, "Jpeg");
        let original = doc
            .new_indirect(jpeg, 0)
            .unwrap()
            .read_raw_stream()
            .unwrap();
        assert!(std::fs::read(at(2, 1)).unwrap() == original);
        assert_eq!(at(2, 1).extension().unwrap(), "jpg");
        // The CMYK JPEG, and the palette, in RGB.
        let p = read_png(at(2, 2));
        assert_eq!(
            (p.color, p.width, p.height),
            (png::ColorType::Rgb, 600, 400)
        );
        assert_eq!(read_png(at(2, 3)).color, png::ColorType::Rgb);
    }

    /// Two pages: a photograph drawn twice on the first and again, as an object of its own
    /// but alike, on the second; a JPEG 2000 image; a one-bit image; a 20-pixel icon on both.
    fn repeats_document() -> PdfDocument {
        let mut doc = PdfDocument::new();
        let samples = photo(300, 200, 9);
        let a = add_image(&mut doc, &rgb_dict(300, 200), &samples, true).unwrap();
        let b = add_image(&mut doc, &rgb_dict(300, 200), &samples, true).unwrap();
        let jpx = encode_jpx(&photo(256, 128, 10), 256, 128, 3, 80).unwrap();
        let jpx = add_image(
            &mut doc,
            "/Width 256 /Height 128 /Filter /JPXDecode",
            &jpx,
            false,
        )
        .unwrap();
        let bits: Vec<u8> = (0..64 * 64 / 8)
            .map(|i| if i % 3 == 0 { 0xF0 } else { 0x0F })
            .collect();
        let bilevel = add_image(
            &mut doc,
            "/Width 64 /Height 64 /ColorSpace /DeviceGray /BitsPerComponent 1",
            &bits,
            true,
        )
        .unwrap();
        let icon = add_image(&mut doc, &gray_dict(20, 20), &[128u8; 400], true).unwrap();
        add_drawing_page(
            &mut doc,
            "q 300 0 0 200 50 500 cm /A Do Q q 150 0 0 100 400 500 cm /A Do Q \
             q 256 0 0 128 50 300 cm /J Do Q q 20 0 0 20 50 50 cm /I Do Q",
            vec![("A", a), ("J", jpx), ("I", icon.clone())],
        )
        .unwrap();
        add_drawing_page(
            &mut doc,
            "q 300 0 0 200 50 500 cm /B Do Q q 64 0 0 64 50 300 cm /Bits Do Q \
             q 20 0 0 20 50 50 cm /I Do Q",
            vec![("B", b), ("Bits", bilevel), ("I", icon)],
        )
        .unwrap();
        doc
    }

    #[test]
    fn repeats_and_images_alike_are_exported_once_and_small_ones_can_be_left_out() {
        let doc = repeats_document();
        let found = page_images(&doc, &[0, 1]).unwrap();
        // Drawn twice on the first page: found once there.
        assert_eq!(found.len(), 6);
        assert_ne!(found[0].num, found[3].num, "two objects");
        assert_eq!(found[0].key, found[3].key, "alike");
        assert_eq!(found[2].num, found[5].num, "the icon on both pages");

        let all = plan(&found, None);
        assert_eq!(all.images.len(), 4);
        assert_eq!(all.repeats, 2);
        assert_eq!(all.small, 0);
        let order: Vec<(usize, usize)> = all.images.iter().map(|i| (i.page, i.index)).collect();
        assert_eq!(order, vec![(0, 1), (0, 2), (0, 3), (1, 1)]);

        let big = plan(&found, Some(32));
        assert_eq!(big.images.len(), 3);
        assert_eq!(big.small, 1, "the icon, counted once");
        assert_eq!(big.repeats, 1);

        let dir = out_dir("repeats");
        let (files, written) = write_all(&doc, &all, &dir);
        assert_eq!(written.files, 4);
        // JPEG 2000: its own bytes.
        let jpx = &all.images[1];
        assert!(
            matches!(jpx.kind, ImageKind::Jpeg2000 { .. }),
            "{:?}",
            jpx.kind
        );
        let original = doc
            .new_indirect(jpx.num, 0)
            .unwrap()
            .read_raw_stream()
            .unwrap();
        assert!(std::fs::read(&files[1].path).unwrap() == original);
        // One bit a pixel stays one bit.
        let p = read_png(&files[3].path);
        assert_eq!(
            (p.color, p.depth, p.width, p.height),
            (png::ColorType::Grayscale, png::BitDepth::One, 64, 64)
        );
        assert_eq!(&p.data[..2], &[0xF0, 0x0F]);
    }

    #[test]
    fn an_image_with_a_finer_mask_is_exported_at_the_masks_size() {
        // A scan's text: colours kept coarse, the shapes sharp in the mask.
        let mut doc = PdfDocument::new();
        let mask: Vec<u8> = (0..200 * 160)
            .map(|i| if (i % 200) / 10 % 2 == 0 { 255 } else { 0 })
            .collect();
        let mask = add_image(&mut doc, &gray_dict(200, 160), &mask, true).unwrap();
        let mut colour =
            add_image(&mut doc, &rgb_dict(50, 40), &[200u8; 50 * 40 * 3], true).unwrap();
        colour.dict_put("SMask", mask).unwrap();
        add_drawing_page(
            &mut doc,
            "q 200 0 0 160 50 50 cm /C Do Q",
            vec![("C", colour)],
        )
        .unwrap();
        let found = page_images(&doc, &[0]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].width, found[0].height), (200, 160));
        let plan = plan(&found, None);
        let dir = out_dir("finer-mask");
        let (files, _) = write_all(&doc, &plan, &dir);
        let p = read_png(&files[0].path);
        assert_eq!(
            (p.color, p.width, p.height),
            (png::ColorType::Rgba, 200, 160)
        );
        let alpha = |x: usize| p.data[(80 * 200 + x) * 4 + 3];
        assert_eq!((alpha(5), alpha(15)), (255, 0), "the mask's stripes, sharp");
        assert!(
            p.data[(80 * 200 + 5) * 4].abs_diff(200) <= 2,
            "the colour under them"
        );
    }

    #[test]
    fn stopped_it_writes_nothing() {
        let doc = image_sample().unwrap();
        let plan = plan(&page_images(&doc, &[0, 1, 2]).unwrap(), None);
        let dir = out_dir("stopped");
        let files = files_for(&plan, &dir);
        let stop = AtomicBool::new(false);
        let result = write_images(&doc, &files, &stop, &mut |done, _| {
            if done >= 2 {
                stop.store(true, Ordering::Relaxed);
            }
        });
        assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "nothing left");
    }

    #[test]
    fn an_image_that_cannot_be_read_is_left_out_and_counted() {
        let doc = repeats_document();
        let plan = plan(&page_images(&doc, &[0]).unwrap(), None);
        let dir = out_dir("unreadable");
        let mut files = files_for(&plan, &dir);
        // The page's content stream: not an image.
        let contents = doc
            .find_page(0)
            .unwrap()
            .get_dict("Contents")
            .unwrap()
            .unwrap()
            .as_indirect()
            .unwrap();
        files.push(ImageFile {
            num: contents,
            kind: ImageKind::Png,
            path: dir.join("not-an-image.png"),
        });
        let written = write_images(&doc, &files, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        assert_eq!(
            written,
            Written {
                files: 3,
                unreadable: 1
            }
        );
        assert!(!dir.join("not-an-image.png").exists());
    }

    #[test]
    fn thumbnails_fit_their_side() {
        let doc = image_sample().unwrap();
        let bytes = thumbnail(&doc, xobject(&doc, 0, "Photo"), 192).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        assert_eq!((reader.info().width, reader.info().height), (192, 144));
        // A stencil mask, in greys.
        let bytes = thumbnail(&doc, xobject(&doc, 1, "Mask"), 100).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().width, 100);
        assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    }

    fn facts(space: SpaceKind) -> ImageFacts {
        ImageFacts {
            width: 10,
            height: 10,
            bits_per_component: 8,
            components: 3,
            image_mask: false,
            has_mask: false,
            use_decode: false,
            use_colorkey: false,
            decode_inverted: false,
            space,
            compressed: Some(Compressed::Jpeg),
            color_transform: None,
            compressed_len: 100,
        }
    }

    /// A JPEG of `n` components, with an Adobe marker of `transform` added if it has none.
    fn jpeg_with(n: usize, adobe: Option<u8>) -> Vec<u8> {
        let samples = vec![128u8; 16 * 16 * n];
        let jpeg = encode_jpeg(&samples, 16, 16, n, 90, false).unwrap();
        let mut out = jpeg.clone();
        if jpeg_info(&jpeg).unwrap().adobe_transform.is_none()
            && let Some(t) = adobe
        {
            let mut seg = vec![0xFF, 0xEE, 0, 14];
            seg.extend_from_slice(b"Adobe");
            seg.extend_from_slice(&[0, 100, 0, 0, 0, 0, t]);
            out.splice(2..2, seg);
        }
        out
    }

    #[test]
    fn jpegs_are_copied_only_when_the_file_shows_what_the_document_does() {
        let rgb = jpeg_with(3, None);
        assert_eq!(kind_of(&facts(SpaceKind::Rgb), Some(&rgb)), ImageKind::Jpeg);
        let decoded = ImageFacts {
            use_decode: true,
            ..facts(SpaceKind::Rgb)
        };
        assert_eq!(
            kind_of(&decoded, Some(&rgb)),
            ImageKind::Png,
            "its own /Decode"
        );
        let masked = ImageFacts {
            has_mask: true,
            ..facts(SpaceKind::Rgb)
        };
        assert_eq!(
            kind_of(&masked, Some(&rgb)),
            ImageKind::Png,
            "a mask, as alpha"
        );
        let untransformed = ImageFacts {
            color_transform: Some(0),
            ..facts(SpaceKind::Rgb)
        };
        assert_eq!(
            kind_of(&untransformed, Some(&rgb)),
            ImageKind::Png,
            "RGB samples a JPEG file would take for YCbCr"
        );
        assert_eq!(kind_of(&facts(SpaceKind::Lab), Some(&rgb)), ImageKind::Png);
        assert_eq!(
            kind_of(&facts(SpaceKind::Rgb), Some(b"not a jpeg")),
            ImageKind::Png
        );

        let cmyk = jpeg_with(4, Some(2));
        assert_eq!(jpeg_info(&cmyk).unwrap().components, 4);
        let cmyk_facts = ImageFacts {
            components: 4,
            ..facts(SpaceKind::Cmyk)
        };
        assert_eq!(
            kind_of(&cmyk_facts, Some(&cmyk)),
            ImageKind::Png,
            "PDF doesn't invert it"
        );
        let inverted = ImageFacts {
            use_decode: true,
            decode_inverted: true,
            ..cmyk_facts
        };
        assert_eq!(
            kind_of(&inverted, Some(&cmyk)),
            ImageKind::Jpeg,
            "inverted back, as Photoshop's"
        );
    }

    #[test]
    fn a_copied_jpeg_gets_the_documents_profile_and_stands_upright() {
        let jpeg = encode_jpeg(&vec![100u8; 16 * 16 * 3], 16, 16, 3, 90, false).unwrap();
        let info = jpeg_info(&jpeg).unwrap();
        assert_eq!(info.components, 3);
        assert!(!info.has_icc);
        // An EXIF segment after the JFIF one, turning the image (6: a quarter turn), in
        // Motorola byte order: its TIFF header, then one entry in IFD 0.
        let mut exif = b"Exif\0\0MM\0*".to_vec();
        exif.extend_from_slice(&[0, 0, 0, 8, 0, 1]);
        exif.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0]);
        exif.extend_from_slice(&[0, 0, 0, 0]);
        let mut seg = vec![0xFF, 0xE1];
        seg.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        seg.extend_from_slice(&exif);
        let mut turned = jpeg.clone();
        turned.splice(info.insert_at..info.insert_at, seg);
        let info = jpeg_info(&turned).unwrap();
        let (at, big) = info.orientation.expect("found");
        assert!(big);
        assert_eq!(&turned[at..at + 2], &[0, 6]);

        // A profile of RGB colours (only its header matters here), longer than a segment.
        let mut profile = vec![0u8; 70_000];
        profile[16..20].copy_from_slice(b"RGB ");
        let out = jpeg_file(&turned, Some(&profile));
        let after = jpeg_info(&out).unwrap();
        assert!(after.has_icc);
        assert_eq!(after.orientation, None, "set upright");
        assert_eq!(out.len(), turned.len() + profile.len() + 2 * 18);
        // After the JFIF and EXIF segments, before the rest.
        let i = info.insert_at;
        assert_eq!(&out[i..i + 2], &[0xFF, 0xE2]);
        assert_eq!(&out[i + 4..i + 16], b"ICC_PROFILE\0");
        assert_eq!(&out[i + 16..i + 18], &[1, 2]);
        assert_eq!(
            &out[out.len() - 2..],
            &[0xFF, 0xD9],
            "the image data follows whole"
        );

        // A profile of other colours, or a JPEG with one of its own, is left alone.
        let mut grey = profile.clone();
        grey[16..20].copy_from_slice(b"GRAY");
        assert!(jpeg_file(&jpeg, Some(&grey)) == jpeg);
        assert!(jpeg_file(&out, Some(&profile)) == out);
    }
}
