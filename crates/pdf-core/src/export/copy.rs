//! A page, or an area of it, copied to the clipboard as an image (section 6.14): drawn as it
//! is exported, in colour, on white, as the RGBA pixels a clipboard takes.

use mupdf::DisplayList;

use super::{Area, Colour, DPI_RANGE, Format, Options, TiffCompression, render};
use crate::error::{Error, Result};

/// The most pixels a page is copied with: A4 and Letter pages fit at 600 dpi (35 and 34
/// million). The clipboard takes them uncompressed, four bytes a pixel, and on Windows
/// as a PNG and a bitmap besides; larger pages are copied at a lower resolution.
pub const MAX_PIXELS: u64 = 36_000_000;

/// A page drawn for the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub width: u32,
    pub height: u32,
    /// The resolution it was drawn at: the one asked for, or less for a large page.
    pub dpi: u16,
    /// `height` rows of `width` pixels, red, green, blue and alpha (always opaque).
    pub rgba: Vec<u8>,
}

/// The highest resolution, at most `dpi`, at which a page `width` x `height` points has at
/// most `max_pixels` pixels (and never less than the lowest resolution exported).
pub fn fit_dpi(dpi: u16, width: f32, height: f32, max_pixels: u64) -> u16 {
    let area = f64::from(width.max(1.0)) * f64::from(height.max(1.0)) / (72.0 * 72.0);
    let most = (max_pixels as f64 / area).sqrt().floor();
    let most = most.clamp(f64::from(DPI_RANGE.0), f64::from(u16::MAX)) as u16;
    dpi.min(most)
}

/// Draws `list`, page `page`'s, or its `area`, at `dpi` (less if it would have more than
/// [`MAX_PIXELS`] pixels), as it shows on screen, for the clipboard.
pub fn copy_image(list: &DisplayList, page: usize, dpi: u16, area: Option<Area>) -> Result<Copied> {
    let bounds = list.bounds();
    let mut options = Options {
        format: Format::Png,
        dpi,
        colour: Colour::Colour,
        quality: 100,
        tiff: TiffCompression::Lzw,
        annotations: true,
        area,
    };
    let (width, height) = options.extent(bounds.width(), bounds.height());
    options.dpi = fit_dpi(dpi, width, height, MAX_PIXELS);
    let (w, h) = options.pixel_size(page, bounds.width(), bounds.height())?;
    // A page so large it doesn't fit even at the lowest resolution.
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(Error::PageTooLarge {
            page,
            width: w,
            height: h,
            format: "the clipboard",
        });
    }
    let raster = render(list, page, &options)?;
    let mut rgba = Vec::with_capacity(raster.data.len() / 3 * 4);
    for px in raster.data.as_chunks::<3>().0 {
        rgba.extend_from_slice(px);
        rgba.push(255);
    }
    Ok(Copied {
        width: raster.width,
        height: raster.height,
        dpi: options.dpi,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};

    #[test]
    fn a_page_is_copied_as_opaque_rgba_at_its_resolution() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = crate::render::display_list(&doc, 0).unwrap();
        let b = list.bounds();
        let copied = copy_image(&list, 0, 150, None).unwrap();
        assert_eq!(copied.dpi, 150);
        let k = 150.0 / 72.0;
        assert!((copied.width as f32 - b.width() * k).abs() <= 1.0);
        assert!((copied.height as f32 - b.height() * k).abs() <= 1.0);
        assert_eq!(
            copied.rgba.len(),
            (copied.width * copied.height * 4) as usize
        );
        assert!(copied.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        // Drawn on white, with text on it.
        assert_eq!(&copied.rgba[..4], &[255, 255, 255, 255]);
        assert!(copied.rgba.as_chunks::<4>().0.iter().any(|p| p[0] < 128));
    }

    #[test]
    fn an_area_is_copied_alone() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = crate::render::display_list(&doc, 0).unwrap();
        let area = Area {
            x0: 72.0,
            y0: 72.0,
            x1: 216.0,
            y1: 108.0,
            turn: 90,
        };
        let copied = copy_image(&list, 0, 144, Some(area)).unwrap();
        // 144 by 36 points at 2 pixels a point, turned.
        assert_eq!((copied.width, copied.height), (72, 288));
        assert_eq!(copied.dpi, 144);
        assert_eq!(copied.rgba.len(), 72 * 288 * 4);
    }

    #[test]
    fn large_pages_are_copied_at_a_lower_resolution() {
        // Letter at 600 dpi: 33.7 million pixels, as asked.
        assert_eq!(fit_dpi(600, 612.0, 792.0, MAX_PIXELS), 600);
        // A0 at 300 dpi would be 140 million.
        let dpi = fit_dpi(300, 2384.0, 3370.0, MAX_PIXELS);
        assert!(dpi < 300 && dpi > 140, "{dpi}");
        let px = (2384.0 * f64::from(dpi) / 72.0) * (3370.0 * f64::from(dpi) / 72.0);
        assert!(px <= MAX_PIXELS as f64);
        // Never below the lowest resolution.
        assert_eq!(
            fit_dpi(300, 14_400.0 * 72.0, 14_400.0 * 72.0, MAX_PIXELS),
            DPI_RANGE.0
        );
    }
}
