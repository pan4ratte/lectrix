//! Compress (section 6.11, ADR 0027): a PDF written smaller, as a new file, with its images
//! downsampled and recompressed as the options say, its fonts subset, what the user chose
//! removed, and its structure packed in object streams.
//!
//! Images are replaced where they are, object by object: page contents, annotations and
//! their appearances are not rewritten, and an image drawn on many pages is encoded once.
//! Where each image is drawn, and so its resolution, comes from reading every page
//! (`ffi::compress::walk_page_images`). Each image is replaced only if what replaces it is
//! smaller; a lossy image re-encoded at its own size only if that saves a tenth.

mod analyze;
mod cleanup;
mod dedupe;
mod fonts;
mod images;
pub mod pixels;

use std::path::Path;

pub use analyze::{Analysis, Found, Usage, analyze};
pub use pixels::Resample;

use crate::docinfo::{self, PdfA};
use crate::error::{Error, Result};
use crate::save::{Encrypt, SaveKind, save_atomic_encrypted};

/// How images of one kind (colour or greyscale) are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSettings {
    /// Fewer pixels for images finer than a resolution; `None` keeps every pixel.
    pub downsample: Option<Downsample>,
    pub compression: Compression,
    /// JPEG or JPEG 2000 quality, 1 to 100.
    pub quality: u8,
}

/// Images drawn finer than `above_dpi` are resampled to `to_dpi`, where they are drawn
/// largest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Downsample {
    pub to_dpi: u32,
    pub above_dpi: u32,
    pub method: Resample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Each image keeps its kind of compression: JPEG stays JPEG, JPEG 2000 stays JPEG
    /// 2000 (both re-encoded only when resampled), and lossless stays lossless, packed
    /// tighter where it can be.
    Keep,
    Jpeg,
    Jpeg2000,
}

/// What images become, whatever their colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Keep,
    /// Colour images become greyscale.
    Gray,
    /// Colour and greyscale images become black and white.
    BlackAndWhite,
}

/// What is removed from the file. Each is removed only where it is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Removals {
    /// Embedded page thumbnails (`/Thumb`).
    pub thumbnails: bool,
    /// Other apps' private data (`/PieceInfo`): Illustrator's and Photoshop's editing data.
    pub private_data: bool,
    /// Files attached to the document (its `/EmbeddedFiles`); attachment annotations stay.
    pub attachments: bool,
    /// JavaScript, in the document, its pages, its links and its form fields.
    pub javascript: bool,
    /// The structure tree (tags), which screen readers and reflow use.
    pub tags: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressOptions {
    pub color: ImageSettings,
    pub gray: ImageSettings,
    /// Black-and-white images: resampled at most (they are always written losslessly, as
    /// CCITT Group 4 or Flate, whichever is smaller).
    pub mono: Option<Downsample>,
    pub color_mode: ColorMode,
    pub subset_fonts: bool,
    /// Objects packed in object streams with a cross-reference stream (PDF 1.5).
    pub object_streams: bool,
    pub remove: Removals,
}

/// The presets the dialog offers (section 6.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Nothing visible changes: images packed tighter without losing a pixel, fonts
    /// subset, the structure compacted.
    Lossless,
    HighQuality,
    Balanced,
    Smallest,
}

impl Preset {
    pub fn options(self) -> CompressOptions {
        let images = |to_dpi: u32, quality: u8| ImageSettings {
            downsample: Some(Downsample {
                to_dpi,
                above_dpi: to_dpi * 3 / 2,
                method: Resample::Bicubic,
            }),
            compression: Compression::Jpeg,
            quality,
        };
        let mono = |to_dpi: u32| {
            Some(Downsample {
                to_dpi,
                above_dpi: to_dpi * 3 / 2,
                method: Resample::Bicubic,
            })
        };
        let keep = ImageSettings {
            downsample: None,
            compression: Compression::Keep,
            quality: 85,
        };
        let (color, gray, mono) = match self {
            Preset::Lossless => (keep, keep, None),
            Preset::HighQuality => (images(300, 85), images(300, 85), mono(600)),
            Preset::Balanced => (images(150, 75), images(150, 75), mono(300)),
            Preset::Smallest => (images(96, 50), images(96, 50), mono(300)),
        };
        CompressOptions {
            color,
            gray,
            mono,
            color_mode: ColorMode::Keep,
            subset_fonts: true,
            object_streams: true,
            remove: Removals {
                thumbnails: true,
                private_data: true,
                ..Removals::default()
            },
        }
    }
}

