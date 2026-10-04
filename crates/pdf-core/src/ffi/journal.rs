//! MuPDF journalling (undo/redo). The crate exposes `begin_operation`/`end_operation` but
//! not enabling the journal or moving through it.

use std::ffi::{CStr, c_char, c_int};

use mupdf::Context;
use mupdf::pdf::PdfDocument;
use mupdf_sys::{fz_context, pdf_document};

use super::{LectrixError, check};
use crate::error::Result;

unsafe extern "C" {
    fn lectrix_pdf_enable_journal(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_begin_implicit_operation(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_undo(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_redo(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_undoredo_state(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        current: *mut c_int,
        steps: *mut c_int,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_undoredo_step(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        step: c_int,
        buf: *mut c_char,
        len: usize,
        err: *mut LectrixError,
    ) -> c_int;
}

/// Position in the undo history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalState {
    /// Steps applied: 0 is the document as opened.
    pub current: usize,
    /// Total steps in the history (`current < steps` means redo is possible).
    pub steps: usize,
    /// Name of the step `undo` would revert.
    pub undo_name: Option<String>,
    /// Name of the step `redo` would re-apply.
    pub redo_name: Option<String>,
}

/// Journalling calls on one document. Borrowing the document mutably keeps the raw
/// handle valid and confines use to the document's owning thread (`PdfDocument` is not
/// `Send`).
pub struct Journal<'a> {
    doc: &'a mut PdfDocument,
}

impl<'a> Journal<'a> {
    pub fn new(doc: &'a mut PdfDocument) -> Self {
        Self { doc }
    }

    /// Calls `f` with the thread's context and the raw document plus a fresh error slot.
    fn call(
        &mut self,
        f: impl FnOnce(*mut fz_context, *mut pdf_document, *mut LectrixError) -> c_int,
    ) -> Result<()> {
        let ctx = Context::get().as_raw_ptr();
        let doc = self.doc.as_raw_pdf_ptr();
        let mut err = LectrixError::new();
        let rc = f(ctx, doc, &mut err);
        check(rc, err)
    }

    pub fn enable(&mut self) -> Result<()> {
        // SAFETY: `ctx` is this thread's context, from the same family that opened `doc`;
        // `doc` is alive for the borrow; `err` points to a live LectrixError.
        self.call(|ctx, doc, err| unsafe { lectrix_pdf_enable_journal(ctx, doc, err) })
    }

    /// Starts an unnamed operation (end it with `PdfDocument::end_operation`). Its changes
    /// are not an undo step of their own: MuPDF folds them into the previous step, or
    /// keeps them out of the history when there is none. Used for changes that should be
    /// saved but not undone, such as which bookmarks are expanded.
    pub fn begin_implicit(&mut self) -> Result<()> {
        // SAFETY: as in `enable`.
        self.call(|ctx, doc, err| unsafe { lectrix_pdf_begin_implicit_operation(ctx, doc, err) })
    }

    pub fn undo(&mut self) -> Result<()> {
        // SAFETY: as in `enable`.
        self.call(|ctx, doc, err| unsafe { lectrix_pdf_undo(ctx, doc, err) })
    }

    pub fn redo(&mut self) -> Result<()> {
        // SAFETY: as in `enable`.
        self.call(|ctx, doc, err| unsafe { lectrix_pdf_redo(ctx, doc, err) })
    }

    pub fn state(&mut self) -> Result<JournalState> {
        let (mut current, mut steps): (c_int, c_int) = (0, 0);
        // SAFETY: as in `enable`; `current` and `steps` are live locals.
        self.call(|ctx, doc, err| unsafe {
            lectrix_pdf_undoredo_state(ctx, doc, &mut current, &mut steps, err)
        })?;
        let current = usize::try_from(current).unwrap_or(0);
        let steps = usize::try_from(steps).unwrap_or(0);
        let undo_name = if current > 0 {
            Some(self.step_name(current - 1)?)
        } else {
            None
        };
        let redo_name = if current < steps {
            Some(self.step_name(current)?)
        } else {
            None
        };
        Ok(JournalState {
            current,
            steps,
            undo_name,
            redo_name,
        })
    }

    fn step_name(&mut self, step: usize) -> Result<String> {
        let mut buf = [0 as c_char; 256];
        let step = c_int::try_from(step).unwrap_or(c_int::MAX);
        // SAFETY: as in `enable`; `buf` is a live buffer of the length passed, and the shim
        // NUL-terminates within it.
        self.call(|ctx, doc, err| unsafe {
            lectrix_pdf_undoredo_step(ctx, doc, step, buf.as_mut_ptr(), buf.len(), err)
        })?;
        // SAFETY: NUL-terminated by the shim (fz_strlcpy with len = buf.len()).
        Ok(unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }
}
