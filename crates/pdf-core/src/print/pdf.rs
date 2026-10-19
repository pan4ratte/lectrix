//! A print job written as a PDF of its sheets: each sheet a page the size of the paper,
//! with the document's page placed on it as it prints. CUPS (Linux, macOS) prints it; the
//! command-line tool writes it for tests. The pages stay vector: MuPDF's PDF device writes
//! what the page draws.

use std::path::{Path, PathBuf};

use mupdf::{DisplayList, DocumentWriter, Matrix, Rect};

use super::{Area, Sheet, Target};
use crate::error::{Error, Result};

pub struct PdfTarget {
    writer: Option<DocumentWriter>,
    path: PathBuf,
    sheet: Sheet,
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
            drawn: false,
        })
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
        let page = self.writer()?.begin_page(media)?;
        list.run(
            &page,
            ctm,
            Rect::new(clip.x, clip.y, clip.right(), clip.bottom()),
        )?;
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
