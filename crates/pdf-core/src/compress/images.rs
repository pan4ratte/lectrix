//! Rewriting a document's images (Compress, ADR 0027).
//!
//! Each image object is replaced where it is, keeping its object number, so whatever draws
//! it (pages, forms, patterns, annotation appearances) draws the new one. Its resolution is
//! the finest it needs: where it is drawn largest, over the whole document.
//!
//! Two ways to a smaller image:
//! - **Repacking** (no pixel changes): the stream of an image compressed losslessly is
//!   compressed again with Flate at its highest level, with PNG predictors where they
//!   help, or as CCITT Group 4 for black and white; the samples stay exactly as they were.
//! - **Re-encoding**: the image is decoded, and then, as the options say, made grey or
//!   black and white, resampled, and encoded as JPEG, JPEG 2000, Group 4 or Flate. A colour
//!   image whose pixels are all grey is stored as grey, and a grey one that is all black
//!   and white as one bit a pixel, which loses nothing when it was stored losslessly.
//!
//! Soft masks and stencil masks follow the image they belong to in resolution and are
//! always written losslessly. An image whose soft mask has `/Matte` (its colours
//! pre-multiplied by the mask's) is only repacked, and so is that mask: the two must keep
//! the same size, and the image the colour components `/Matte` gives.

use std::collections::HashMap;

use mupdf::pdf::{PdfDocument, PdfObject};

use super::pixels::{self, Pixels};
use super::{ColorMode, CompressOptions, Compression, Downsample, ImageReport, Progress};
use crate::error::{Error, Result};
use crate::ffi::compress::{self as raw, PixelSpace};
use crate::ffi::set_new_stream;

/// How an image object is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Image,
    /// Another image's `/SMask`.
    SoftMask,
    /// Another image's `/Mask` stream (a stencil mask).
    StencilMask,
}

/// What an image is, for choosing its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Color,
    Gray,
    /// One bit a pixel: black and white, or an image mask.
    Mono,
    /// A soft mask: grey coverage.
    Mask,
    /// A colour space Lectrix does not decode into pixels it can write back (Lab,
    /// separations, DeviceN): only repacked.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Replaced { downsampled: bool },
    Kept,
    Skipped,
}

