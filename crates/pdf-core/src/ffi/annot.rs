//! Annotation calls the crate does not wrap.

use std::ffi::c_int;

use mupdf::pdf::PdfAnnotation;
use mupdf::{Context, Device};
use mupdf_sys::{fz_context, fz_device, pdf_annot};

use super::{LectrixError, check};
use crate::error::Result;

unsafe extern "C" {
    fn lectrix_pdf_dirty_annot(
        ctx: *mut fz_context,
        annot: *mut pdf_annot,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_run_annot(
        ctx: *mut fz_context,
        annot: *mut pdf_annot,
        dev: *mut fz_device,
        err: *mut LectrixError,
    ) -> c_int;
}

/// Makes the next `update()` write a new appearance stream into the document. Without
/// this, MuPDF gives an annotation that has no appearance a local one for display only.
pub fn request_appearance(annot: &mut PdfAnnotation) -> Result<()> {
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; `annot` is a live annotation of a document
    // owned by this thread (PdfAnnotation is not Send) and is only borrowed for the call;
    // `err` is a live local.
    let rc = unsafe { lectrix_pdf_dirty_annot(ctx, annot.as_raw_ptr(), &mut err) };
    check(rc, err)
}

/// Draws one annotation on `device` in page space, as MuPDF draws each annotation of a page
/// it runs. The crate only runs all of a page's annotations at once.
pub fn run_annotation(annot: &PdfAnnotation, device: &Device) -> Result<()> {
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; `annot` is a live annotation of a document
    // owned by this thread (PdfAnnotation is not Send), and `device` a live device made on
    // it; both are only borrowed for the call. `err` is a live local.
    let rc =
        unsafe { lectrix_pdf_run_annot(ctx, annot.as_raw_ptr(), device.as_raw_ptr(), &mut err) };
    check(rc, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};
    use mupdf::pdf::{PdfAnnotationType, PdfObject};
    use mupdf::{Colorspace, DisplayList, Matrix, Rect};

    #[test]
    fn one_annotation_is_drawn_on_a_device() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let mut page = doc.load_pdf_page(0).unwrap();
        let mut annot = page.create_annotation(PdfAnnotationType::Square).unwrap();
        annot
            .set_rect(Rect::new(100.0, 100.0, 200.0, 200.0))
            .unwrap();
        annot.update().unwrap();

        let mut list = DisplayList::new(page.bounds().unwrap()).unwrap();
        {
            let device = Device::from_display_list(&mut list).unwrap();
            run_annotation(&annot, &device).unwrap();
        }
        // Nothing but the square's outline was drawn: transparent away from it.
        let pixmap = list
            .to_pixmap(&Matrix::IDENTITY, &Colorspace::device_rgb(), true)
            .unwrap();
        let alpha = |x: usize, y: usize| {
            let n = pixmap.n() as usize;
            pixmap.samples()[y * pixmap.stride() as usize + x * n + n - 1]
        };
        assert_eq!(alpha(50, 50), 0);
        assert!((95..106).any(|x| alpha(x, 150) > 0), "no outline drawn");
    }

    #[test]
    fn requested_appearances_are_written_to_the_document() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let mut page = doc.load_pdf_page(0).unwrap();
        let mut annot = page.create_annotation(PdfAnnotationType::Square).unwrap();
        annot.update().unwrap();
        // Drop the appearance, as files from other apps sometimes have none.
        annot.object().dict_delete("AP").unwrap();
        annot.update().unwrap();
        let ap = |a: &PdfAnnotation| a.object().get_dict("AP").unwrap().is_some();
        assert!(
            !ap(&annot),
            "without a request, MuPDF synthesizes for display only"
        );

        request_appearance(&mut annot).unwrap();
        annot.update().unwrap();
        assert!(ap(&annot));
        let n: Option<PdfObject> = annot
            .object()
            .get_dict("AP")
            .unwrap()
            .and_then(|ap| ap.get_dict("N").unwrap());
        assert!(n.unwrap().is_stream().unwrap());
    }
}
