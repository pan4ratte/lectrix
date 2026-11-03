//! Pages exported as images (section 6.14, ADR 0034): each page drawn as it shows, at a
//! resolution, in colour, greys or black and white, and written as PNG, JPEG or TIFF (a
//! file a page, or one file of all of them).
//!
//! [`render`] draws a page, [`encode`] writes it in a format, [`names`] names the files,
//! and [`run`] exports a job's pages on worker threads, into temporary files that take
//! their names only once every page is written. [`copy`] draws a page for the clipboard.
//! An [`Area`] of a page, framed with the viewer's area tool, is exported and copied the
//! same way, alone.

pub mod copy;
pub mod embedded;
pub mod encode;
pub mod names;
mod run;

pub use run::{Job, Output, run};

use mupdf::{Colorspace, Device, DisplayList, Matrix, Pixmap, Rect};

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

/// A part of a page, as the viewer's area tool frames it (section 6.14): a rectangle in the
/// page's points as it shows, its origin at the page's top-left corner (as the page's
/// display list has it), and the quarter turn the viewer shows the page with (section 6.1),
/// which the image takes too, so it looks as it was framed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// 0, 90, 180 or 270 degrees, clockwise.
    pub turn: u16,
}

impl Area {
    /// Whether it is a rectangle and a quarter turn; why not if it isn't.
    pub fn check(&self) -> Result<()> {
        let finite = [self.x0, self.y0, self.x1, self.y1]
            .iter()
            .all(|v| v.is_finite());
        if !finite || self.x1 <= self.x0 || self.y1 <= self.y0 {
            return Err(Error::InvalidArgument("the area is empty".into()));
        }
        if !matches!(self.turn, 0 | 90 | 180 | 270) {
            return Err(Error::InvalidArgument(format!(
                "a turn of {} degrees is not a quarter turn",
                self.turn
            )));
        }
        Ok(())
    }

    /// The part of it on a page of `bounds`, or none.
    fn within(&self, bounds: Rect) -> Option<Area> {
        let a = Area {
            x0: self.x0.max(bounds.x0),
            y0: self.y0.max(bounds.y0),
            x1: self.x1.min(bounds.x1),
            y1: self.y1.min(bounds.y1),
            turn: self.turn,
        };
        (a.x1 > a.x0 && a.y1 > a.y0).then_some(a)
    }

