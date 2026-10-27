//! HTML laid out on pages and written as a PDF, by MuPDF's story engine (Compare's report,
//! section 6.13). The crate does not wrap it.

use std::ffi::{CString, c_char, c_int};
use std::path::Path;

use mupdf::Context;
use mupdf_sys::fz_context;

use super::{LectrixError, check};
use crate::error::{Error, Result};

unsafe extern "C" {
    fn lectrix_write_html_pdf(
        ctx: *mut fz_context,
        path: *const c_char,
        html: *const c_char,
        css: *const c_char,
        width: f32,
        height: f32,
        margin: f32,
        err: *mut LectrixError,
    ) -> c_int;
}

fn c_string(s: &str, what: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::InvalidArgument(format!("NUL in the {what}")))
}

/// Lays `html` (styled with `css`) out on pages of `size` points, `margin` points from each
/// edge, and writes them to a new PDF at `path`, replacing what is there.
pub fn write_html_pdf(
    path: &Path,
    html: &str,
    css: &str,
    size: (f32, f32),
    margin: f32,
) -> Result<()> {
    let s = path.to_str().ok_or_else(|| {
        Error::InvalidArgument(format!("path is not valid Unicode: {}", path.display()))
    })?;
    let (path, html, css) = (
        c_string(s, "path")?,
        c_string(html, "HTML")?,
        c_string(css, "CSS")?,
    );
    let ctx = Context::get().as_raw_ptr();
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; the three strings are NUL-terminated and
    // outlive the call; `err` points to a live LectrixError. The shim catches MuPDF's errors.
    let rc = unsafe {
        lectrix_write_html_pdf(
            ctx,
            path.as_ptr(),
            html.as_ptr(),
            css.as_ptr(),
            size.0,
            size.1,
            margin,
            &mut err,
        )
    };
    check(rc, err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_html_as_a_pdf() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-output");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("story.pdf");
        let rows: String = (0..200)
            .map(|i| format!("<tr><td>{i}</td><td>Row {i}</td></tr>"))
            .collect();
        let html = format!("<h1>Report</h1><table>{rows}</table>");
        write_html_pdf(&path, &html, "td { padding: 2pt }", (595.0, 842.0), 40.0).unwrap();
        let doc = mupdf::pdf::PdfDocument::open(path.to_str().unwrap()).unwrap();
        assert!(doc.page_count().unwrap() > 1);
    }
}