/// Rewrites `doc`'s images as `options` say.
pub(super) fn rewrite(
    doc: &mut PdfDocument,
    options: &CompressOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<ImageReport> {
    let pages = usize::try_from(doc.page_count()?).unwrap_or(0);
    let mut dpi: HashMap<i32, f32> = HashMap::new();
    // The first page each image is drawn on, for the preview.
    let mut first_page: HashMap<i32, usize> = HashMap::new();
    for page in 0..pages {
        if !progress(Progress::Reading {
            done: page,
            total: pages,
        }) {
            return Err(Error::Cancelled);
        }
        // A page that can't be read leaves its images at their resolution.
        let _ = raw::walk_page_images(doc, page as i32, &mut |num, d| {
            let finest = dpi.entry(num).or_insert(d);
            if d < *finest {
                *finest = d;
            }
            first_page.entry(num).or_insert(page);
        });
    }

    let images = find_images(doc)?;
    let total = images.len();
    let mut report = ImageReport {
        total,
        ..ImageReport::default()
    };
    let mut largest = 0;
    for (done, (num, role)) in images.into_iter().enumerate() {
        if !progress(Progress::Images { done, total }) {
            return Err(Error::Cancelled);
        }
        let before = doc
            .new_indirect(num, 0)?
            .read_raw_stream()
            .map_or(0, |s| s.len());
        match rewrite_one(doc, num, role, dpi.get(&num).copied(), options)? {
            Outcome::Replaced { downsampled } => {
                report.replaced += 1;
                report.downsampled += usize::from(downsampled);
                if let Some(&page) = first_page.get(&num)
                    && before > largest
                {
                    largest = before;
                    report.preview_page = Some(page);
                }
            }
            Outcome::Kept => {}
            Outcome::Skipped => report.skipped += 1,
        }
    }
    if !progress(Progress::Images { done: total, total }) {
        return Err(Error::Cancelled);
    }
    Ok(report)
}

/// Every image XObject in `doc`, in object order, with how it is used.
fn find_images(doc: &PdfDocument) -> Result<Vec<(i32, Role)>> {
    let len = i32::try_from(doc.xref_len()?).unwrap_or(i32::MAX);
    let mut images = Vec::new();
    let mut roles = HashMap::new();
    for num in 1..len {
        let obj = doc.new_indirect(num, 0)?;
        if !obj.is_stream()? || name(&obj, "Subtype")?.as_deref() != Some("Image") {
            continue;
        }
        images.push(num);
        if let Some(smask) = obj.get_dict("SMask")?
            && smask.is_indirect()?
        {
            roles.insert(smask.as_indirect()?, Role::SoftMask);
        }
        if let Some(mask) = obj.get_dict("Mask")?
            && mask.is_indirect()?
            && mask.is_stream()?
        {
            roles.insert(mask.as_indirect()?, Role::StencilMask);
        }
    }
    Ok(images
        .into_iter()
        .map(|num| (num, roles.get(&num).copied().unwrap_or(Role::Image)))
        .collect())
}

fn name(obj: &PdfObject, key: &str) -> Result<Option<String>> {
    match obj.get_dict(key)? {
        Some(v) if v.is_name()? => Ok(Some(String::from_utf8_lossy(&v.as_name()?).into_owned())),
        _ => Ok(None),
    }
}

/// A colour space, as far as rewriting an image needs it.
enum Space {
    /// Grey, RGB or CMYK in some form (device, calibrated, ICC), with its components.
    Plain(usize),
    /// Indexed: one component an index into its base.
    Indexed { base: PdfObject, base_n: usize },
    /// Lab, separations, DeviceN, patterns and anything unknown.
    Other,
    /// None named: a JPEG 2000 image, whose codestream says.
    FromData,
}

fn space_of(cs: &PdfObject, depth: u8) -> Result<Space> {
    if depth > 4 {
        return Ok(Space::Other);
    }
    if cs.is_name()? {
        return Ok(match &*String::from_utf8_lossy(&cs.as_name()?) {
            "DeviceGray" | "G" | "CalGray" => Space::Plain(1),
            "DeviceRGB" | "RGB" | "CalRGB" => Space::Plain(3),
            "DeviceCMYK" | "CMYK" => Space::Plain(4),
            _ => Space::Other,
        });
    }
    if !cs.is_array()? || cs.len()? == 0 {
        return Ok(Space::Other);
    }
    let Some(family) = cs.get_array(0)? else {
        return Ok(Space::Other);
    };
    if !family.is_name()? {
        return Ok(Space::Other);
    }
    Ok(match &*String::from_utf8_lossy(&family.as_name()?) {
        "CalGray" => Space::Plain(1),
        "CalRGB" => Space::Plain(3),
        "ICCBased" => match cs.get_array(1)? {
            Some(profile) => match profile.get_dict("N")? {
                Some(n) if n.is_int()? && matches!(n.as_int()?, 1 | 3 | 4) => {
                    Space::Plain(n.as_int()? as usize)
                }
                _ => Space::Other,
            },
            None => Space::Other,
        },
        "Indexed" | "I" => match cs.get_array(1)? {
            Some(base) => match space_of(&base, depth + 1)? {
                Space::Plain(base_n) => Space::Indexed { base, base_n },
                _ => Space::Other,
            },
            None => Space::Other,
        },
        _ => Space::Other,
    })
}

/// What an image object says about itself.
struct Info {
    width: usize,
    height: usize,
    bpc: u8,
    image_mask: bool,
    space: Space,
    filters: Vec<String>,
    color_key: bool,
    smask_in_data: bool,
    /// A soft mask with `/Matte`, or an image that has one.
    matte: bool,
    raw_len: usize,
}

impl Info {
    fn read(obj: &PdfObject) -> Result<Option<Info>> {
        let int = |key: &str| -> Result<i64> {
            Ok(match obj.get_dict(key)? {
                Some(v) if v.is_number()? => v.as_float()? as i64,
                _ => 0,
            })
        };
        let (width, height) = (int("Width")?, int("Height")?);
        if width <= 0 || height <= 0 || width > 1 << 20 || height > 1 << 20 {
            return Ok(None);
        }
        let image_mask =
            matches!(obj.get_dict("ImageMask")?, Some(v) if v.is_bool()? && v.as_bool()?);
        let mut filters = Vec::new();
        if let Some(filter) = obj.get_dict("Filter")? {
            if filter.is_name()? {
                filters.push(String::from_utf8_lossy(&filter.as_name()?).into_owned());
            } else if filter.is_array()? {
                for i in 0..filter.len()? {
                    if let Some(f) = filter.get_array(i as i32)?
                        && f.is_name()?
                    {
                        filters.push(String::from_utf8_lossy(&f.as_name()?).into_owned());
                    }
                }
            }
        }
        // JPEG 2000 may leave its depth and colour space to its codestream; MuPDF decodes
        // it to 8 bits.
        let jpx = filters.last().is_some_and(|f| f == "JPXDecode");
        let bpc = match obj.get_dict("BitsPerComponent")? {
            _ if image_mask => 1,
            Some(v) if v.is_number()? => v.as_float()? as i64,
            _ if jpx => 8,
            _ => 0,
        };
        if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
            return Ok(None);
        }
        let space = if image_mask {
            Space::Plain(1)
        } else {
            match obj.get_dict("ColorSpace")? {
                Some(cs) => space_of(&cs, 0)?,
                None if jpx => Space::FromData,
                None => Space::Other,
            }
        };
        let color_key = matches!(obj.get_dict("Mask")?, Some(m) if m.is_array()?);
        let smask_in_data = int("SMaskInData")? != 0;
        let matte = obj.get_dict("Matte")?.is_some()
            || match obj.get_dict("SMask")? {
                Some(smask) if smask.is_dict()? => smask.get_dict("Matte")?.is_some(),
                _ => false,
            };
        Ok(Some(Info {
            width: width as usize,
            height: height as usize,
            bpc: bpc as u8,
            image_mask,
            space,
            filters,
            color_key,
            smask_in_data,
            matte,
            raw_len: obj.read_raw_stream()?.len(),
        }))
    }

    fn last_filter(&self) -> &str {
        self.filters.last().map(String::as_str).unwrap_or("")
    }

    /// Compressed with loss: JPEG or JPEG 2000.
    fn lossy(&self) -> bool {
        matches!(self.last_filter(), "DCTDecode" | "DCT" | "JPXDecode")
    }

    fn jpx(&self) -> bool {
        self.last_filter() == "JPXDecode"
    }

    fn jbig2(&self) -> bool {
        self.filters.iter().any(|f| f == "JBIG2Decode")
    }

    /// Its decoded stream holds its samples as they are: only general-purpose filters.
    fn repackable(&self) -> bool {
        self.filters.iter().all(|f| {
            matches!(
                f.as_str(),
                "FlateDecode"
                    | "Fl"
                    | "LZWDecode"
                    | "LZW"
                    | "RunLengthDecode"
                    | "RL"
                    | "ASCIIHexDecode"
                    | "AHx"
                    | "ASCII85Decode"
                    | "A85"
            )
        })
    }
}

