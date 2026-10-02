//! Thin safe wrappers over MuPDF APIs the `mupdf` crate does not cover.
//!
//! Each call goes through the C shim in `shim.c`, which catches MuPDF's longjmp-based
//! errors before they can reach Rust frames.

mod journal;

pub use journal::{Journal, JournalState};

use std::ffi::{CStr, c_char};

use crate::error::{Error, Result};

#[repr(C)]
pub(crate) struct FolioError {
    code: i32,
    message: [c_char; 256],
}

impl FolioError {
    pub(crate) fn new() -> Self {
        Self {
            code: 0,
            message: [0; 256],
        }
    }

    pub(crate) fn into_error(self) -> Error {
        // SAFETY: the shim always NUL-terminates `message` (fz_strlcpy), and `new` zeroes it.
        let message = unsafe { CStr::from_ptr(self.message.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        Error::Ffi {
            code: self.code,
            message,
        }
    }
}

unsafe extern "C" {
    fn folio_mupdf_version() -> *const c_char;
    fn folio_mupdf_headers_match_library() -> std::ffi::c_int;
}

/// MuPDF version the shim was compiled against (the vendored headers).
pub fn shim_mupdf_version() -> String {
    // SAFETY: returns a pointer to the static string literal FZ_VERSION.
    unsafe { CStr::from_ptr(folio_mupdf_version()) }
        .to_string_lossy()
        .into_owned()
}

/// Fails if the vendored headers do not match the linked MuPDF (from `mupdf-sys`).
pub fn check_version() -> Result<()> {
    // SAFETY: creates and drops a standalone context; no preconditions.
    if unsafe { folio_mupdf_headers_match_library() } == 1 {
        Ok(())
    } else {
        Err(Error::VersionMismatch {
            headers: shim_mupdf_version(),
            library: "a different version (see MuPDF's stderr message)".to_owned(),
        })
    }
}

/// Turns a shim return code into a `Result`.
pub(crate) fn check(rc: i32, err: FolioError) -> Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(err.into_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_match_linked_library() {
        check_version().unwrap();
        assert_eq!(shim_mupdf_version(), "1.27.2");
    }
}
