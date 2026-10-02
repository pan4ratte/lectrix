//! Page grafting with one shared graft map per source document.

use std::ffi::c_int;

use mupdf::Context;
use mupdf::pdf::PdfDocument;
use mupdf_sys::{fz_context, pdf_document};

use super::{FolioError, check};
use crate::error::{Error, Result};

unsafe extern "C" {
    fn folio_pdf_graft_pages(
        ctx: *mut fz_context,
        dst: *mut pdf_document,
        insert_at: c_int,
        src: *mut pdf_document,
        pages: *const c_int,
        count: c_int,
        err: *mut FolioError,
    ) -> c_int;
}

/// Copies `pages` of `src` into `dst` at `insert_at` (`None` appends), sharing resources
/// between the copied pages. Both documents must have been opened on this thread.
pub fn graft_pages(
    dst: &mut PdfDocument,
    insert_at: Option<usize>,
    src: &PdfDocument,
    pages: &[usize],
) -> Result<()> {
    let src_count = usize::try_from(src.page_count()?).unwrap_or(0);
    let mut indices = Vec::with_capacity(pages.len());
    for &p in pages {
        if p >= src_count {
            return Err(Error::PageOutOfRange(p));
        }
        indices.push(c_int::try_from(p).map_err(|_| Error::PageOutOfRange(p))?);
    }
    let insert_at = match insert_at {
        Some(i) => c_int::try_from(i).map_err(|_| Error::PageOutOfRange(i))?,
        None => -1,
    };
    let count = c_int::try_from(indices.len())
        .map_err(|_| Error::InvalidArgument("too many pages".into()))?;
    let ctx = Context::get().as_raw_ptr();
    let mut err = FolioError::new();
    // SAFETY: `ctx` is this thread's context; both documents are alive for the call and
    // belong to this thread's context family (they are not Send, so they were opened
    // here); `indices` holds `count` valid page indices of `src`; `err` is a live local.
    let rc = unsafe {
        folio_pdf_graft_pages(
            ctx,
            dst.as_raw_pdf_ptr(),
            insert_at,
            src.as_raw_pdf_ptr(),
            indices.as_ptr(),
            count,
            &mut err,
        )
    };
    check(rc, err)
}
