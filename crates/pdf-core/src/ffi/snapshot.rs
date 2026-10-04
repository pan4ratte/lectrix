//! Snapshots, for crash recovery (AGENTS.md section 7). The crate does not wrap them.
//!
//! A snapshot is the document's file followed by its unsaved changes as an incremental
//! section, written without finalizing that section in memory: the document carries on as
//! if nothing was written, undo history included. Reopened, a snapshot is an ordinary PDF
//! whose last incremental update holds the changes.
//!
//! MuPDF could also save the undo history (`pdf_save_journal`), but 1.27.2 cannot load such
//! a journal back once it records a change (ADR 0001), so recovery does without it.

use std::ffi::{CString, c_char, c_int};
use std::path::Path;

use mupdf::Context;
use mupdf::pdf::PdfDocument;
use mupdf_sys::{fz_context, pdf_document};

use super::{LectrixError, check};
use crate::error::{Error, Result};

unsafe extern "C" {
    fn lectrix_pdf_save_snapshot(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        path: *const c_char,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_has_unsaved_changes(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        changed: *mut c_int,
        err: *mut LectrixError,
    ) -> c_int;
}

/// Writes `doc` as a snapshot to `path` (replacing it). Fails for documents MuPDF repaired
/// on open, which cannot be written incrementally.
pub fn save_snapshot(doc: &mut PdfDocument, path: &Path) -> Result<()> {
    let s = path.to_str().ok_or_else(|| {
        Error::InvalidArgument(format!("path is not valid Unicode: {}", path.display()))
    })?;
    let path = CString::new(s).map_err(|_| Error::InvalidArgument(format!("NUL in {s}")))?;
    // A snapshot is an incremental update too: keep its /Size true after an undo.
    super::trim_unused_objects(doc);
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context, from the family that opened `doc`; `doc` is
    // mutably borrowed for the call (PdfDocument is not Send, so this is its owning
    // thread); `path` is a live NUL-terminated string; `err` is a live LectrixError.
    let rc =
        unsafe { lectrix_pdf_save_snapshot(ctx, doc.as_raw_pdf_ptr(), path.as_ptr(), &mut err) };
    check(rc, err)
}

/// True if saving `doc` would write changes.
pub fn has_unsaved_changes(doc: &PdfDocument) -> Result<bool> {
    let ctx = Context::get().as_raw_ptr();
    let mut changed: c_int = 0;
    let mut err = LectrixError::new();
    // SAFETY: as in `save_snapshot` (the document is only read); `changed` is a live local.
    let rc = unsafe {
        lectrix_pdf_has_unsaved_changes(ctx, doc.as_raw_pdf_ptr(), &mut changed, &mut err)
    };
    check(rc, err)?;
    Ok(changed != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::{Journal, open_pdf_shared};
    use crate::save::{SaveKind, save_atomic};
    use crate::testgen::{SampleSpec, sample_document};
    use mupdf::pdf::PdfObject;

    fn out_dir(name: &str) -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/pdf-core/ffi-snapshot")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rotation(doc: &PdfDocument, page: i32) -> i32 {
        doc.find_page(page)
            .unwrap()
            .get_dict("Rotate")
            .unwrap()
            .map(|r| r.as_int().unwrap())
            .unwrap_or(0)
    }

    /// Opens a journalled document from a new file and rotates pages 0 and 1 as two steps.
    fn edited(dir: &Path) -> (std::path::PathBuf, PdfDocument) {
        let path = dir.join("original.pdf");
        let doc = sample_document(&SampleSpec {
            pages: 3,
            ..SampleSpec::default()
        })
        .unwrap();
        save_atomic(&doc, SaveKind::Full, None, &path).unwrap();
        let mut doc = open_pdf_shared(&path).unwrap();
        Journal::new(&mut doc).enable().unwrap();
        assert!(!has_unsaved_changes(&doc).unwrap());
        for page in 0..2 {
            doc.begin_operation(&format!("Rotate {page}")).unwrap();
            doc.find_page(page)
                .unwrap()
                .dict_put("Rotate", PdfObject::new_int(90).unwrap())
                .unwrap();
            doc.end_operation().unwrap();
        }
        (path, doc)
    }

    #[test]
    fn a_snapshot_leaves_the_document_and_its_history_as_they_were() {
        let dir = out_dir("unchanged");
        let (_, mut doc) = edited(&dir);
        // One step undone, so there is something to redo.
        Journal::new(&mut doc).undo().unwrap();
        let before = Journal::new(&mut doc).state().unwrap();

        save_snapshot(&mut doc, &dir.join("snap.pdf")).unwrap();
        save_snapshot(&mut doc, &dir.join("snap.pdf")).unwrap();

        assert_eq!(Journal::new(&mut doc).state().unwrap(), before);
        assert!(has_unsaved_changes(&doc).unwrap());
        Journal::new(&mut doc).redo().unwrap();
        assert_eq!(rotation(&doc, 1), 90);
        Journal::new(&mut doc).undo().unwrap();
        Journal::new(&mut doc).undo().unwrap();
        assert_eq!(rotation(&doc, 0), 0);
    }

    #[test]
    fn a_reopened_snapshot_has_the_changes_after_the_original_bytes() {
        let dir = out_dir("reopen");
        let (original, mut doc) = edited(&dir);
        let snap = dir.join("snap.pdf");
        save_snapshot(&mut doc, &snap).unwrap();
        drop(doc);

        let original_bytes = std::fs::read(&original).unwrap();
        assert!(std::fs::read(&snap).unwrap().starts_with(&original_bytes));
        let reopened = open_pdf_shared(&snap).unwrap();
        assert_eq!((rotation(&reopened, 0), rotation(&reopened, 1)), (90, 90));
        assert!(!has_unsaved_changes(&reopened).unwrap());

        // Saved incrementally elsewhere, it still starts with the original bytes.
        let saved = dir.join("saved.pdf");
        save_atomic(&reopened, SaveKind::Incremental, Some(&snap), &saved).unwrap();
        assert!(std::fs::read(&saved).unwrap().starts_with(&original_bytes));
    }

    #[test]
    fn a_repaired_document_cannot_be_snapshotted() {
        let dir = out_dir("repaired");
        let path = dir.join("damaged.pdf");
        let doc = sample_document(&SampleSpec::default()).unwrap();
        save_atomic(&doc, SaveKind::Full, None, &path).unwrap();
        // Break the cross-reference offsets so MuPDF repairs the file on open.
        let mut bytes = std::fs::read(&path).unwrap();
        let at = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
        bytes.truncate(at);
        bytes.extend_from_slice(b"startxref\n999999\n%%EOF\n");
        std::fs::write(&path, bytes).unwrap();
        let mut doc = open_pdf_shared(&path).unwrap();
        assert!(crate::ffi::was_repaired(&doc).unwrap());
        assert!(save_snapshot(&mut doc, &dir.join("snap.pdf")).is_err());
    }
}
