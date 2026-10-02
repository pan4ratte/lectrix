//! IPC payloads. TypeScript types are generated from these by ts-rs into
//! `src/lib/ipc/generated/` (run `cargo test -p folio`); never write them by hand.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentInfo {
    pub id: u32,
    /// File name shown in the tab.
    pub name: String,
    pub page_count: u32,
    pub pages: Vec<PageSize>,
    /// Time to open the file and read page geometry, in milliseconds.
    pub open_ms: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartupTimings {
    /// Milliseconds from the start of `main` to the frontend's first mount.
    pub main_to_ready_ms: f64,
}

/// Errors shown to the user: plain language plus a suggested next step (section 8).
#[derive(Debug, Clone, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[error("{message}")]
pub struct AppError {
    pub message: String,
    pub suggestion: Option<String>,
}

impl AppError {
    pub fn new(message: impl Into<String>, suggestion: Option<&str>) -> Self {
        AppError {
            message: message.into(),
            suggestion: suggestion.map(str::to_owned),
        }
    }
}

impl From<pdf_core::Error> for AppError {
    fn from(e: pdf_core::Error) -> Self {
        use pdf_core::Error as E;
        // Details go to the log; the user sees a plain sentence.
        eprintln!("[folio] {e:?}");
        match e {
            E::NotPdf => AppError::new(
                "This file isn't a PDF, or it is too damaged to open.",
                Some("Check that you picked the right file."),
            ),
            E::Io(_) => AppError::new(
                "The file couldn't be read.",
                Some("Check that it still exists and that you have permission to open it."),
            ),
            E::TargetLocked(path) => AppError::new(
                format!(
                    "{} is open in another program, so it can't be replaced.",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ),
                Some(
                    "Close it in the other program (Acrobat locks files it has open), or use Save As.",
                ),
            ),
            _ => AppError::new(
                "Something went wrong while working with this document.",
                Some("Try again. If it keeps happening, the app log has details."),
            ),
        }
    }
}
