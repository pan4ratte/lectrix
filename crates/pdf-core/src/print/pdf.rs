//! A print job written as a PDF of its sheets: each sheet a page the size of the paper,
//! with the document's page placed on it as it prints. CUPS (Linux, macOS) prints it; the
//! command-line tool writes it for tests. The pages stay vector: MuPDF's PDF device writes
//! what the page draws. A document that may be printed at low resolution only gets its
//! pages as images at that resolution instead, as on Windows (section 5.4).

use std::path::{Path, PathBuf};

use mupdf::{
    ColorParams, Colorspace, Device, DisplayList, DocumentWriter, IRect, Image, Matrix, Pixmap,
    Rect,
};

use super::{Area, Sheet, Target};
use crate::error::{Error, Result};

pub struct PdfTarget {
    writer: Option<DocumentWriter>,
    path: PathBuf,
    sheet: Sheet,
    /// Pages are drawn as images at this resolution (dots per inch), not as vectors.
    raster_dpi: Option<f32>,
    /// Something was drawn on the sheet begun last.
    drawn: bool,
}

impl PdfTarget {
    /// A job written to `path`, on sheets like `sheet`.
    pub fn new(path: &Path, sheet: Sheet) -> Result<PdfTarget> {
        let name = path
            .to_str()
            .ok_or_else(|| Error::InvalidArgument("the print file's path is not UTF-8".into()))?;
        Ok(PdfTarget {
            writer: Some(DocumentWriter::new(name, "pdf", "compress")?),
            path: path.to_path_buf(),
            sheet,
            raster_dpi: None,
            drawn: false,
        })
    }

    /// The pages drawn as images at `dpi`, for a document that allows no more.
    pub fn rasterized(mut self, dpi: f32) -> PdfTarget {
        self.raster_dpi = Some(dpi);
        self
    }

    fn media(&self) -> Rect {
        Rect::new(0.0, 0.0, self.sheet.width, self.sheet.height)
    }

    fn writer(&mut self) -> Result<&mut DocumentWriter> {
        self.writer
            .as_mut()
            .ok_or_else(|| Error::InvalidArgument("the print file is closed".into()))
    }
}

/// `list` drawn through `ctm` within `clip` (sheet points) at `dpi`, on white; `None` for
/// a clip with no area.
fn rasterize(list: &DisplayList, ctm: &Matrix, clip: Area, dpi: f32) -> Result<Option<Image>> {
    let scale = dpi / 72.0;
    let size = |v: f32| -> Result<i32> {
        let v = (v * scale).ceil();
        if v.is_finite() && v < 1.0e5 {
            Ok(v as i32)
        } else {
            Err(Error::InvalidArgument(
                "the page is too large to print".into(),
            ))
        }
    };
    let (width, height) = (size(clip.width)?, size(clip.height)?);
    if width <= 0 || height <= 0 {
        return Ok(None);
    }
    let mut pixmap = Pixmap::new(&Colorspace::device_rgb(), 0, 0, width, height, false)?;
    pixmap.clear_with(255)?;
    // Page space to the pixmap's pixels: the sheet's points, less the clip's corner.
    let m = Matrix::new(
        ctm.a * scale,
        ctm.b * scale,
        ctm.c * scale,
        ctm.d * scale,
        (ctm.e - clip.x) * scale,
        (ctm.f - clip.y) * scale,
    );
    {
        let device = Device::from_pixmap_with_clip(&pixmap, IRect::new(0, 0, width, height))?;
        list.run(
            &device,
            &m,
            Rect::new(0.0, 0.0, width as f32, height as f32),
        )?;
        // Dropping the device closes it, which flushes pending drawing into the pixmap.
    }
    Ok(Some(Image::from_pixmap(&pixmap)?))
}

impl Target for PdfTarget {
    fn sheet(&self) -> Sheet {
        self.sheet
    }

    fn makes_copies(&self) -> bool {
        // The printer (CUPS) is asked for them.
        true
    }

    fn begin_sheet(&mut self) -> Result<()> {
        self.drawn = false;
        Ok(())
    }

    fn draw(&mut self, list: &DisplayList, ctm: &Matrix, clip: Area, _grey: bool) -> Result<()> {
        // Greys are asked of the printer: the PDF keeps the page's colours.
        let media = self.media();
        let image = match self.raster_dpi {
            Some(dpi) => rasterize(list, ctm, clip, dpi)?,
            None => None,
        };
        let raster = self.raster_dpi.is_some();
        let page = self.writer()?.begin_page(media)?;
        match image {
            // The image fills the clip: its unit square scaled to it.
            Some(image) => page.fill_image(
                &image,
                &Matrix::new(clip.width, 0.0, 0.0, clip.height, clip.x, clip.y),
                1.0,
                ColorParams::default(),
            )?,
            None if raster => {}
            None => list.run(
                &page,
                ctm,
                Rect::new(clip.x, clip.y, clip.right(), clip.bottom()),
            )?,
        }
        page.end()?;
        self.drawn = true;
        Ok(())
    }

    fn end_sheet(&mut self) -> Result<()> {
        if !self.drawn {
            // A blank sheet.
            let media = self.media();
            self.writer()?.begin_page(media)?.end()?;
        }
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        // Closing the writer writes the file (its errors cannot be read: the size tells).
        drop(self.writer.take());
        match std::fs::metadata(&self.path) {
            Ok(m) if m.len() > 0 => Ok(()),
            _ => Err(Error::InvalidArgument(
                "the print file was not written".into(),
            )),
        }
    }

    fn abort(mut self: Box<Self>) {
        drop(self.writer.take());
        let _ = std::fs::remove_file(&self.path);
    }
}
