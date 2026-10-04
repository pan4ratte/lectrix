//! Keeping the trailer's /Size true after undo (see `lectrix_pdf_trim_unused_objects` in
//! the shim).

use std::ffi::c_int;

use mupdf::pdf::PdfDocument;
use mupdf_sys::pdf_document;

unsafe extern "C" {
    fn lectrix_pdf_trim_unused_objects(doc: *mut pdf_document) -> c_int;
}

/// Drops the never-used object numbers an undone step left at the end of `doc`'s edit
/// section, so an incremental save writes a /Size that matches the file. Changes nothing
/// in the document's content or history. Returns how many were dropped.
pub fn trim_unused_objects(doc: &PdfDocument) -> usize {
    // SAFETY: `doc` is a live document owned by this thread (PdfDocument is not Send), and
    // no MuPDF call is in progress on it. The shim calls no MuPDF function (nothing can
    // throw); it only shortens the edit section's length over entries that hold nothing,
    // which MuPDF regrows on demand. It takes a mutable pointer, as the crate's own
    // `save_with_options` does from `&self`.
    let dropped = unsafe { lectrix_pdf_trim_unused_objects(doc.as_raw_pdf_ptr()) };
    usize::try_from(dropped).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::Journal;
    use crate::testgen::{SampleSpec, sample_document};
    use mupdf::pdf::PdfObject;

    fn add_objects(doc: &mut PdfDocument, n: usize) -> Vec<i32> {
        doc.begin_operation("Add objects").unwrap();
        let mut nums = Vec::new();
        for i in 0..n {
            let obj = doc
                .add_object(&PdfObject::new_int(i32::try_from(i).unwrap()).unwrap())
                .unwrap();
            nums.push(obj.as_indirect().unwrap());
        }
        // Referenced from the catalog, so the objects are part of the document.
        let mut list = doc.new_array().unwrap();
        for &num in &nums {
            list.array_push(doc.new_indirect(num, 0).unwrap()).unwrap();
        }
        doc.catalog()
            .unwrap()
            .dict_put("LectrixTest", list)
            .unwrap();
        doc.end_operation().unwrap();
        nums
    }

    #[test]
    fn trims_what_undo_left_and_redo_still_works() {
        let mut doc = sample_document(&SampleSpec::default()).unwrap();
        Journal::new(&mut doc).enable().unwrap();
        assert_eq!(trim_unused_objects(&doc), 0, "no edits, nothing to trim");

        let before = doc.count_objects().unwrap();
        let nums = add_objects(&mut doc, 3);
        assert_eq!(trim_unused_objects(&doc), 0, "the new objects are in use");
        Journal::new(&mut doc).undo().unwrap();
        assert_eq!(trim_unused_objects(&doc), 3);
        assert_eq!(doc.count_objects().unwrap(), before);
        assert_eq!(trim_unused_objects(&doc), 0);

        // Redo brings the objects back under their own numbers.
        Journal::new(&mut doc).redo().unwrap();
        for (i, &num) in nums.iter().enumerate() {
            let obj = doc
                .new_indirect(num, 0)
                .unwrap()
                .resolve()
                .unwrap()
                .unwrap();
            assert_eq!(obj.as_int().unwrap(), i32::try_from(i).unwrap());
        }

        // A new step after an undo and a trim numbers its objects from the trimmed end.
        Journal::new(&mut doc).undo().unwrap();
        trim_unused_objects(&doc);
        let again = add_objects(&mut doc, 1);
        assert_eq!(u32::try_from(again[0]).unwrap(), before);
    }
}
