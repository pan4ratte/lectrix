//! Removing a file's protection, its password and its restrictions (the start screen's
//! Unlock, ADR 0025). Whatever password opens the file is enough, restrictions included,
//! and a file with restrictions alone needs none. The file is written whole, without
//! encryption, to a new file through an atomic replace; the original is only read.

use std::path::Path;

use mupdf::pdf::PdfDocument;

use crate::error::{Error, Result};
use crate::ffi::open_pdf_shared;
use crate::save::{SaveKind, save_atomic};

/// What a protected file is, once opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Protection {
    /// The file is encrypted: it has a password, restrictions, or both. False means there
    /// is nothing to remove.
    pub encrypted: bool,
    /// At least one signature field holds a signature, which a file written whole no
    /// longer matches.
    pub signed: bool,
}

/// Opens `path` with `password` and says how it is protected. Without a password, a file
/// that needs one gives [`Error::PasswordRequired`]; a password that does not open it,
/// [`Error::WrongPassword`].
pub fn inspect(path: &Path, password: Option<&str>) -> Result<Protection> {
    let doc = load(path, password)?;
    Ok(Protection {
        encrypted: is_encrypted(&doc)?,
        signed: crate::docinfo::is_signed(&doc)?,
    })
}

/// Writes `path`, opened with `password`, to `target` without its encryption. `target`
/// may be `path` itself: it is replaced in one rename once the new file is written.
pub fn remove_protection(path: &Path, password: Option<&str>, target: &Path) -> Result<()> {
    let doc = load(path, password)?;
    save_atomic(&doc, SaveKind::Decrypted, None, target)?;
    Ok(())
}

pub(crate) fn load(path: &Path, password: Option<&str>) -> Result<PdfDocument> {
    let mut doc = open_pdf_shared(path)?;
    if doc.needs_password()? {
        let Some(password) = password else {
            return Err(Error::PasswordRequired);
        };
        if !doc.authenticate(password)? {
            return Err(Error::WrongPassword);
        }
    }
    crate::session::repair_if_without_pages(&mut doc)?;
    Ok(doc)
}

fn is_encrypted(doc: &PdfDocument) -> Result<bool> {
    Ok(doc.trailer()?.get_dict("Encrypt")?.is_some())
}
