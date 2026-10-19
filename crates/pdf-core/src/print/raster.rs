//! Pages drawn as pixels for a printer that takes images (Windows' print spooler), a band
//! of rows at a time, so a sheet at 600 dpi never needs the 100 MB a whole one would.

use mupdf::{Colorspace, Device, DisplayList, IRect, Matrix, Pixmap, Rect};

use super::Area;
use crate::error::{Error, Result};

/// Bytes of pixels drawn at a time.
pub const BAND_BYTES: usize = 16 << 20;

/// How sheet points become a printer's pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolution {
    /// The printer's pixels per point, across and down.
    pub device_x: f32,
    pub device_y: f32,
    /// Pixels drawn per point (at most the printer's): the printer enlarges them.
    pub drawn_x: f32,
    pub drawn_y: f32,
    /// Where the printer's pixel (0, 0) is on the sheet, in points: the printable area's
    /// top-left corner.
    pub origin_x: f32,
    pub origin_y: f32,
}

impl Resolution {
    /// A printer of `dpi_x` x `dpi_y` whose pixels start at `origin` (points), drawn at no
    /// more than `max_dpi`.
    pub fn new(dpi_x: f32, dpi_y: f32, origin: (f32, f32), max_dpi: f32) -> Resolution {
        Resolution {
            device_x: dpi_x / 72.0,
            device_y: dpi_y / 72.0,
            drawn_x: dpi_x.min(max_dpi) / 72.0,
            drawn_y: dpi_y.min(max_dpi) / 72.0,
            origin_x: origin.0,
            origin_y: origin.1,
        }
    }
}

/// A band of a sheet's pixels and where it goes.
#[derive(Debug)]
pub struct Band<'a> {
    /// Where the band goes, in the printer's pixels: x, y, width, height.
    pub dest: (i32, i32, i32, i32),
    pub width: u32,
    pub height: u32,
    /// Bytes a row: a multiple of 4, as Windows' bitmaps have them.
    pub stride: usize,
    /// Blue, green, red a pixel; or one grey byte a pixel.
    pub samples: &'a [u8],
    pub grey: bool,
}

