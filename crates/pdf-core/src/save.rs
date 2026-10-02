//! Saving: incremental, full, or optimized, always through an atomic replace
//! (AGENTS.md sections 5.4 and 7).
//!
//! 1. Write the new file to a temporary file in the target's folder.
//! 2. Flush it to disk.
//! 3. Replace the target with one rename on the same volume.
//! 4. On failure, delete the temporary file and leave the target untouched.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mupdf::pdf::{PdfDocument, PdfWriteOptions};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKind {
    /// Append changes; the original bytes stay intact. Needs the original file.
    Incremental,
    /// Rewrite the whole file, keeping all objects.
    Full,
    /// Rewrite with garbage collection and stream compression ("Save As (optimized)").
    Optimized,
}

/// What actually happened, so the UI can explain a fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveOutcome {
    pub kind: SaveKind,
    /// True when an incremental save was requested but the document could not be saved
    /// incrementally (for example because MuPDF repaired it on open).
    pub fell_back_to_full: bool,
}

/// Saves `doc` to `target` atomically. `original` is the file the document was opened
/// from; incremental saves copy it first and append to the copy.
pub fn save_atomic(
    doc: &PdfDocument,
    kind: SaveKind,
    original: Option<&Path>,
    target: &Path,
) -> Result<SaveOutcome> {
    let mut outcome = SaveOutcome {
        kind,
        fell_back_to_full: false,
    };
    if kind == SaveKind::Incremental && (original.is_none() || !doc.can_be_saved_incrementally()) {
        outcome = SaveOutcome {
            kind: SaveKind::Full,
            fell_back_to_full: true,
        };
    }

    let temp = temp_path_for(target)?;
    let result = write_to(doc, outcome.kind, original, &temp).and_then(|()| {
        // FlushFileBuffers on Windows needs a handle with write access.
        OpenOptions::new().write(true).open(&temp)?.sync_all()?;
        fs::rename(&temp, target).map_err(|e| replace_error(e, target))
    });
    if result.is_err() {
        // Best effort: the temp file may not exist if writing failed early.
        let _ = fs::remove_file(&temp);
    }
    result.map(|()| outcome)
}

fn write_to(doc: &PdfDocument, kind: SaveKind, original: Option<&Path>, temp: &Path) -> Result<()> {
    let mut options = PdfWriteOptions::default();
    match kind {
        SaveKind::Incremental => {
            let original = original.ok_or_else(|| {
                Error::InvalidArgument("an incremental save needs the original file".into())
            })?;
            fs::copy(original, temp)?;
            options.set_incremental(true);
        }
        SaveKind::Full => {}
        SaveKind::Optimized => {
            options
                .set_garbage_level(3)
                .set_compress(true)
                .set_compress_images(true)
                .set_compress_fonts(true);
        }
    }
    let temp_str = temp.to_str().ok_or_else(|| {
        Error::InvalidArgument(format!("path is not valid Unicode: {}", temp.display()))
    })?;
    doc.save_with_options(temp_str, options)?;
    Ok(())
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary file next to `target` (same folder, so the final rename stays on one volume).
fn temp_path_for(target: &Path) -> Result<PathBuf> {
    let name = target
        .file_name()
        .ok_or_else(|| Error::InvalidArgument(format!("not a file path: {}", target.display())))?;
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(
        ".{}.{}-{n}.folio-tmp",
        name.to_string_lossy(),
        std::process::id()
    );
    Ok(target.with_file_name(temp_name))
}

/// Maps a failed replace to a clear error. Sharing violations usually mean another program
/// (often Acrobat) has the file open.
fn replace_error(e: io::Error, target: &Path) -> Error {
    // ERROR_SHARING_VIOLATION (32) and ERROR_LOCK_VIOLATION (33) on Windows; EACCES and
    // EBUSY elsewhere surface as PermissionDenied / ResourceBusy.
    let locked = matches!(e.raw_os_error(), Some(32 | 33))
        || matches!(
            e.kind(),
            io::ErrorKind::PermissionDenied | io::ErrorKind::ResourceBusy
        );
    if locked {
        Error::TargetLocked(target.to_path_buf())
    } else {
        Error::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_file_is_hidden_sibling() {
        let t = temp_path_for(Path::new("C:/docs/report.pdf")).unwrap();
        assert_eq!(t.parent(), Some(Path::new("C:/docs")));
        let name = t.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with(".report.pdf."));
        assert!(name.ends_with(".folio-tmp"));
        assert_ne!(t, temp_path_for(Path::new("C:/docs/report.pdf")).unwrap());
    }

    #[test]
    fn rejects_paths_without_file_name() {
        assert!(temp_path_for(Path::new("/")).is_err());
    }
}
