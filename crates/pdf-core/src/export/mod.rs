//! Pages exported as images (section 6.14, ADR 0034): each page drawn as it shows, at a
//! resolution, in colour, greys or black and white, and written as PNG, JPEG or TIFF (a
//! file a page, or one file of all of them).
//!
//! [`render`] draws a page, [`encode`] writes it in a format, [`names`] names the files,
//! and [`run`] exports a job's pages on worker threads, into temporary files that take
//! their names only once every page is written. [`copy`] draws a page for the clipboard.

pub mod copy;
pub mod encode;
pub mod names;
mod run;

pub use run::{Job, Output, run};

use mupdf::{Colorspace, DisplayList, Matrix};

use crate::compress::pixels;
use crate::error::{Error, Result};

/// Lowest and highest resolution, dots per inch.
pub const DPI_RANGE: (u16, u16) = (10, 2400);

/// The most pixels a page is exported with: an A4 page at 1,200 dpi has 140 million.
pub const MAX_PIXELS: u64 = 150_000_000;

/// What a page is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Png,
    Jpeg,
    Tiff,
}

impl Format {
    /// The extension a file of this format is given.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Tiff => "tif",
        }
    }

    /// Every extension a file of this format may have (lower case).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Format::Png => &["png"],
            Format::Jpeg => &["jpg", "jpeg", "jpe"],
            Format::Tiff => &["tif", "tiff"],
        }
    }

    /// What the format is called, in a Save dialog's list of file types.
    pub fn name(self) -> &'static str {
        match self {
            Format::Png => "PNG image",
            Format::Jpeg => "JPEG image",
            Format::Tiff => "TIFF image",
        }
    }

    /// What the format is called in a sentence: "PNG", "TIFF".
    pub fn short_name(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Tiff => "TIFF",
        }
    }

    /// The widest or tallest image the format holds, in pixels.
    pub fn max_side(self) -> u32 {
        match self {
            // JPEG's header has 16 bits for each side.
            Format::Jpeg => 65_535,
            Format::Png | Format::Tiff => 1 << 20,
        }
    }

    /// Whether the format keeps black and white as one bit a pixel (PNG; TIFF with CCITT
    /// Group 4). The others would only make large grey files of it.
    pub fn takes_black_and_white(self) -> bool {
        matches!(self, Format::Png | Format::Tiff)
    }
}

/// The colours a page is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Colour {
    Colour,
    Grey,
    /// One bit a pixel, split at Otsu's threshold, as Compress splits images (ADR 0027).
    BlackAndWhite,
}

/// How a TIFF file's colour and grey pages are compressed; black and white is always
/// CCITT Group 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TiffCompression {
    Lzw,
    Deflate,
}

/// How pages are exported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub format: Format,
    /// Dots per inch: the page's points times `dpi / 72` make its pixels.
    pub dpi: u16,
    pub colour: Colour,
    /// 1 to 100, for JPEG.
    pub quality: u8,
    pub tiff: TiffCompression,
    /// Annotations drawn on the page, as on screen; form fields always are.
    pub annotations: bool,
}

impl Options {
    /// Bytes of memory a page takes while it is drawn and written, a pixel: the page drawn,
    /// its copy, and what the encoder keeps (measured on a page at 600 dpi, ADR 0034).
    pub fn bytes_per_pixel(&self) -> u64 {
        let channels: u64 = if self.colour == Colour::Colour { 3 } else { 1 };
        match self.format {
            Format::Png | Format::Tiff => 2 * channels,
            Format::Jpeg => 3 * channels,
        }
    }

    /// Whether the options go together; why not if they don't.
    pub fn check(&self) -> Result<()> {
        if !(DPI_RANGE.0..=DPI_RANGE.1).contains(&self.dpi) {
            return Err(Error::InvalidArgument(format!(
                "a resolution of {} dpi is out of range",
                self.dpi
            )));
        }
        if !(1..=100).contains(&self.quality) {
            return Err(Error::InvalidArgument(format!(
                "a quality of {} is out of range",
                self.quality
            )));
        }
        if self.colour == Colour::BlackAndWhite && !self.format.takes_black_and_white() {
            return Err(Error::InvalidArgument(format!(
                "{} doesn't take black and white",
                self.format.name()
            )));
        }
        Ok(())
    }

    /// The size in pixels of page `page`, `width` x `height` points (as it shows, turned
    /// and cropped), unless the format or [`MAX_PIXELS`] can't take it.
    pub fn pixel_size(&self, page: usize, width: f32, height: f32) -> Result<(u32, u32)> {
        let (w, h) = crate::render::pixel_size(width, height, f32::from(self.dpi) / 72.0);
        let max = self.format.max_side();
        if w > max || h > max || u64::from(w) * u64::from(h) > MAX_PIXELS {
            return Err(Error::PageTooLarge {
                page,
                width: w,
                height: h,
                format: self.format.short_name(),
            });
        }
        Ok((w, h))
    }
}

/// A page's pixels, ready to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// 1 (grey, or black and white) or 3 (RGB).
    pub channels: u8,
    /// `height` rows of `width * channels` bytes; black and white is 0 or 255.
    pub data: Vec<u8>,
    pub dpi: u16,
    /// Black and white: every sample is 0 or 255.
    pub mono: bool,
}

impl Raster {
    /// One bit a pixel, rows padded to whole bytes, the first pixel in the high bit, 0 for
    /// black: as PNG's one-bit grey and CCITT Group 4 take them.
    pub fn bits(&self) -> Vec<u8> {
        pixels::invert(&pixels::pack_black(
            &self.data,
            self.width as usize,
            self.height as usize,
            127,
        ))
    }
}