    /// Its width and height as the image shows it (turned), points.
    pub fn shown_size(&self) -> (f32, f32) {
        let (w, h) = (self.x1 - self.x0, self.y1 - self.y0);
        if self.turn % 180 == 90 {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// The matrix drawing the page at `scale` pixels a point, turned, with the area's
    /// top-left corner as the image shows it at (0, 0).
    fn matrix(&self, scale: f32) -> Matrix {
        let s = scale;
        // x' = a x + c y + e, y' = b x + d y + f: turned clockwise as the page shows (y down).
        let (a, b, c, d) = match self.turn {
            90 => (0.0, s, -s, 0.0),
            180 => (-s, 0.0, 0.0, -s),
            270 => (0.0, -s, s, 0.0),
            _ => (s, 0.0, 0.0, s),
        };
        let corners = [
            (self.x0, self.y0),
            (self.x1, self.y0),
            (self.x0, self.y1),
            (self.x1, self.y1),
        ];
        let (mut e, mut f) = (f32::INFINITY, f32::INFINITY);
        for (x, y) in corners {
            e = e.min(a * x + c * y);
            f = f.min(b * x + d * y);
        }
        Matrix::new(a, b, c, d, -e, -f)
    }
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
    /// Only this part of the page, turned as the viewer shows it; the whole page if none.
    pub area: Option<Area>,
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
        if let Some(area) = &self.area {
            area.check()?;
        }
        Ok(())
    }

    /// What is drawn of a page `width` x `height` points, in points as the image shows it:
    /// the page, or the area.
    pub fn extent(&self, width: f32, height: f32) -> (f32, f32) {
        self.area.map_or((width, height), |a| a.shown_size())
    }

    /// The size in pixels of the image of page `page`, `width` x `height` points (as it
    /// shows, turned and cropped), or of its area, unless the format or [`MAX_PIXELS`]
    /// can't take it.
    pub fn pixel_size(&self, page: usize, width: f32, height: f32) -> Result<(u32, u32)> {
        let (width, height) = self.extent(width, height);
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

/// Draws `list`, page `page`'s, at `options`' resolution and in its colours, on white: the
/// whole page, or its area.
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
    let pixmap = match options.area {
        // Without alpha, MuPDF draws on white.
        None => list.to_pixmap(&Matrix::new_scale(scale, scale), &colorspace, false)?,
        Some(area) => {
            let area = area
                .within(bounds)
                .ok_or_else(|| Error::InvalidArgument("the area is off the page".into()))?;
            let (sw, sh) = area.shown_size();
            let (w, h) = crate::render::pixel_size(sw, sh, scale);
            let to_i32 = |v: u32| {
                i32::try_from(v).map_err(|_| Error::InvalidArgument("image too large".into()))
            };
            let (w, h) = (to_i32(w)?, to_i32(h)?);
            let mut pixmap = Pixmap::new(&colorspace, 0, 0, w, h, false)?;
            pixmap.clear_with(255)?;
            {
                let device = Device::from_pixmap(&pixmap)?;
                let scissor = Rect::new(0.0, 0.0, w as f32, h as f32);
                list.run(&device, &area.matrix(scale), scissor)?;
                // Dropping the device closes it, which flushes its drawing into the pixmap.
            }
            pixmap
        }
    };
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
            area: None,
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
    fn an_area_is_that_part_of_the_page_turned_as_it_shows() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = crate::render::display_list(&doc, 0).unwrap();
        // At 72 dpi a point is a pixel, so the area's pixels are the page's, exactly.
        let o = Options {
            dpi: 72,
            ..options(Format::Png)
        };
        let page = render(&list, 0, &o).unwrap();
        let at = |x: u32, y: u32| {
            let i = ((y * page.width + x) * 3) as usize;
            [page.data[i], page.data[i + 1], page.data[i + 2]]
        };
        // Across the text at the top of the page.
        let (x0, y0, w, h) = (60u32, 60u32, 240u32, 60u32);
        let area = Area {
            x0: x0 as f32,
            y0: y0 as f32,
            x1: (x0 + w) as f32,
            y1: (y0 + h) as f32,
            turn: 0,
        };
        let inked = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| at(x0 + x, y0 + y)[0] < 128)
            .count();
        assert!(inked > 50, "the area has text in it: {inked}");
        for turn in [0u16, 90, 180, 270] {
            let r = render(
                &list,
                0,
                &Options {
                    area: Some(Area { turn, ..area }),
                    ..o
                },
            )
            .unwrap();
            let (rw, rh) = if turn % 180 == 90 { (h, w) } else { (w, h) };
            assert_eq!((r.width, r.height), (rw, rh), "{turn}");
            let mut differ = 0;
            for y in 0..h {
                for x in 0..w {
                    // Where the page's pixel (x, y) of the area lands, turned clockwise.
                    let (tx, ty) = match turn {
                        90 => (h - 1 - y, x),
                        180 => (w - 1 - x, h - 1 - y),
                        270 => (y, w - 1 - x),
                        _ => (x, y),
                    };
                    let i = ((ty * r.width + tx) * 3) as usize;
                    let want = at(x0 + x, y0 + y);
                    if (0..3).any(|c| r.data[i + c].abs_diff(want[c]) > 8) {
                        differ += 1;
                    }
                }
            }
            assert!(
                differ < (w * h / 200) as usize,
                "{turn}: {differ} pixels differ"
            );
        }
        // Its size, whatever the page's.
        assert_eq!(
            Options {
                area: Some(Area { turn: 90, ..area }),
                ..o
            }
            .pixel_size(0, 612.0, 792.0)
            .unwrap(),
            (h, w)
        );
    }

    #[test]
    fn areas_off_the_page_or_empty_are_refused() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = crate::render::display_list(&doc, 0).unwrap();
        let off = Area {
            x0: 5000.0,
            y0: 5000.0,
            x1: 5100.0,
            y1: 5100.0,
            turn: 0,
        };
        let with = |area: Area| Options {
            area: Some(area),
            ..options(Format::Png)
        };
        assert!(render(&list, 0, &with(off)).is_err());
        let empty = Area {
            x1: 10.0,
            ..Area {
                x0: 10.0,
                y0: 10.0,
                x1: 20.0,
                y1: 20.0,
                turn: 0,
            }
        };
        assert!(with(empty).check().is_err());
        assert!(
            with(Area {
                x0: 10.0,
                y0: 10.0,
                x1: 20.0,
                y1: 20.0,
                turn: 45
            })
            .check()
            .is_err()
        );
        // Partly off the page: what is on it.
        let b = list.bounds();
        let r = render(
            &list,
            0,
            &with(Area {
                x0: b.x1 - 50.0,
                y0: 0.0,
                x1: b.x1 + 50.0,
                y1: 100.0,
                turn: 0,
            }),
        )
        .unwrap();
        let k = 150.0 / 72.0;
        assert!((r.width as f32 - 50.0 * k).abs() <= 1.0, "{}", r.width);
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
