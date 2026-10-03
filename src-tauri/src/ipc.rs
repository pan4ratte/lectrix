//! IPC payloads. TypeScript types are generated from these by ts-rs into
//! `src/lib/ipc/generated/` (run `cargo test -p folio`); never write them by hand.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use pdf_core::session as core;

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

impl From<core::PageSize> for PageSize {
    fn from(p: core::PageSize) -> Self {
        PageSize {
            width: p.width,
            height: p.height,
        }
    }
}

/// What the document allows and how it is saved (AGENTS.md section 5.4).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentFlags {
    pub encrypted: bool,
    /// Warn before the first edit: edits show as changes made after signing.
    pub signed: bool,
    /// The file was damaged and repaired on open; the next save is a full save.
    pub repaired: bool,
    /// Pages may be rotated.
    pub can_assemble: bool,
    pub can_annotate: bool,
    /// Text may be copied.
    pub can_copy: bool,
}

impl From<pdf_core::docinfo::DocumentFlags> for DocumentFlags {
    fn from(f: pdf_core::docinfo::DocumentFlags) -> Self {
        DocumentFlags {
            encrypted: f.encrypted,
            signed: f.signed,
            repaired: f.repaired,
            can_assemble: f.can_assemble,
            can_annotate: f.can_annotate,
            can_copy: f.can_copy,
        }
    }
}

/// Undo position and save state, returned by every command that can change them.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentState {
    /// Content revision: part of every page-image URL, so images refresh on change.
    #[ts(type = "number")]
    pub revision: u64,
    pub dirty: bool,
    pub undo_name: Option<String>,
    pub redo_name: Option<String>,
}

