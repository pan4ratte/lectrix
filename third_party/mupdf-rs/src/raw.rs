//! FOLIO PATCH: raw-pointer accessors for downstream FFI.
//!
//! Upstream keeps every raw handle `pub(crate)`, which leaves no way to call MuPDF APIs the
//! safe wrapper does not cover yet (for example journalling). These accessors expose the
//! handles without transferring ownership. See `../FOLIO_PATCHES.md`.
//!
//! Every pointer returned here is borrowed: it stays valid only while the wrapper it came
//! from is alive, must be used with a context of the same family (the calling thread's
//! [`Context::get`]), and must never be dropped by the caller.

use mupdf_sys::{fz_context, fz_document, fz_page, pdf_annot, pdf_document, pdf_obj, pdf_page};

use crate::pdf::{PdfAnnotation, PdfDocument, PdfObject, PdfPage};
use crate::{Context, Document, Page};

impl Context {
    /// The calling thread's `fz_context`.
    pub fn as_raw_ptr(&self) -> *mut fz_context {
        self.inner
    }
}

impl Document {
    /// Borrowed `fz_document` handle.
    pub fn as_raw_ptr(&self) -> *mut fz_document {
        self.inner
    }
}

impl PdfDocument {
    /// Borrowed `pdf_document` handle.
    pub fn as_raw_pdf_ptr(&self) -> *mut pdf_document {
        self.as_raw()
    }

    /// Wraps a `pdf_document` that downstream FFI opened (for example with
    /// `pdf_open_document_with_stream`). The wrapper takes over the caller's reference and
    /// drops it when it is dropped.
    ///
    /// # Safety
    ///
    /// `ptr` must be a valid, non-null `pdf_document` carrying one reference owned by the
    /// caller, created with a context of the calling thread's family.
    pub unsafe fn from_raw_owned(ptr: *mut pdf_document) -> Self {
        // SAFETY: guaranteed by the caller, as documented above.
        unsafe { Self::from_raw(ptr) }
    }
}

impl Page {
    /// Borrowed `fz_page` handle.
    pub fn as_raw_ptr(&self) -> *mut fz_page {
        self.inner.as_ptr()
    }
}

impl PdfPage {
    /// Borrowed `pdf_page` handle.
    pub fn as_raw_pdf_ptr(&self) -> *mut pdf_page {
        self.inner.as_ptr()
    }
}

impl PdfAnnotation {
    /// Borrowed `pdf_annot` handle.
    pub fn as_raw_ptr(&self) -> *mut pdf_annot {
        self.inner.as_ptr()
    }
}

impl PdfObject {
    /// Borrowed `pdf_obj` handle.
    pub fn as_raw_ptr(&self) -> *mut pdf_obj {
        self.inner
    }
}