/// Draws `list`, page `page`'s, at `options`' resolution and in its colours, on white.
pub fn render(list: &DisplayList, page: usize, options: &Options) -> Result<Raster> {
    options.check()?;
    let bounds = list.bounds();
    // Refused before any pixel is drawn; MuPDF rounds the size as `pixel_size` does.
    options.pixel_size(page, bounds.width(), bounds.height())?;
    let scale = f32::from(options.dpi) / 72.0;
    let grey = options.colour != Colour::Colour;
    let colorspace = if grey {
        Colorspace::device_gray()
    } else {
        Colorspace::device_rgb()
    };
    // Without alpha, MuPDF draws on white.
    let pixmap = list.to_pixmap(&Matrix::new_scale(scale, scale), &colorspace, false)?;
    let channels: u8 = if grey { 1 } else { 3 };
    let (pw, ph) = (pixmap.width(), pixmap.height());
    let stride = usize::try_from(pixmap.stride())
        .map_err(|_| Error::InvalidArgument("negative pixmap stride".into()))?;
    let row = pw as usize * usize::from(channels);
    let samples = pixmap.samples();
    let mut data = if stride == row {
        samples[..row * ph as usize].to_vec()
    } else {
        samples
            .chunks(stride)
            .take(ph as usize)
            .flat_map(|r| &r[..row])
            .copied()
            .collect()
    };
    drop(pixmap);
    let mono = options.colour == Colour::BlackAndWhite;
    if mono {
        let threshold = pixels::otsu(&data);
        for v in &mut data {
            *v = if *v <= threshold { 0 } else { 255 };
        }
    }
    Ok(Raster {
        width: pw,
        height: ph,
        channels,
        data,
        dpi: options.dpi,
        mono,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};

    pub(crate) fn options(format: Format) -> Options {
        Options {
            format,
            dpi: 150,
            colour: Colour::Colour,
            quality: 85,
            tiff: TiffCompression::Lzw,
            annotations: true,
        }
    }

    #[test]
    fn a_page_has_the_pixels_its_resolution_gives() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = crate::render::display_list(&doc, 0).unwrap();
        let b = list.bounds();
        for dpi in [72u16, 150, 300] {
            let o = Options {
                dpi,
                ..options(Format::Png)
            };
            let r = render(&list, 0, &o).unwrap();
            let k = f32::from(dpi) / 72.0;
            assert!(
                (r.width as f32 - b.width() * k).abs() <= 1.0,
                "{dpi}: {}",
                r.width
            );
            assert!(
                (r.height as f32 - b.height() * k).abs() <= 1.0,
                "{dpi}: {}",
                r.height
            );
            assert_eq!(r.data.len(), (r.width * r.height * 3) as usize);
            assert_eq!(r.dpi, dpi);
        }
    }

    #[test]
    fn grey_and_black_and_white_have_one_channel() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = crate::render::display_list(&doc, 0).unwrap();
        let grey = render(
            &list,
            0,
            &Options {
                colour: Colour::Grey,
                ..options(Format::Png)
            },
        )
        .unwrap();
        assert_eq!(grey.channels, 1);
        assert!(
            grey.data.iter().any(|&v| v != 0 && v != 255),
            "anti-aliased"
        );
        let mono = render(
            &list,
            0,
            &Options {
                colour: Colour::BlackAndWhite,
                ..options(Format::Png)
            },
        )
        .unwrap();
        assert!(mono.mono);
        assert!(mono.data.iter().all(|&v| v == 0 || v == 255));
        let black = mono.data.iter().filter(|&&v| v == 0).count();
        assert!(black > 100 && black < mono.data.len() / 4, "{black}");
        // Bits: 0 is black, as many as there are black pixels.
        let bits = mono.bits();
        let zeros: usize = bits
            .chunks(mono.width.div_ceil(8) as usize)
            .map(|row| {
                (0..mono.width as usize)
                    .filter(|&x| row[x / 8] & (0x80 >> (x % 8)) == 0)
                    .count()
            })
            .sum();
        assert_eq!(zeros, black);
    }

    #[test]
    fn options_that_dont_go_together() {
        let mut o = options(Format::Jpeg);
        o.colour = Colour::BlackAndWhite;
        assert!(o.check().is_err(), "JPEG in black and white");
        o.colour = Colour::Grey;
        assert!(o.check().is_ok());
        o.dpi = 9;
        assert!(o.check().is_err());
        o.dpi = 2400;
        o.quality = 0;
        assert!(o.check().is_err());
    }

    #[test]
    fn pages_too_large_for_the_format_are_refused() {
        // A4 at 2,400 dpi: 19,843 x 28,063 pixels, beyond every limit.
        let o = Options {
            dpi: 2400,
            ..options(Format::Png)
        };
        assert!(o.pixel_size(0, 595.0, 842.0).is_err());
        // JPEG stops at 65,535 pixels a side: a page 9 m tall at 300 dpi.
        let tall = Options {
            dpi: 300,
            ..options(Format::Jpeg)
        };
        assert!(tall.pixel_size(0, 595.0, 842.0).is_ok());
        assert!(tall.pixel_size(0, 72.0, 16_000.0).is_err());
        assert!(
            Options {
                format: Format::Tiff,
                ..tall
            }
            .pixel_size(0, 72.0, 16_000.0)
            .is_ok()
        );
    }
}
