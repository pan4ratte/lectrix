use thiserror::Error;

/// Errors produced by `pdf-core`.
#[derive(Debug, Error)]
pub enum Error {
    #[error("MuPDF error: {0}")]
    MuPdf(#[from] mupdf::Error),
    /// An error raised inside the FFI shim (`code` is MuPDF's FZ_ERROR_* value).
    #[error("MuPDF error {code}: {message}")]
    Ffi { code: i32, message: String },
    #[error("the document is not a PDF")]
    NotPdf,
    #[error("page {0} does not exist")]
    PageOutOfRange(usize),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("MuPDF version mismatch: headers {headers}, library {library}")]
    VersionMismatch { headers: String, library: String },
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("{} is open in another program", .0.display())]
    TargetLocked(std::path::PathBuf),
    #[error("the document's worker thread has stopped")]
    ActorGone,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