/// Draws `list` through `ctm` (page space to sheet points), within `clip` (sheet points),
/// in bands of at most `band_bytes`, each handed to `out`.
pub fn draw_bands(
    list: &DisplayList,
    ctm: &Matrix,
    clip: Area,
    grey: bool,
    resolution: &Resolution,
    band_bytes: usize,
    out: &mut dyn FnMut(&Band) -> Result<()>,
) -> Result<()> {
    let r = resolution;
    let px = |v: f32| -> Result<i32> {
        if v.is_finite() && v.abs() < 1.0e7 {
            Ok(v as i32)
        } else {
            Err(Error::InvalidArgument(
                "the page is too large to print".into(),
            ))
        }
    };
    let x0 = px(((clip.x - r.origin_x) * r.drawn_x).floor())?.max(0);
    let y0 = px(((clip.y - r.origin_y) * r.drawn_y).floor())?.max(0);
    let x1 = px(((clip.right() - r.origin_x) * r.drawn_x).ceil())?;
    let y1 = px(((clip.bottom() - r.origin_y) * r.drawn_y).ceil())?;
    if x1 <= x0 || y1 <= y0 {
        return Ok(());
    }
    // A width whose rows are whole multiples of 4 bytes, grey or not; the columns added
    // stay white.
    let width = (x1 - x0 + 3) / 4 * 4;
    let channels = if grey { 1 } else { 3 };
    let stride = width as usize * channels;
    let rows = i32::try_from((band_bytes / stride).max(1)).unwrap_or(i32::MAX);
    let colorspace = if grey {
        Colorspace::device_gray()
    } else {
        Colorspace::device_bgr()
    };
    // Page space to drawn pixels: the sheet's points, less the origin, at the resolution.
    let m = Matrix::new(
        ctm.a * r.drawn_x,
        ctm.b * r.drawn_y,
        ctm.c * r.drawn_x,
        ctm.d * r.drawn_y,
        (ctm.e - r.origin_x) * r.drawn_x,
        (ctm.f - r.origin_y) * r.drawn_y,
    );
    let (fx, fy) = (r.device_x / r.drawn_x, r.device_y / r.drawn_y);
    let mut top = y0;
    while top < y1 {
        let bottom = (top + rows).min(y1);
        let height = bottom - top;
        let mut pixmap = Pixmap::new(&colorspace, x0, top, width, height, false)?;
        pixmap.clear_with(255)?;
        {
            let device = Device::from_pixmap_with_clip(&pixmap, IRect::new(x0, top, x1, bottom))?;
            list.run(
                &device,
                &m,
                Rect::new(x0 as f32, top as f32, x1 as f32, bottom as f32),
            )?;
            // Dropping the device closes it, which flushes pending drawing into the pixmap.
        }
        let stride = usize::try_from(pixmap.stride())
            .map_err(|_| Error::InvalidArgument("negative pixmap stride".into()))?;
        // The edges of the band in the printer's pixels, rounded the same way for every
        // band so that they meet.
        let scale = |v: i32, f: f32| (v as f32 * f).round() as i32;
        let (dx0, dx1) = (scale(x0, fx), scale(x0 + width, fx));
        let (dy0, dy1) = (scale(top, fy), scale(bottom, fy));
        out(&Band {
            dest: (dx0, dy0, dx1 - dx0, dy1 - dy0),
            width: width as u32,
            height: height as u32,
            stride,
            samples: pixmap.samples(),
            grey,
        })?;
        top = bottom;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::{Orientation, Sheet, Sizing, page_to_sheet, place};
    use crate::render;
    use crate::testgen::{SampleSpec, sample_document};

    /// Each band's place, the sheet's pixels put together from the bands, and the bytes a
    /// row.
    type Assembled = (Vec<(i32, i32, i32, i32)>, Vec<u8>, usize);

    fn assemble(
        list: &DisplayList,
        sheet: &Sheet,
        grey: bool,
        resolution: &Resolution,
        band_bytes: usize,
    ) -> Assembled {
        let bounds = list.bounds();
        let placement = place(
            bounds.width(),
            bounds.height(),
            0,
            sheet,
            Sizing::Fit,
            Orientation::Auto,
        );
        let ctm = page_to_sheet(bounds, &placement);
        let clip = placement.area.intersect(&sheet.printable);
        let mut dests = Vec::new();
        let mut pixels = Vec::new();
        let mut row_bytes = 0;
        draw_bands(
            list,
            &ctm,
            clip,
            grey,
            resolution,
            band_bytes,
            &mut |band| {
                assert_eq!(band.stride % 4, 0);
                row_bytes = band.stride;
                dests.push(band.dest);
                pixels.extend_from_slice(&band.samples[..band.stride * band.height as usize]);
                Ok(())
            },
        )
        .unwrap();
        (dests, pixels, row_bytes)
    }

    #[test]
    fn bands_cover_the_page_without_gaps_and_draw_it() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = render::display_list(&doc, 0).unwrap();
        let sheet = Sheet::with_margin(595.0, 842.0, 18.0);
        let resolution = Resolution::new(300.0, 300.0, (18.0, 18.0), 600.0);
        // Small bands, so there are many.
        let (dests, pixels, row_bytes) = assemble(&list, &sheet, true, &resolution, 64 << 10);
        assert!(dests.len() > 10, "{} bands", dests.len());
        for pair in dests.windows(2) {
            assert_eq!(
                pair[0].1 + pair[0].3,
                pair[1].1,
                "bands do not meet: {pair:?}"
            );
            assert_eq!(pair[0].0, pair[1].0);
        }
        // The sample's text is there: some dark pixels, most of them white.
        let dark = pixels.iter().filter(|&&v| v < 128).count();
        assert!(
            dark > 100 && dark < pixels.len() / 4,
            "{dark} dark of {}",
            pixels.len()
        );
        // Fitted into the printable area: no wider than it is at 300 dpi.
        let printable_px = ((595.0 - 36.0) * 300.0 / 72.0) as usize;
        assert!(
            row_bytes > printable_px / 2 && row_bytes <= printable_px + 8,
            "{row_bytes}"
        );
    }

    #[test]
    fn a_lower_resolution_is_enlarged_to_the_printers() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = render::display_list(&doc, 0).unwrap();
        let sheet = Sheet::with_margin(595.0, 842.0, 0.0);
        let resolution = Resolution::new(600.0, 600.0, (0.0, 0.0), 150.0);
        let (dests, pixels, row_bytes) = assemble(&list, &sheet, false, &resolution, BAND_BYTES);
        let rows = pixels.len() / row_bytes;
        let total: i32 = dests.iter().map(|d| d.3).sum();
        // Drawn at 150 dpi, placed at 600: four printer pixels a pixel drawn.
        assert!((total - rows as i32 * 4).abs() <= 1, "{total} vs {rows}");
        assert!(dests[0].2 >= (row_bytes / 3) as i32 * 4 - 1);
    }
}