/// A new stream for an image, and what its dictionary must say about it.
struct Encoded {
    data: Vec<u8>,
    filter: &'static str,
    parms: Option<String>,
}

/// The colour space a re-encoded image is written with.
enum NewSpace {
    /// The one it has.
    Same,
    Gray,
    /// An Indexed image's base: its samples are colours now, not indices.
    Base(PdfObject),
    /// An image mask has none.
    None,
    /// A device space, for an image that named none (JPEG 2000) written in another form.
    Device(&'static str),
}

fn rewrite_one(
    doc: &mut PdfDocument,
    num: i32,
    role: Role,
    dpi: Option<f32>,
    options: &CompressOptions,
) -> Result<Outcome> {
    let obj = doc.new_indirect(num, 0)?;
    let Some(info) = Info::read(&obj)? else {
        return Ok(Outcome::Skipped);
    };
    if info.color_key {
        return Ok(Outcome::Skipped);
    }
    if info.jbig2() {
        // JBIG2 is about as small as black and white gets.
        return Ok(Outcome::Kept);
    }
    let class = match (&info.space, role) {
        _ if info.image_mask || role == Role::StencilMask => Class::Mono,
        (Space::Plain(1), Role::SoftMask) => Class::Mask,
        (_, Role::SoftMask) => Class::Other,
        (Space::Plain(1), _) if info.bpc == 1 => Class::Mono,
        (Space::Plain(1), _) | (Space::Indexed { base_n: 1, .. }, _) => Class::Gray,
        (Space::Plain(_), _) | (Space::Indexed { .. }, _) => Class::Color,
        (Space::Other, _) => Class::Other,
        // Taken for colour; its pixels say what it is.
        (Space::FromData, _) => Class::Color,
    };
    // What the options make of it, before its pixels are seen.
    let target = match (class, options.color_mode, role) {
        (Class::Color, ColorMode::Gray, Role::Image) => Class::Gray,
        (Class::Color | Class::Gray, ColorMode::BlackAndWhite, Role::Image) => Class::Mono,
        (c, ..) => c,
    };
    let lossy = info.lossy();
    let reencode_lossy = |c: Class| match c {
        Class::Color => options.color.compression != Compression::Keep,
        Class::Gray => options.gray.compression != Compression::Keep,
        _ => false,
    };
    let downsampling = downsample_for(target, options)
        .zip(dpi)
        .is_some_and(|(d, dpi)| dpi > d.above_dpi as f32);
    let detect = role == Role::Image
        && !lossy
        && info.bpc == 8
        && matches!(info.space, Space::Plain(1 | 3))
        && matches!(class, Class::Color | Class::Gray);
    // Group 4 is written again only from Group 3 or byte-aligned rows: as Group 4 it would
    // come out the same.
    let mono_reencode = class == Class::Mono && !info.repackable() && !plain_group_4(&obj)?;
    // A JPEG or JPEG 2000 image already under 0.4 bits a pixel is not re-encoded at its own
    // size: JPEG would not make it a tenth smaller, and decoding it would take long for
    // nothing (scans in layers have thousands).
    let compact = info.lossy() && (info.raw_len as f64) < (info.width * info.height) as f64 * 0.05;
    let wants_pixels = class != Class::Other
        && !info.smask_in_data
        && !info.matte
        && (downsampling
            || target != class
            || (reencode_lossy(target) && !compact)
            || detect
            || mono_reencode);

    if wants_pixels {
        match reencode(doc, &obj, &info, class, target, dpi, options) {
            Ok(Some((encoded, size, space, downsampled))) => {
                write(doc, &obj, &encoded, size, space)?;
                return Ok(Outcome::Replaced { downsampled });
            }
            Ok(None) => {}
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            // A damaged image stays as it is, and is still repacked if it can be.
            Err(_) => {
                return match repack(doc, &obj, &info)? {
                    true => Ok(Outcome::Replaced { downsampled: false }),
                    false => Ok(Outcome::Skipped),
                };
            }
        }
    }
    if repack(doc, &obj, &info)? {
        return Ok(Outcome::Replaced { downsampled: false });
    }
    Ok(if class == Class::Other && !info.repackable() {
        Outcome::Skipped
    } else {
        Outcome::Kept
    })
}

/// The image is CCITT Group 4 (`/K` below 0) without rows aligned to bytes.
fn plain_group_4(obj: &PdfObject) -> Result<bool> {
    let Some(parms) = obj.get_dict("DecodeParms")? else {
        return Ok(false);
    };
    // With several filters, the parameters are an array; CCITT is then not alone.
    if !parms.is_dict()? {
        return Ok(false);
    }
    let k = match parms.get_dict("K")? {
        Some(k) if k.is_number()? => k.as_float()?,
        _ => 0.0,
    };
    let aligned =
        matches!(parms.get_dict("EncodedByteAlign")?, Some(a) if a.is_bool()? && a.as_bool()?);
    Ok(k < 0.0 && !aligned)
}

fn downsample_for(class: Class, options: &CompressOptions) -> Option<Downsample> {
    match class {
        Class::Color => options.color.downsample,
        Class::Gray | Class::Mask => options.gray.downsample,
        Class::Mono => options.mono,
        Class::Other => None,
    }
}

/// The size a resampling makes of `width` x `height` drawn at `dpi`, if it makes it
/// smaller.
fn new_size(
    width: usize,
    height: usize,
    dpi: Option<f32>,
    d: Option<Downsample>,
) -> Option<(usize, usize)> {
    let (dpi, d) = (dpi?, d?);
    if dpi <= d.above_dpi as f32 {
        return None;
    }
    let scale = f64::from(d.to_dpi) / f64::from(dpi);
    let w = ((width as f64 * scale).round() as usize).max(1);
    let h = ((height as f64 * scale).round() as usize).max(1);
    (w < width || h < height).then_some((w.min(width), h.min(height)))
}

/// Decodes the image and encodes it again as the options say. `None` if that changes
/// nothing or makes nothing smaller.
#[allow(clippy::type_complexity)]
fn reencode(
    doc: &PdfDocument,
    obj: &PdfObject,
    info: &Info,
    class: Class,
    mut target: Class,
    dpi: Option<f32>,
    options: &CompressOptions,
) -> Result<Option<(Encoded, (usize, usize, u8), NewSpace, bool)>> {
    let num = obj.as_indirect()?;
    let decoded = raw::decode_image(doc, num)?;
    if decoded.width != info.width || decoded.height != info.height {
        return Ok(None);
    }
    let lossy = info.lossy();
    let mut space = NewSpace::Same;
    // The pixels to encode: grey (0 black) for grey, black-and-white and masks.
    let mut px = match (class, decoded.space) {
        // An image mask decodes as coverage: 255 where it paints. Black paints.
        (Class::Mono, PixelSpace::None) if decoded.n == 1 => {
            let mut data = decoded.to_vec();
            data.iter_mut().for_each(|v| *v = 255 - *v);
            Pixels {
                width: decoded.width,
                height: decoded.height,
                n: 1,
                data,
            }
        }
        (_, PixelSpace::None | PixelSpace::Other) => return Ok(None),
        _ if decoded.alpha => return Ok(None),
        (Class::Mono | Class::Gray | Class::Mask, PixelSpace::Gray) => gray_pixels(&decoded),
        // A JPEG 2000 image without a colour space that decodes to grey.
        (Class::Color, PixelSpace::Gray) if matches!(info.space, Space::FromData) => {
            if target == Class::Color {
                target = Class::Gray;
            }
            gray_pixels(&decoded)
        }
        (Class::Color, PixelSpace::Rgb | PixelSpace::Cmyk) => Pixels {
            width: decoded.width,
            height: decoded.height,
            n: decoded.n,
            data: decoded.to_vec(),
        },
        _ => return Ok(None),
    };
    if let Space::Indexed { base, .. } = &info.space {
        space = NewSpace::Base(base.try_clone()?);
    }
    let mut changed = false;

    // A colour image that is grey throughout is stored as grey: exactly so when it was
    // stored losslessly, within JPEG's colour noise otherwise.
    if class == Class::Color
        && decoded.space == PixelSpace::Rgb
        && pixels::rgb_is_gray(&px.data, if lossy { 6 } else { 0 })
    {
        px = Pixels {
            data: pixels::gray_from_gray_rgb(&px.data),
            n: 1,
            ..px
        };
        space = NewSpace::Gray;
        changed = true;
        if target == Class::Color {
            target = Class::Gray;
        }
    }
    // Colour mode: grey through MuPDF's colour management.
    if px.n > 1 && matches!(target, Class::Gray | Class::Mono) {
        px = gray_pixels(&decoded.to_gray()?);
        space = NewSpace::Gray;
        changed = true;
    }
    // A grey image stored losslessly with nothing but black and white is one bit a pixel.
    let mut bitonal = class == Class::Mono;
    if !lossy
        && target == Class::Gray
        && class != Class::Mask
        && info.bpc <= 8
        && pixels::is_bitonal(&px.data)
    {
        target = Class::Mono;
        bitonal = true;
        if class != Class::Mono {
            changed = true;
        }
    }
    if target == Class::Mono && !info.image_mask && class != Class::Mono {
        space = NewSpace::Gray;
    }
    if info.image_mask {
        space = NewSpace::None;
    }
    // An image whose codestream named its colours is written with them named.
    if matches!(info.space, Space::FromData) && matches!(space, NewSpace::Same) {
        space = NewSpace::Device(match px.n {
            1 => "DeviceGray",
            4 => "DeviceCMYK",
            _ => "DeviceRGB",
        });
    }

    // Resolution.
    let mut downsampled = false;
    if let Some((w, h)) = new_size(px.width, px.height, dpi, downsample_for(target, options)) {
        let method =
            downsample_for(target, options).map_or(pixels::Resample::Bicubic, |d| d.method);
        px = pixels::resample(&px, w, h, method);
        downsampled = true;
        changed = true;
    }

    let (encoded, bpc, lossy_out) = match target {
        Class::Mono => {
            let threshold = if bitonal { 127 } else { pixels::otsu(&px.data) };
            if !bitonal {
                changed = true;
            }
            (mono(&px, threshold)?, 1, false)
        }
        Class::Mask => (flate_predicted(&px, 8)?, 8, false),
        Class::Gray | Class::Color => {
            let settings = if target == Class::Color {
                options.color
            } else {
                options.gray
            };
            // Drawings and screenshots stay lossless: JPEG would blur them.
            let graphic = !lossy && pixels::looks_drawn(&px.data, px.n);
            let codec = match settings.compression {
                _ if graphic => Codec::Flate,
                Compression::Jpeg => Codec::Jpeg,
                Compression::Jpeg2000 => Codec::Jpx,
                Compression::Keep if info.jpx() => Codec::Jpx,
                Compression::Keep if lossy => Codec::Jpeg,
                Compression::Keep => Codec::Flate,
            };
            let encoded = match codec {
                Codec::Flate => flate_predicted(&px, 8)?,
                Codec::Jpeg => Encoded {
                    data: raw::encode_jpeg(
                        &px.data,
                        px.width,
                        px.height,
                        px.n,
                        settings.quality,
                        px.n == 3 && settings.quality < 90,
                    )?,
                    filter: "DCTDecode",
                    parms: None,
                },
                Codec::Jpx => Encoded {
                    data: raw::encode_jpx(
                        &px.data,
                        px.width,
                        px.height,
                        px.n,
                        settings.quality.min(99),
                    )?,
                    filter: "JPXDecode",
                    parms: None,
                },
            };
            (encoded, 8, codec != Codec::Flate)
        }
        Class::Other => return Ok(None),
    };

    // Lossy re-encoding at the same size and colours loses a generation of quality: worth
    // it only for a tenth of the size.
    let worth = if lossy_out && !changed {
        encoded.data.len() * 10 <= info.raw_len * 9
    } else {
        encoded.data.len() < info.raw_len
    };
    Ok(worth.then_some((encoded, (px.width, px.height, bpc), space, downsampled)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Codec {
    Flate,
    Jpeg,
    Jpx,
}

fn gray_pixels(decoded: &raw::Decoded) -> Pixels {
    Pixels {
        width: decoded.width,
        height: decoded.height,
        n: 1,
        data: decoded.to_vec(),
    }
}

/// Black and white (values at or below `threshold` black) as the smaller of Group 4 and
/// Flate, both lossless; 0 is black either way.
fn mono(px: &Pixels, threshold: u8) -> Result<Encoded> {
    let bits = pixels::invert(&pixels::pack_black(
        &px.data, px.width, px.height, threshold,
    ));
    let g4 = raw::encode_g4(&bits, px.width, px.height)?;
    // Flate's highest level takes long on a scanned page, which Group 4 wins anyway: a
    // quick Flate decides, and the slow one is made only if Flate wins.
    let mut flate = raw::deflate(&bits, 3)?;
    if flate.len() < g4.len() {
        let tighter = raw::deflate(&bits, 9)?;
        if tighter.len() < flate.len() {
            flate = tighter;
        }
    }
    Ok(if g4.len() <= flate.len() {
        Encoded {
            data: g4,
            filter: "CCITTFaxDecode",
            parms: Some(format!(
                "<< /K -1 /Columns {} /Rows {} >>",
                px.width, px.height
            )),
        }
    } else {
        Encoded {
            data: flate,
            filter: "FlateDecode",
            parms: None,
        }
    })
}

/// 8-bit samples with PNG predictors and Flate, or Flate alone if that is smaller.
fn flate_predicted(px: &Pixels, bpc: u8) -> Result<Encoded> {
    let row = px.width * px.n;
    let predicted = raw::deflate(&pixels::png_predict(&px.data, row, px.height, px.n), 9)?;
    let plain = raw::deflate(&px.data, 9)?;
    Ok(if predicted.len() < plain.len() {
        Encoded {
            data: predicted,
            filter: "FlateDecode",
            parms: Some(format!(
                "<< /Predictor 15 /Colors {} /BitsPerComponent {bpc} /Columns {} >>",
                px.n, px.width
            )),
        }
    } else {
        Encoded {
            data: plain,
            filter: "FlateDecode",
            parms: None,
        }
    })
}

/// Writes a re-encoded image: its stream, size, depth and colour space. Its `/Decode`
/// went into the pixels.
fn write(
    doc: &mut PdfDocument,
    obj: &PdfObject,
    encoded: &Encoded,
    (width, height, bpc): (usize, usize, u8),
    space: NewSpace,
) -> Result<()> {
    set_new_stream(doc, obj, &encoded.data)?;
    let mut obj = doc.new_indirect(obj.as_indirect()?, 0)?;
    set_filter(doc, &mut obj, encoded)?;
    obj.dict_put("Width", PdfObject::new_int(width as i32)?)?;
    obj.dict_put("Height", PdfObject::new_int(height as i32)?)?;
    obj.dict_put("BitsPerComponent", PdfObject::new_int(i32::from(bpc))?)?;
    match space {
        NewSpace::Same => {}
        NewSpace::Gray => obj.dict_put("ColorSpace", PdfObject::new_name("DeviceGray")?)?,
        NewSpace::Base(base) => obj.dict_put("ColorSpace", base)?,
        NewSpace::None => obj.dict_delete("ColorSpace")?,
        NewSpace::Device(name) => obj.dict_put("ColorSpace", PdfObject::new_name(name)?)?,
    }
    for key in ["Decode", "DL", "SMaskInData"] {
        obj.dict_delete(key)?;
    }
    Ok(())
}

fn set_filter(doc: &PdfDocument, obj: &mut PdfObject, encoded: &Encoded) -> Result<()> {
    obj.dict_put("Filter", PdfObject::new_name(encoded.filter)?)?;
    match &encoded.parms {
        Some(parms) => obj.dict_put("DecodeParms", doc.new_object_from_str(parms)?)?,
        None => obj.dict_delete("DecodeParms")?,
    }
    Ok(())
}

/// Compresses a losslessly stored image's samples again, exactly as they are, if that
/// makes it smaller: Flate at its highest level with or without PNG predictors, or Group
/// 4 for one bit a pixel. True if it was replaced.
fn repack(doc: &mut PdfDocument, obj: &PdfObject, info: &Info) -> Result<bool> {
    if !info.repackable() {
        return Ok(false);
    }
    let Ok(mut data) = obj.read_stream() else {
        return Ok(false);
    };
    let (w, h, bpc) = (info.width, info.height, usize::from(info.bpc));
    let known = match info.space {
        Space::Plain(n) => Some(n),
        Space::Indexed { .. } => Some(1),
        Space::Other | Space::FromData => None,
    };
    // A colour space Lectrix doesn't know still has a whole number of components a pixel.
    let colors = known.or_else(|| (1..=32).find(|&c| data.len() == h * (w * c * bpc).div_ceil(8)));
    let Some(colors) = colors else {
        return Ok(false);
    };
    let row = (w * colors * bpc).div_ceil(8);
    if data.len() < row * h {
        return Ok(false);
    }
    data.truncate(row * h);

    let mut best = Encoded {
        data: raw::deflate(
            &pixels::png_predict(&data, row, h, predictor_bpp(colors, bpc)),
            9,
        )?,
        filter: "FlateDecode",
        parms: Some(format!(
            "<< /Predictor 15 /Colors {colors} /BitsPerComponent {bpc} /Columns {w} >>"
        )),
    };
    let plain = raw::deflate(&data, 9)?;
    if plain.len() < best.data.len() {
        best = Encoded {
            data: plain,
            filter: "FlateDecode",
            parms: None,
        };
    }
    if colors == 1 && bpc == 1 {
        // The bits as they are: Group 4 gives them back so with /BlackIs1 false.
        let g4 = raw::encode_g4(&data, w, h)?;
        if g4.len() < best.data.len() {
            best = Encoded {
                data: g4,
                filter: "CCITTFaxDecode",
                parms: Some(format!("<< /K -1 /Columns {w} /Rows {h} >>")),
            };
        }
    }
    if best.data.len() >= info.raw_len {
        return Ok(false);
    }
    set_new_stream(doc, obj, &best.data)?;
    let mut obj = doc.new_indirect(obj.as_indirect()?, 0)?;
    set_filter(doc, &mut obj, &best)?;
    obj.dict_delete("DL")?;
    Ok(true)
}

/// The bytes a pixel that PNG predictors compare across, as a reader takes them for
/// `/Colors` and `/BitsPerComponent`: rounded up, so 12 bits a pixel are 2 (PDF 7.4.4.4,
/// MuPDF's `fz_open_predict`), and at least 1.
fn predictor_bpp(colors: usize, bpc: usize) -> usize {
    (colors * bpc).div_ceil(8).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predictors_compare_whole_bytes_a_pixel_rounded_up() {
        assert_eq!(predictor_bpp(1, 1), 1);
        assert_eq!(predictor_bpp(1, 4), 1);
        assert_eq!(predictor_bpp(3, 4), 2, "12 bits a pixel");
        assert_eq!(predictor_bpp(5, 4), 3, "20 bits a pixel");
        assert_eq!(predictor_bpp(3, 8), 3);
        assert_eq!(predictor_bpp(4, 16), 8);
    }

    #[test]
    fn resampling_keeps_the_resolution_where_the_image_is_drawn_largest() {
        let d = Some(Downsample {
            to_dpi: 150,
            above_dpi: 225,
            method: pixels::Resample::Bicubic,
        });
        assert_eq!(new_size(3000, 2000, Some(600.0), d), Some((750, 500)));
        assert_eq!(
            new_size(3000, 2000, Some(200.0), d),
            None,
            "under the threshold"
        );
        assert_eq!(new_size(3000, 2000, None, d), None, "never drawn");
        assert_eq!(new_size(3000, 2000, Some(600.0), None), None);
        assert_eq!(new_size(2, 2, Some(10_000.0), d), Some((1, 1)));
    }
}