/// How far compressing has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Finding where each image is drawn, page by page.
    Reading {
        done: usize,
        total: usize,
    },
    Images {
        done: usize,
        total: usize,
    },
    Fonts,
    Writing,
}

/// What became of the images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImageReport {
    /// Image objects in the file (soft masks and stencil masks included).
    pub total: usize,
    /// Replaced by something smaller.
    pub replaced: usize,
    /// Of those, resampled to fewer pixels.
    pub downsampled: usize,
    /// The page with the largest image replaced (0-based), for a preview.
    pub preview_page: Option<usize>,
    /// Left as they were because Lectrix can't rewrite them safely (colour-key masks,
    /// separations and DeviceN, Lab, JBIG2, damaged images).
    pub skipped: usize,
}

/// What was removed, by count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RemovedReport {
    pub thumbnails: usize,
    pub private_data: usize,
    pub attachments: usize,
    pub javascript: usize,
    pub tags: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressReport {
    /// File sizes, in bytes.
    pub before: u64,
    pub after: u64,
    pub pages: usize,
    pub images: ImageReport,
    /// Fonts subset to the glyphs they show.
    pub fonts_subset: usize,
    pub removed: RemovedReport,
    /// The PDF/A claim the result no longer makes (section 5.4).
    pub pdfa_removed: Option<PdfA>,
}

/// Compresses `path`, opened with `password`, into `target` (which may be `path` itself:
/// it is replaced in one rename once the new file is written). Encryption is kept.
/// `progress` is told how far it has come; returning false stops with
/// [`Error::Cancelled`] and writes nothing.
pub fn compress_file(
    path: &Path,
    password: Option<&str>,
    options: &CompressOptions,
    target: &Path,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<CompressReport> {
    validate(options)?;
    let before = std::fs::metadata(path)?.len();
    let mut doc = crate::unprotect::load(path, password)?;
    let pages = usize::try_from(doc.page_count()?).unwrap_or(0);
    let images = images::rewrite(&mut doc, options, progress)?;
    if !progress(Progress::Fonts) {
        return Err(Error::Cancelled);
    }
    let fonts_subset = if options.subset_fonts {
        fonts::subset(&mut doc)?
    } else {
        0
    };
    let removed = cleanup::remove(&mut doc, &options.remove)?;
    dedupe::merge_duplicates(&mut doc)?;
    // Lectrix cannot vouch that what it wrote still complies (section 5.4).
    let pdfa_removed = docinfo::remove_pdfa_claim(&mut doc)?;
    if !progress(Progress::Writing) {
        return Err(Error::Cancelled);
    }
    save_atomic_encrypted(
        &doc,
        SaveKind::Compressed {
            object_streams: options.object_streams,
        },
        None,
        target,
        Encrypt::Keep,
        &mut || {
            if progress(Progress::Writing) {
                Ok(())
            } else {
                Err(Error::Cancelled)
            }
        },
    )?;
    let after = std::fs::metadata(target)?.len();
    Ok(CompressReport {
        before,
        after,
        pages,
        images,
        fonts_subset,
        removed,
        pdfa_removed,
    })
}

/// Refuses options MuPDF or the encoders would take badly.
pub fn validate(options: &CompressOptions) -> Result<()> {
    let bad = |why: &str| Err(Error::InvalidArgument(why.into()));
    for settings in [options.color, options.gray] {
        if !(1..=100).contains(&settings.quality) {
            return bad("image quality must be 1 to 100");
        }
    }
    let downsamples = [
        options.color.downsample,
        options.gray.downsample,
        options.mono,
    ];
    for d in downsamples.into_iter().flatten() {
        if !(9..=2400).contains(&d.to_dpi) || d.above_dpi < d.to_dpi {
            return bad(
                "downsampling needs a resolution of 9 to 2400 dpi, and a threshold no lower",
            );
        }
    }
    Ok(())
}
