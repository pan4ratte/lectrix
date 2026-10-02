#![allow(dead_code)]

use std::path::{Path, PathBuf};

use mupdf::pdf::PdfDocument;
use pdf_core::save::{SaveKind, save_atomic};
use pdf_core::testgen::{SampleSpec, sample_document};

/// `target/test-output/pdf-core/<name>` (tests never write next to corpus files).
pub fn out_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-output/pdf-core")
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn open(path: &Path) -> PdfDocument {
    PdfDocument::open(path.to_str().unwrap()).unwrap()
}

/// Generates a sample file on disk and returns its path.
pub fn sample_file(dir: &Path, name: &str, spec: SampleSpec) -> PathBuf {
    let path = dir.join(name);
    let doc = sample_document(&spec).unwrap();
    save_atomic(&doc, SaveKind::Full, None, &path).unwrap();
    path
}

/// Saves incrementally from `original` to `out` and reopens the result.
pub fn save_and_reopen(doc: &PdfDocument, original: &Path, out: &Path) -> PdfDocument {
    save_atomic(doc, SaveKind::Incremental, Some(original), out).unwrap();
    open(out)
}
