//! Writing objects created in the journal operation in progress.

use std::ffi::{c_int, c_uchar};

use mupdf::Context;
use mupdf::pdf::{PdfDocument, PdfObject};
use mupdf_sys::{fz_context, pdf_document, pdf_obj};

use super::{LectrixError, check};
use crate::error::{Error, Result};

unsafe extern "C" {
    fn lectrix_pdf_set_new_stream(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        num: c_int,
        data: *const c_uchar,
        len: usize,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_obj_is_incremental(
        ctx: *mut fz_context,
        obj: *mut pdf_obj,
        incremental: *mut c_int,
        err: *mut LectrixError,
    ) -> c_int;
}

/// True if `obj`, an indirect object, was created or changed since its document was
/// opened (MuPDF's incremental section). One that was not came from the file, where other
/// objects may refer to it too: an appearance stream shared by several annotations.
pub fn is_incremental(obj: &PdfObject) -> Result<bool> {
    let ctx = Context::get().as_raw_ptr();
    let mut incremental = 0;
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context and `obj` a live object of a document owned
    // by this thread, only read; `incremental` and `err` are live locals.
    let rc = unsafe {
        lectrix_pdf_obj_is_incremental(ctx, obj.as_raw_ptr(), &mut incremental, &mut err)
    };
    check(rc, err)?;
    Ok(incremental != 0)
}

/// Sets the raw (still encoded) data of `stream`, an indirect stream object of `doc` that
/// was created in the journal operation in progress (or `doc` has no journal). Unlike the
/// crate's `write_raw_stream_buffer`, this costs the same however many objects the
/// operation has already changed (see the shim).
pub fn set_new_stream(doc: &mut PdfDocument, stream: &PdfObject, data: &[u8]) -> Result<()> {
    if !stream.is_indirect()? {
        return Err(Error::InvalidArgument(
            "a stream must be an indirect object".into(),
        ));
    }
    let num = stream.as_indirect()?;
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context and `doc` belongs to this thread (PdfDocument
    // is not Send); `data` is valid for `len` bytes and only read (the shim copies it);
    // `err` is a live local. The shim restores the document's journal before returning.
    let rc = unsafe {
        lectrix_pdf_set_new_stream(
            ctx,
            doc.as_raw_pdf_ptr(),
            num,
            data.as_ptr(),
            data.len(),
            &mut err,
        )
    };
    check(rc, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::Journal;

    #[test]
    fn objects_from_the_file_are_not_incremental() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/ffi-incremental");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.pdf");
        let doc = crate::testgen::sample_document(&crate::testgen::SampleSpec::default()).unwrap();
        crate::save::save_atomic(&doc, crate::save::SaveKind::Full, None, &path).unwrap();
        let mut doc = PdfDocument::open(path.to_str().unwrap()).unwrap();
        let page = doc.find_page(0).unwrap();
        assert!(!is_incremental(&page).unwrap());
        let made = doc
            .add_object(&doc.new_object_from_str("<< /New true >>").unwrap())
            .unwrap();
        assert!(is_incremental(&made).unwrap());
    }

    #[test]
    fn new_streams_are_undone_and_redone_with_their_object() {
        let mut doc = PdfDocument::new();
        Journal::new(&mut doc).enable().unwrap();
        doc.begin_operation("Add stream").unwrap();
        let mut obj = doc.create_object().unwrap();
        obj.write_object(&doc.new_object_from_str("<< /Kind /Test >>").unwrap())
            .unwrap();
        set_new_stream(&mut doc, &obj, b"hello").unwrap();
        doc.end_operation().unwrap();
        let num = obj.as_indirect().unwrap();
        let read = |doc: &PdfDocument| {
            let obj = doc.new_indirect(num, 0).unwrap();
            obj.is_stream().unwrap().then(|| obj.read_stream().unwrap())
        };
        assert_eq!(read(&doc).as_deref(), Some(&b"hello"[..]));

        let mut journal = Journal::new(&mut doc);
        journal.undo().unwrap();
        assert_eq!(read(&doc), None, "undo removes the new object");
        Journal::new(&mut doc).redo().unwrap();
        assert_eq!(read(&doc).as_deref(), Some(&b"hello"[..]));
    }
}
