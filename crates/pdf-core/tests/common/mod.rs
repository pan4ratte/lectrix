#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::RwLock;

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

/// Serializes MuPDF file writes against child processes.
///
/// MuPDF writes files through the C runtime's `fopen`, whose handles are inheritable on
/// Windows. A child process spawned while such a handle is open (`qpdf` in another test
/// thread) inherits it and keeps the file open, so a rename or share-delete open of that
/// file fails until the child exits. Writes take the lock shared; [`qpdf_check`] takes it
/// exclusively. (Folio itself spawns no child processes; see ADR 0003.)
static SPAWN_LOCK: RwLock<()> = RwLock::new(());

/// Runs `f`, which writes files through MuPDF, without racing a child process spawn.
pub fn writing<T>(f: impl FnOnce() -> T) -> T {
    let _guard = SPAWN_LOCK.read().unwrap_or_else(|e| e.into_inner());
    f()
}

/// Generates a sample file on disk and returns its path.
pub fn sample_file(dir: &Path, name: &str, spec: SampleSpec) -> PathBuf {
    let path = dir.join(name);
    let doc = sample_document(&spec).unwrap();
    writing(|| save_atomic(&doc, SaveKind::Full, None, &path)).unwrap();
    path
}

/// Saves incrementally from `original` to `out` and reopens the result.
pub fn save_and_reopen(doc: &PdfDocument, original: &Path, out: &Path) -> PdfDocument {
    writing(|| save_atomic(doc, SaveKind::Incremental, Some(original), out)).unwrap();
    open(out)
}

/// Runs `qpdf --check` on `path`; any warning fails the test (AGENTS.md section 9).
/// qpdf is required in CI. Locally, the check is skipped with a note if qpdf is missing.
pub fn qpdf_check(path: &Path) {
    let _guard = SPAWN_LOCK.write().unwrap_or_else(|e| e.into_inner());
    let output = match std::process::Command::new(qpdf_program())
        .arg("--check")
        .arg(path)
        .output()
    {
        Ok(output) => output,
        Err(e) if std::env::var_os("CI").is_none() => {
            eprintln!("qpdf not found ({e}); skipping the structural check");
            return;
        }
        Err(e) => panic!("qpdf is required in CI: {e}"),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stdout.contains("WARNING") && stderr.trim().is_empty(),
        "qpdf --check {} failed:\n{stdout}\n{stderr}",
        path.display()
    );
}

fn qpdf_program() -> PathBuf {
    if let Some(p) = std::env::var_os("QPDF") {
        return PathBuf::from(p);
    }
    // winget installs qpdf without adding it to PATH.
    if cfg!(windows)
        && let Ok(entries) = std::fs::read_dir("C:/Program Files")
    {
        for entry in entries.flatten() {
            let candidate = entry.path().join("bin/qpdf.exe");
            if entry.file_name().to_string_lossy().starts_with("qpdf") && candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from("qpdf")
}

/// `qpdf --check` severity for real-world files that may already have problems: 0 clean,
/// 1 warnings, 2 errors. `None` if qpdf is not installed.
pub fn qpdf_severity(path: &Path) -> Option<u8> {
    let _guard = SPAWN_LOCK.write().unwrap_or_else(|e| e.into_inner());
    let output = std::process::Command::new(qpdf_program())
        .arg("--check")
        .arg(path)
        .output()
        .ok()?;
    // qpdf exits with 0 (clean), 3 (warnings) or 2 (errors).
    Some(match output.status.code() {
        Some(0) => 0,
        Some(3) => 1,
        _ => 2,
    })
}
