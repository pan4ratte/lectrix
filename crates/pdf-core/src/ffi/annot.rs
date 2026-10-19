//! Annotation calls the crate does not wrap.

use std::ffi::c_int;

use mupdf::pdf::{PdfAnnotation, PdfPage};
use mupdf::{Context, Device};
use mupdf_sys::{fz_context, fz_device, pdf_annot, pdf_page};

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
    fn lectrix_pdf_run_page_for_print(
        ctx: *mut fz_context,
        page: *mut pdf_page,
        dev: *mut fz_device,
        annots: c_int,
        err: *mut LectrixError,
    ) -> c_int;
}

/// Draws `page` on `device` in page space as it prints: its contents, its annotations
/// (unless `annotations` is false) and its form fields, with MuPDF's "Print" usage, which
/// leaves out annotations without the Print flag and content hidden for printing.
pub fn run_page_for_print(page: &PdfPage, device: &Device, annotations: bool) -> Result<()> {
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; `page` is a live page of a document owned by
    // this thread (PdfPage is not Send), and `device` a live device made on it; both are
    // only borrowed for the call. `err` is a live local.
    let rc = unsafe {
        lectrix_pdf_run_page_for_print(
            ctx,
            page.as_raw_pdf_ptr(),
            device.as_raw_ptr(),
            c_int::from(annotations),
            &mut err,
        )
    };
    check(rc, err)
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
    use mupdf::pdf::{AnnotationFlags, PdfAnnotationType, PdfObject};
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
    fn printing_draws_only_annotations_with_the_print_flag() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let mut page = doc.load_pdf_page(0).unwrap();
        let mut square = |x: f32, flags: AnnotationFlags| {
            let mut annot = page.create_annotation(PdfAnnotationType::Square).unwrap();
            annot
                .set_rect(Rect::new(x, 100.0, x + 100.0, 200.0))
                .unwrap();
            annot.set_flags(flags).unwrap();
            annot.update().unwrap();
        };
        square(100.0, AnnotationFlags::IS_PRINT);
        // Shown on screen only.
        square(300.0, AnnotationFlags::empty());
        let pixels = |annotations: bool| {
            let mut list = DisplayList::new(page.bounds().unwrap()).unwrap();
            {
                let device = Device::from_display_list(&mut list).unwrap();
                run_page_for_print(&page, &device, annotations).unwrap();
            }
            list.to_pixmap(&Matrix::IDENTITY, &Colorspace::device_gray(), false)
                .unwrap()
        };
        let (with, without) = (pixels(true), pixels(false));
        let stride = with.stride() as usize;
        // Pixels that differ along each square's left edge.
        let changed = |x0: usize| {
            (x0 - 3..x0 + 4)
                .flat_map(|x| (110..190).map(move |y| y * stride + x))
                .filter(|&i| with.samples()[i] != without.samples()[i])
                .count()
        };
        assert!(changed(100) > 0, "the printable square is not printed");
        assert_eq!(
            changed(300),
            0,
            "the square without the Print flag is printed"
        );
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
