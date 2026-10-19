//! How a document is encrypted, read from its security handler, and what its file declares
//! (Protect and the document's properties, sections 6.9 and 6.10). The `mupdf` crate gives
//! only the permissions as they apply now, which are every permission once the permissions
//! password opened the file.

use std::ffi::{CStr, c_char, c_int};

use mupdf::Context;
use mupdf::pdf::PdfDocument;
use mupdf_sys::{fz_context, pdf_document};

use super::{LectrixError, check};
use crate::error::Result;

unsafe extern "C" {
    fn lectrix_pdf_file_facts(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        version: *mut c_int,
        linearized: *mut c_int,
        err: *mut LectrixError,
    ) -> c_int;
    fn lectrix_pdf_crypt_info(
        ctx: *mut fz_context,
        doc: *mut pdf_document,
        encrypted: *mut c_int,
        revision: *mut c_int,
        length: *mut c_int,
        permissions: *mut c_int,
        method: *mut c_char,
        method_len: usize,
        err: *mut LectrixError,
    ) -> c_int;
}

/// A document's encryption as its file has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptInfo {
    /// The security handler's revision (`/R`): 6 for AES-256 as Acrobat X and later write.
    pub revision: i32,
    /// The key length in bits.
    pub length: i32,
    /// The file's `/P`, whichever password opened it.
    pub permissions: i32,
    /// The cipher: "AES", "RC4", or "Unknown".
    pub method: String,
}

/// The PDF version `doc`'s header declares (17 for 1.7), and whether the file starts with a
/// linearization dictionary (the document's properties, section 6.10).
pub fn file_facts(doc: &PdfDocument) -> Result<(i32, bool)> {
    let ctx = Context::get().as_raw_ptr();
    let mut version: c_int = 0;
    let mut linearized: c_int = 0;
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; `doc` is alive for the call (PdfDocument is
    // not Send, so this is its owning thread); the out-pointers are live locals.
    let rc = unsafe {
        lectrix_pdf_file_facts(
            ctx,
            doc.as_raw_pdf_ptr(),
            &mut version,
            &mut linearized,
            &mut err,
        )
    };
    check(rc, err)?;
    Ok((version, linearized != 0))
}

/// How `doc` is encrypted, or `None` for a document without encryption.
pub fn crypt_info(doc: &PdfDocument) -> Result<Option<CryptInfo>> {
    let ctx = Context::get().as_raw_ptr();
    let mut encrypted: c_int = 0;
    let mut revision: c_int = 0;
    let mut length: c_int = 0;
    let mut permissions: c_int = 0;
    let mut method: [c_char; 16] = [0; 16];
    let mut err = LectrixError::new();
    // SAFETY: `ctx` is this thread's context; `doc` is alive for the call (PdfDocument is
    // not Send, so this is its owning thread); the out-pointers are live locals, and
    // `method` is as long as the length passed with it.
    let rc = unsafe {
        lectrix_pdf_crypt_info(
            ctx,
            doc.as_raw_pdf_ptr(),
            &mut encrypted,
            &mut revision,
            &mut length,
            &mut permissions,
            method.as_mut_ptr(),
            method.len(),
            &mut err,
        )
    };
    check(rc, err)?;
    if encrypted == 0 {
        return Ok(None);
    }
    // SAFETY: the shim NUL-terminates `method` (fz_strlcpy), and it was zeroed before.
    let method = unsafe { CStr::from_ptr(method.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    Ok(Some(CryptInfo {
        revision,
        length,
        permissions,
        method,
    }))
}

#[cfg(test)]
mod tests {
    use mupdf::pdf::{Encryption, PdfWriteOptions, Permission};

    use super::*;
    use crate::ffi::open_pdf_shared;
    use crate::save::{SaveKind, save_atomic};
    use crate::testgen::{SampleSpec, sample_document};

    fn out_dir() -> std::path::PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-output/pdf-core/ffi-crypt");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_the_files_own_permissions_whichever_password_opened_it() {
        let path = out_dir().join("aes.pdf");
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let mut options = PdfWriteOptions::default();
        options
            .set_encryption(Encryption::Aes256)
            .set_permissions(Permission::PRINT | Permission::COPY)
            .set_user_password("")
            .set_owner_password("owner");
        doc.save_with_options(path.to_str().unwrap(), options)
            .unwrap();

        let mut opened = open_pdf_shared(&path).unwrap();
        assert!(!opened.needs_password().unwrap());
        let info = crypt_info(&opened).unwrap().unwrap();
        assert_eq!((info.revision, info.length), (6, 256));
        assert_eq!(info.method, "AES");
        let named = Permission::all().bits() as i32;
        assert_eq!(
            info.permissions & named,
            (Permission::PRINT | Permission::COPY).bits() as i32
        );
        // The permissions password lifts the restrictions, not the file's own /P.
        assert!(opened.authenticate("owner").unwrap());
        assert!(opened.permissions().contains(Permission::all()));
        assert_eq!(crypt_info(&opened).unwrap().unwrap(), info);
    }

    #[test]
    fn a_file_without_encryption_has_none() {
        let path = out_dir().join("plain.pdf");
        let doc = sample_document(&SampleSpec::default()).unwrap();
        save_atomic(&doc, SaveKind::Full, None, &path).unwrap();
        assert_eq!(crypt_info(&open_pdf_shared(&path).unwrap()).unwrap(), None);
    }
}
