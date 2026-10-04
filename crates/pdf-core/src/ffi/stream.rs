//! Opening documents from a file handle that still lets the file be replaced (ADR 0003).
//!
//! MuPDF's own `fz_open_file` keeps the file open without `FILE_SHARE_DELETE` on Windows,
//! so an atomic save cannot rename a new file over the open one. Here the file is opened
//! by Rust with read and delete sharing (no write sharing, so nobody can change the bytes
//! under MuPDF in place), and the shim reads through that handle. After a replace, the
//! handle keeps reading the old file's data until the document is dropped.

use std::ffi::c_int;
use std::fs::{File, OpenOptions};
use std::path::Path;

use mupdf::Context;
use mupdf::pdf::PdfDocument;
use mupdf_sys::{fz_context, pdf_document};

use super::{LectrixError, check};
use crate::error::Result;

unsafe extern "C" {
    fn lectrix_pdf_open_os_handle(
        ctx: *mut fz_context,
        handle: isize,
        out: *mut *mut pdf_document,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_was_repaired(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        repaired: *mut c_int,
        err: *mut LectrixError,
    ) -> c_int;
}

/// Opens `path` for reading so that other programs (and Lectrix's own atomic save) can still
/// rename or delete it, but not write into it.
fn open_shared(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        const FILE_SHARE_DELETE: u32 = 0x4;
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE);
    }
    options.open(path)
}

/// Releases ownership of the OS handle (Windows `HANDLE` or POSIX file descriptor).
fn into_raw(file: File) -> isize {
    #[cfg(windows)]
    {
        use std::os::windows::io::IntoRawHandle;
        file.into_raw_handle() as isize
    }
    #[cfg(unix)]
    {
        use std::os::unix::io::IntoRawFd;
        file.into_raw_fd() as isize
    }
}

/// Opens a PDF through a share-delete file handle.
pub fn open_pdf_shared(path: &Path) -> Result<PdfDocument> {
    let file = open_shared(path)?;
    let handle = into_raw(file);
    let ctx = Context::get().as_raw_ptr();
    let mut raw: *mut pdf_document = std::ptr::null_mut();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; `handle` is an owned, open, readable handle
    // whose ownership passes to the shim (it closes it on failure or when the document is
    // dropped); `raw` and `err` are live locals.
    let rc = unsafe { lectrix_pdf_open_os_handle(ctx, handle, &mut raw, &mut err) };
    check(rc, err)?;
    if raw.is_null() {
        return Err(crate::Error::NotPdf);
    }
    // SAFETY: the shim returned a document carrying one reference that we now own, opened
    // with this thread's context.
    Ok(unsafe { PdfDocument::from_raw_owned(raw) })
}

/// True if MuPDF had to repair the file's cross-reference table (such files cannot be
/// saved incrementally; AGENTS.md section 5.4).
pub fn was_repaired(doc: &PdfDocument) -> Result<bool> {
    let ctx = Context::get().as_raw_ptr();
    let mut repaired: c_int = 0;
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; `doc` is alive for the call (PdfDocument is
    // not Send, so this is its owning thread); out-pointers are live locals.
    let rc =
        unsafe { lectrix_pdf_was_repaired(ctx, doc.as_raw_pdf_ptr(), &mut repaired, &mut err) };
    check(rc, err)?;
    Ok(repaired != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::{SaveKind, save_atomic};
    use crate::testgen::{SampleSpec, sample_document};

    fn out_dir() -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/pdf-core/ffi-stream");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn opens_and_reads_pages() {
        let path = out_dir().join("open.pdf");
        let doc = sample_document(&SampleSpec {
            pages: 4,
            ..SampleSpec::default()
        })
        .unwrap();
        save_atomic(&doc, SaveKind::Full, None, &path).unwrap();

        let opened = open_pdf_shared(&path).unwrap();
        assert_eq!(opened.page_count().unwrap(), 4);
        assert!(!was_repaired(&opened).unwrap());
        let text = opened
            .load_page(3)
            .unwrap()
            .to_text_page(mupdf::TextPageFlags::empty())
            .unwrap()
            .to_text()
            .unwrap();
        assert!(text.contains("Page 4"), "{text}");
    }

    #[test]
    fn open_file_can_be_replaced_and_still_read() {
        let dir = out_dir();
        let path = dir.join("replace.pdf");
        let first = sample_document(&SampleSpec {
            pages: 2,
            ..SampleSpec::default()
        })
        .unwrap();
        save_atomic(&first, SaveKind::Full, None, &path).unwrap();
        let opened = open_pdf_shared(&path).unwrap();

        // Replace the open file by rename, as an atomic save does.
        let second = sample_document(&SampleSpec {
            pages: 5,
            ..SampleSpec::default()
        })
        .unwrap();
        save_atomic(&second, SaveKind::Full, None, &path).unwrap();

        // The open document still reads the old bytes; the file on disk is the new one.
        assert_eq!(opened.page_count().unwrap(), 2);
        assert!(opened.load_page(1).is_ok());
        drop(opened);
        assert_eq!(open_pdf_shared(&path).unwrap().page_count().unwrap(), 5);
    }

    #[test]
    fn rejects_non_pdf_and_missing_files() {
        let dir = out_dir();
        let junk = dir.join("junk.pdf");
        std::fs::write(&junk, b"this is not a pdf").unwrap();
        assert!(open_pdf_shared(&junk).is_err());
        assert!(open_pdf_shared(&dir.join("missing.pdf")).is_err());
    }
}