impl From<core::DocumentState> for DocumentState {
    fn from(s: core::DocumentState) -> Self {
        DocumentState {
            revision: s.revision,
            dirty: s.dirty,
            undo_name: s.undo_name,
            redo_name: s.redo_name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ZoomMode {
    FitWidth,
    FitPage,
    Custom,
}

/// Where the user was in a document; remembered per file in app data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewState {
    /// Physical page index (0-based).
    pub page: u32,
    /// How far down the page the top of the view was, from 0 to 1.
    pub offset: f32,
    /// Zoom factor (1 = 100%).
    pub zoom: f32,
    pub zoom_mode: ZoomMode,
    /// View rotation in degrees (0, 90, 180, 270). Does not change the document.
    pub rotation: u16,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentInfo {
    pub id: u32,
    /// File name shown in the tab.
    pub name: String,
    /// Full path, shown as the tab's tooltip.
    pub path: String,
    pub pages: Vec<PageSize>,
    /// One label per page, or null when the document has no page labels.
    pub labels: Option<Vec<String>>,
    pub flags: DocumentFlags,
    pub state: DocumentState,
    /// Where the user left off last time, if this file was opened before.
    pub view: Option<ViewState>,
    /// Time to open the file and read page geometry, in milliseconds.
    pub open_ms: f64,
}

/// The outcome of opening one file.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum OpenResult {
    Opened {
        document: DocumentInfo,
    },
    /// The file is already open in a tab; switch to it.
    AlreadyOpen {
        id: u32,
    },
    /// The file is encrypted. Ask for the password and call `unlock_document`.
    NeedsPassword {
        token: u32,
        name: String,
        /// True after a wrong password.
        retry: bool,
    },
    Failed {
        name: String,
        error: AppError,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangedPage {
    pub index: u32,
    pub size: PageSize,
}

/// What a mutating command changed (AGENTS.md section 3).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentChange {
    pub state: DocumentState,
    pub changed_pages: Vec<ChangedPage>,
    /// True when `labels` holds new labels (which may be null: labels removed).
    pub labels_changed: bool,
    pub labels: Option<Vec<String>>,
}

impl From<core::DocumentChange> for DocumentChange {
    fn from(c: core::DocumentChange) -> Self {
        let (labels_changed, labels) = match c.labels {
            Some(labels) => (true, labels),
            None => (false, None),
        };
        DocumentChange {
            state: c.state.into(),
            changed_pages: c
                .changed_pages
                .into_iter()
                .map(|(index, size)| ChangedPage {
                    index: u32::try_from(index).unwrap_or(u32::MAX),
                    size: size.into(),
                })
                .collect(),
            labels_changed,
            labels,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SaveResult {
    pub state: DocumentState,
    /// The document's file name and path after saving (they change with Save As).
    pub name: String,
    pub path: String,
    /// An incremental save was not possible (the file had been repaired), so the whole
    /// file was rewritten.
    pub fell_back_to_full: bool,
}

/// A change to a document, sent from the frontend.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum OperationInput {
    /// Turn pages by `degrees` (multiple of 90, positive is clockwise).
    RotatePages { pages: Vec<u32>, degrees: i32 },
}

impl From<OperationInput> for pdf_core::ops::Operation {
    fn from(op: OperationInput) -> Self {
        match op {
            OperationInput::RotatePages { pages, degrees } => {
                pdf_core::ops::Operation::RotatePages {
                    pages: pages.into_iter().map(|p| p as usize).collect(),
                    degrees,
                }
            }
        }
    }
}

/// One line of text with one box per character (`boxes` holds 4 numbers per character:
/// x0, y0, x1, y1 in points, view space at zoom 1).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TextLine {
    pub text: String,
    pub boxes: Vec<f32>,
    pub bbox: [f32; 4],
    pub block: u32,
    pub vertical: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageText {
    pub page: u32,
    #[ts(type = "number")]
    pub revision: u64,
    pub lines: Vec<TextLine>,
}

impl PageText {
    pub fn new(page: usize, revision: u64, text: pdf_core::text::PageText) -> Self {
        PageText {
            page: u32::try_from(page).unwrap_or(u32::MAX),
            revision,
            lines: text
                .lines
                .into_iter()
                .map(|l| TextLine {
                    text: l.text,
                    boxes: l.boxes.into_iter().flatten().collect(),
                    bbox: l.bbox,
                    block: l.block,
                    vertical: l.vertical,
                })
                .collect(),
        }
    }
}

/// Search hits on one page. Each hit is a list of quads (8 numbers each: upper-left,
/// upper-right, lower-left, lower-right corners, view space at zoom 1).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageHits {
    pub page: u32,
    pub hits: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchChunk {
    #[ts(type = "number")]
    pub revision: u64,
    pub pages: Vec<PageHits>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecentFile {
    /// Position in the list; pass it to `open_recent`.
    pub index: u32,
    pub name: String,
    pub folder: String,
    /// False when the file is no longer where it was.
    pub exists: bool,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Backdrop {
    Mica,
    Solid,
}

/// How page images travel to the webview (docs/progress.md, Phase 1 measurements).
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ImageFormat {
    /// Raw RGBA drawn with putImageData (AGENTS.md section 3).
    Rgba,
    /// PNG decoded by the webview.
    Png,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartupInfo {
    /// Milliseconds from the start of `main` to the frontend's first mount.
    pub main_to_ready_ms: f64,
    pub backdrop: Backdrop,
    /// The system accent color as #rrggbb, if any.
    pub accent_color: Option<String>,
    /// Set by the FOLIO_PERF environment variable: the frontend runs its scripted
    /// performance measurements (tests/perf/measure.ps1).
    pub perf_mode: bool,
    /// Page image format; FOLIO_IMAGE_FORMAT=png switches from the default raw RGBA.
    pub image_format: ImageFormat,
}

/// Emitted as `file-changed` when an open document's file changes on disk.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileChangedEvent {
    pub id: u32,
    /// False when the file was moved or deleted.
    pub exists: bool,
}

/// Errors shown to the user: plain language plus a suggested next step (section 8).
#[derive(Debug, Clone, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[error("{message}")]
pub struct AppError {
    pub message: String,
    pub suggestion: Option<String>,
    /// A machine-readable reason the UI can act on (for example offering Save As).
    pub code: ErrorCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorCode {
    General,
    /// The target file is locked by another program.
    FileLocked,
    NotPermitted,
    NothingToUndo,
    NothingToRedo,
    DocumentClosed,
}

impl AppError {
    pub fn new(message: impl Into<String>, suggestion: Option<&str>) -> Self {
        AppError {
            message: message.into(),
            suggestion: suggestion.map(str::to_owned),
            code: ErrorCode::General,
        }
    }

    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.code = code;
        self
    }

    pub fn document_closed() -> Self {
        AppError::new(
            "This document is no longer open.",
            Some("Open it again from File > Open."),
        )
        .with_code(ErrorCode::DocumentClosed)
    }

    pub fn bad_state() -> Self {
        AppError::new("The app is in a bad state.", Some("Restart Folio."))
    }
}

impl From<pdf_core::Error> for AppError {
    fn from(e: pdf_core::Error) -> Self {
        use pdf_core::Error as E;
        // Details go to the log; the user sees a plain sentence.
        crate::applog::warn(format!("{e:?}"));
        match e {
            E::NotPdf => AppError::new(
                "This file isn't a PDF, or it is too damaged to open.",
                Some("Check that you picked the right file."),
            ),
            E::Io(_) => AppError::new(
                "The file couldn't be read or written.",
                Some("Check that it still exists and that you have permission to change it."),
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
            )
            .with_code(ErrorCode::FileLocked),
            E::NotPermitted => AppError::new(
                "This document's security settings don't allow that change.",
                Some("Ask the document's author for an unrestricted copy."),
            )
            .with_code(ErrorCode::NotPermitted),
            E::NothingToUndo => {
                AppError::new("There is nothing to undo.", None).with_code(ErrorCode::NothingToUndo)
            }
            E::NothingToRedo => {
                AppError::new("There is nothing to redo.", None).with_code(ErrorCode::NothingToRedo)
            }
            E::PasswordRequired | E::WrongPassword => AppError::new(
                "This document needs a password.",
                Some("Open it again and enter the password."),
            ),
            E::ActorGone => AppError::document_closed(),
            _ => AppError::new(
                "Something went wrong while working with this document.",
                Some("Try again. If it keeps happening, the app log has details."),
            ),
        }
    }
}
